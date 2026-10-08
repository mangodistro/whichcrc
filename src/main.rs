use std::fmt::Write as _;
use std::io::{IsTerminal, Read, Write};
use std::process::ExitCode;
use whichcrc::catalogue::{self, CATALOGUE};
use whichcrc::crc::{hex, parse_params, Engine, Params};
use whichcrc::input;
use whichcrc::search::{self, Layout, Options, Order, Outcome, RawSample};

const USAGE: &str = "\
whichcrc: work out which CRC produced the checksums in captured data

usage:
  whichcrc find [options] SAMPLE...            identify the CRC behind some samples
  whichcrc calc (-m MODEL | --all) [INPUT...]  compute a CRC
  whichcrc list [-w WIDTH] [NAME]              show the built-in catalogue

A SAMPLE is a frame in hex with the CRC in its last bytes
(\"01 03 00 00 00 0a c5 cd\"), or MESSAGE=CRC with the CRC as a hex
number (\"3132333435=546c\").

'whichcrc help find' (or calc, list) shows the options.
Exit status: 0 success, 1 find matched nothing, 2 bad usage or input.
";

const FIND_HELP: &str = "\
usage: whichcrc find [options] SAMPLE...

Each SAMPLE is a frame in hex with the CRC in its last bytes, or
MESSAGE=CRC with the CRC as a hex number. Hex may contain spaces, colons,
commas and 0x prefixes. Samples can also come one per line from -f FILE or
standard input; # starts a comment.

Every catalogue model is tried first. Given enough samples (3 of one length,
or 4 of mixed lengths) the parameters are also solved for directly, which
finds CRCs that no catalogue lists.

options:
  -w, --width N[,N...]  CRC width in bits (default: 8,16,24,32,64 for frames,
                        4 bits per hex digit for MESSAGE=CRC samples)
  -e, --endian ORDER    byte order of the CRC in frames: big, little or both
                        (default both)
  -s, --skip N[-M]      leading bytes the CRC does not cover; a range tries
                        each value (default 0)
  -t, --tail N          bytes that follow the CRC in each frame (default 0)
  -f, --file FILE       read samples from FILE, one per line ('-' for stdin)
  -b, --binary          each SAMPLE argument is a file holding one raw frame
      --text            the MESSAGE in MESSAGE=CRC is text, not hex
      --catalogue-only  do not run the solver
      --solve-only      do not match against the catalogue
      --explain         show how solved models were derived, and notes
  -h, --help            show this help
";

const CALC_HELP: &str = "\
usage: whichcrc calc (-m MODEL | --all) [--hex HEX | --text TEXT | FILE...]

MODEL is a catalogue name or alias (CRC-32, MODBUS, CRC-16/XMODEM, ...), or
a parameter line as printed by find and list, such as
  \"width=16 poly=0x1021 init=0xffff refin=false refout=false xorout=0\"
With no input given, standard input is read.

options:
  -m, --model MODEL   the CRC to compute
  -a, --all           compute every catalogue model
  -x, --hex HEX       input these bytes (repeatable)
      --text TEXT     input this text (repeatable)
  -h, --help          show this help
";

const LIST_HELP: &str = "\
usage: whichcrc list [-w WIDTH] [NAME]

Without NAME, one line per catalogue model (all 112 models of width 64 or
less from the CRC RevEng catalogue). With NAME, that model's parameters,
check and residue values, and aliases.

options:
  -w, --width N   only models of this width
  -h, --help      show this help
";

enum CliError {
    Usage(String),
    Input(String),
}

fn usage(msg: impl Into<String>) -> CliError {
    CliError::Usage(msg.into())
}

fn bad_input(msg: impl Into<String>) -> CliError {
    CliError::Input(msg.into())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(args) {
        Ok(code) => code,
        Err(CliError::Usage(m)) => {
            eprintln!("whichcrc: {m}\nRun 'whichcrc --help' for usage.");
            ExitCode::from(2)
        }
        Err(CliError::Input(m)) => {
            eprintln!("whichcrc: {m}");
            ExitCode::from(2)
        }
    }
}

fn run(args: Vec<String>) -> Result<ExitCode, CliError> {
    let Some(first) = args.first() else {
        eprint!("{USAGE}");
        return Ok(ExitCode::from(2));
    };
    let rest = args[1..].to_vec();
    match first.as_str() {
        "find" => find(rest),
        "calc" => calc(rest),
        "list" => list(rest),
        "help" | "-h" | "--help" => {
            let text = match rest.first().map(String::as_str) {
                Some("find") => FIND_HELP,
                Some("calc") => CALC_HELP,
                Some("list") => LIST_HELP,
                None => USAGE,
                Some(other) => return Err(usage(format!("no help for {other:?}"))),
            };
            emit(text);
            Ok(ExitCode::SUCCESS)
        }
        "-V" | "--version" => {
            emit(&format!("whichcrc {}\n", env!("CARGO_PKG_VERSION")));
            Ok(ExitCode::SUCCESS)
        }
        other => Err(usage(format!("unknown command {other:?}"))),
    }
}

/// Write to stdout, ignoring a closed pipe (`whichcrc list | head`).
fn emit(s: &str) {
    let _ = std::io::stdout().lock().write_all(s.as_bytes());
}

enum Arg {
    Opt(String, Option<String>),
    Pos(String),
}

/// Options as `-w 16`, `-w16`, `--width 16` or `--width=16`; `--` ends them.
struct Args {
    list: Vec<String>,
    pos: usize,
    only_positional: bool,
}

impl Args {
    fn new(list: Vec<String>) -> Args {
        Args { list, pos: 0, only_positional: false }
    }

    fn next(&mut self) -> Option<Arg> {
        let a = self.list.get(self.pos)?.clone();
        self.pos += 1;
        if self.only_positional || a == "-" || !a.starts_with('-') {
            return Some(Arg::Pos(a));
        }
        if a == "--" {
            self.only_positional = true;
            return self.next();
        }
        if let Some(long) = a.strip_prefix("--") {
            return Some(match long.split_once('=') {
                Some((k, v)) => Arg::Opt(format!("--{k}"), Some(v.to_string())),
                None => Arg::Opt(a, None),
            });
        }
        let mut chars = a.char_indices().skip(1);
        let (_, c) = chars.next().expect("at least two characters");
        let rest = chars.next().map(|(i, _)| a[i..].to_string());
        Some(Arg::Opt(format!("-{c}"), rest))
    }

    fn value(&mut self, name: &str, inline: Option<String>) -> Result<String, CliError> {
        if let Some(v) = inline {
            return Ok(v);
        }
        let v = self.list.get(self.pos).cloned().ok_or_else(|| usage(format!("{name} needs a value")))?;
        self.pos += 1;
        Ok(v)
    }
}

fn flag(name: &str, inline: Option<String>) -> Result<bool, CliError> {
    match inline {
        None => Ok(true),
        Some(_) => Err(usage(format!("{name} takes no value"))),
    }
}

fn parse_count(name: &str, v: &str) -> Result<usize, CliError> {
    v.parse().map_err(|_| usage(format!("{name} wants a whole number, not {v:?}")))
}

fn parse_widths(v: &str) -> Result<Vec<u32>, CliError> {
    let mut widths = Vec::new();
    for part in v.split(',') {
        match part.trim().parse::<u32>() {
            Ok(w) if (1..=64).contains(&w) => widths.push(w),
            _ => return Err(usage(format!("widths must be 1 to 64, not {part:?}"))),
        }
    }
    widths.sort_unstable();
    widths.dedup();
    Ok(widths)
}

fn parse_orders(v: &str) -> Result<Vec<Order>, CliError> {
    match v.to_ascii_lowercase().as_str() {
        "big" | "be" => Ok(vec![Order::Big]),
        "little" | "le" => Ok(vec![Order::Little]),
        "both" => Ok(vec![Order::Big, Order::Little]),
        _ => Err(usage(format!("--endian is big, little or both, not {v:?}"))),
    }
}

fn parse_skip(v: &str) -> Result<(usize, usize), CliError> {
    let (lo, hi) = match v.split_once('-') {
        Some((a, b)) => (parse_count("--skip", a)?, parse_count("--skip", b)?),
        None => {
            let n = parse_count("--skip", v)?;
            (n, n)
        }
    };
    if lo > hi || hi - lo >= 256 {
        return Err(usage(format!("--skip range {v:?} should run low to high, at most 256 values")));
    }
    Ok((lo, hi))
}

fn read_bytes(path: &str) -> Result<Vec<u8>, CliError> {
    if path == "-" {
        let mut buf = Vec::new();
        std::io::stdin().read_to_end(&mut buf).map_err(|e| bad_input(format!("standard input: {e}")))?;
        Ok(buf)
    } else {
        std::fs::read(path).map_err(|e| bad_input(format!("{path}: {e}")))
    }
}

fn read_text(path: &str) -> Result<String, CliError> {
    let name = if path == "-" { "standard input" } else { path };
    String::from_utf8(read_bytes(path)?).map_err(|_| bad_input(format!("{name} is not UTF-8 text")))
}

fn find(args: Vec<String>) -> Result<ExitCode, CliError> {
    let mut opts = Options::default();
    let (mut files, mut positional) = (Vec::new(), Vec::new());
    let (mut binary, mut text, mut explain) = (false, false, false);
    let mut args = Args::new(args);
    while let Some(arg) = args.next() {
        let (name, inline) = match arg {
            Arg::Pos(p) => {
                positional.push(p);
                continue;
            }
            Arg::Opt(name, inline) => (name, inline),
        };
        match name.as_str() {
            "-w" | "--width" => opts.widths = parse_widths(&args.value(&name, inline)?)?,
            "-e" | "--endian" => opts.orders = parse_orders(&args.value(&name, inline)?)?,
            "-s" | "--skip" => opts.skip = parse_skip(&args.value(&name, inline)?)?,
            "-t" | "--tail" => opts.tail = parse_count(&name, &args.value(&name, inline)?)?,
            "-f" | "--file" => files.push(args.value(&name, inline)?),
            "-b" | "--binary" => binary = flag(&name, inline)?,
            "--text" => text = flag(&name, inline)?,
            "--catalogue-only" | "--catalog-only" => opts.solver = !flag(&name, inline)?,
            "--solve-only" => opts.catalogue = !flag(&name, inline)?,
            "--explain" => explain = flag(&name, inline)?,
            "-h" | "--help" => {
                emit(FIND_HELP);
                return Ok(ExitCode::SUCCESS);
            }
            _ => return Err(usage(format!("find: unknown option {name}"))),
        }
    }
    if !opts.catalogue && !opts.solver {
        return Err(usage("--catalogue-only with --solve-only leaves nothing to try"));
    }
    if binary && text {
        return Err(usage("--binary and --text do not mix"));
    }

    let mut raw: Vec<RawSample> = Vec::new();
    for p in &positional {
        if binary {
            raw.push(RawSample { bytes: read_bytes(p)?, crc: None });
        } else {
            raw.push(input::parse_sample(p, text).map_err(|e| bad_input(format!("sample {p:?}: {e}")))?);
        }
    }
    for f in &files {
        let name = if f == "-" { "standard input" } else { f.as_str() };
        raw.extend(input::parse_sample_lines(&read_text(f)?, text).map_err(|e| bad_input(format!("{name}, {e}")))?);
    }
    if positional.is_empty() && files.is_empty() {
        if std::io::stdin().is_terminal() {
            return Err(usage("find needs samples: as arguments, with -f FILE, or on standard input"));
        }
        let content = read_text("-")?;
        raw.extend(input::parse_sample_lines(&content, text).map_err(|e| bad_input(format!("standard input, {e}")))?);
    }
    if raw.is_empty() {
        return Err(bad_input("no samples to work with"));
    }

    let outcome = search::search(&raw, &opts);
    if outcome.matches.is_empty() {
        eprint!("{}", nothing_found(&outcome, &raw, &opts));
        return Ok(ExitCode::from(1));
    }
    emit(&report(&outcome, &raw, explain));
    Ok(ExitCode::SUCCESS)
}

fn report(outcome: &Outcome, raw: &[RawSample], explain: bool) -> String {
    let has_frames = raw.iter().any(|r| r.crc.is_none());
    let mut s = String::new();
    for (i, m) in outcome.matches.iter().enumerate() {
        if i > 0 {
            s.push('\n');
        }
        let w = m.params.width;
        let how = match (m.from_catalogue, m.solved) {
            (true, true) => "catalogue + solver",
            (true, false) => "catalogue",
            _ => "solver",
        };
        let _ = writeln!(s, "{}  ({how})", m.name.unwrap_or("unnamed CRC"));
        let _ = writeln!(s, "  {} check={}", m.params, hex(m.params.check(), w));
        if let Some(l) = describe(&m.layout, has_frames) {
            let _ = writeln!(s, "  {l}");
        }
        let _ = writeln!(s, "  {}; {}", fits(m.samples, raw.len()), odds(m.evidence));
        if !m.twins.is_empty() && (explain || (m.name.is_none() && m.open == 0)) {
            let shown: Vec<String> = m.twins.iter().take(3).map(init_xorout).collect();
            let more = m.twins.len().saturating_sub(3);
            let _ = writeln!(
                s,
                "  the same CRC exactly: {}{} (x+1 divides the polynomial, so init and xorout can shift together)",
                shown.join("; "),
                if more > 0 { format!(" and {more} more") } else { String::new() }
            );
        }
        if !m.readings.is_empty() {
            let _ = writeln!(s, "  the leading bytes never change, and a CRC cannot tell those from a different init,");
            let _ = writeln!(s, "  so this is also:");
            for &(skip, init) in &m.readings {
                let how = if skip == 0 {
                    "covering every byte".to_string()
                } else {
                    let shown: Vec<String> = raw[0].bytes[..skip].iter().take(8).map(|b| format!("{b:02x}")).collect();
                    let more = if skip > 8 { " ..." } else { "" };
                    format!("leaving out the first {} ({}{more})", plural(skip, "byte"), shown.join(" "))
                };
                let _ = writeln!(s, "    {how}: init={}", hex(init, w));
            }
        }
        if m.open > 0 {
            let why = if m.one_length {
                "every sample has the same length"
            } else {
                "the sample lengths cannot tell them apart"
            };
            let _ = writeln!(s, "  init and xorout are not pinned down: {why}, so 2^{} different CRCs fit.", m.open);
            if !m.alternatives.is_empty() {
                let _ = writeln!(s, "  Other conventional choices that fit:");
                for a in &m.alternatives {
                    let _ = writeln!(s, "    {} check={}", init_xorout(a), hex(a.check(), w));
                }
            }
            let _ = writeln!(s, "  A sample of another length would decide between them.");
        }
        if explain {
            for t in &m.trace {
                let _ = writeln!(s, "  how: {t}");
            }
        }
    }
    if explain && !outcome.notes.is_empty() {
        s.push('\n');
        for n in &outcome.notes {
            let _ = writeln!(s, "note: {n}");
        }
    }
    s
}

fn nothing_found(outcome: &Outcome, raw: &[RawSample], opts: &Options) -> String {
    let mut s = String::new();
    let _ = writeln!(
        s,
        "whichcrc: no CRC fits {}",
        if raw.len() == 1 { "the sample".to_string() } else { format!("all {} samples", raw.len()) }
    );
    let widths: Vec<String> = outcome.widths.iter().map(u32::to_string).collect();
    let tried = match (opts.catalogue, opts.solver) {
        (true, true) => "the catalogue and the solver",
        (true, false) => "the catalogue",
        _ => "the solver",
    };
    let _ = writeln!(s, "  tried {tried} on {} readings (widths {})", outcome.layouts, widths.join(", "));
    for n in &outcome.notes {
        let _ = writeln!(s, "  {n}");
    }
    let _ = writeln!(
        s,
        "  if the CRC leaves out a header or is followed by more bytes, try --skip or --tail; other widths need -w"
    );
    s
}

fn init_xorout(p: &Params) -> String {
    format!("init={} xorout={}", hex(p.init, p.width), hex(p.xorout, p.width))
}

fn plural(n: usize, word: &str) -> String {
    if n == 1 {
        word.to_string()
    } else {
        format!("{n} {word}s")
    }
}

fn describe(layout: &Layout, has_frames: bool) -> Option<String> {
    let mut parts = Vec::new();
    if has_frames {
        let n = layout.width.div_ceil(8) as usize;
        let place = if layout.tail == 0 {
            format!("the last {}", plural(n, "byte"))
        } else {
            format!("the {} before the last {}", plural(n, "byte"), plural(layout.tail, "byte"))
        };
        let order = match layout.order {
            Some(Order::Big) => ", big-endian",
            Some(Order::Little) => ", little-endian",
            None => "",
        };
        parts.push(format!("CRC is {place}{order}"));
    }
    if layout.skip > 0 {
        parts.push(format!("the first {} not covered", plural(layout.skip, "byte")));
    }
    (!parts.is_empty()).then(|| parts.join("; "))
}

fn fits(distinct: usize, given: usize) -> String {
    let repeats = given - distinct.min(given);
    let core = match distinct {
        1 => "fits the one sample".to_string(),
        2 => "fits both samples".to_string(),
        n => format!("fits all {n} samples"),
    };
    if repeats > 0 {
        format!("{core} ({} ignored)", plural(repeats, "repeat"))
    } else {
        core
    }
}

/// `bits` is log2 of the odds against a fit this good arising by chance.
fn odds(bits: f64) -> String {
    if bits >= 64.0 {
        "odds of a fluke below 1 in 2^64".to_string()
    } else if bits >= 20.0 {
        format!("odds of a fluke about 1 in 2^{}", bits.floor())
    } else if bits >= 1.0 {
        format!("weak: odds of a fluke about 1 in {}; more samples would settle it", 2f64.powf(bits).round())
    } else {
        "unconfirmed: this few samples could fit by chance; add more".to_string()
    }
}

fn resolve_model(spec: &str) -> Result<Params, CliError> {
    if spec.contains('=') {
        parse_params(spec).map_err(bad_input)
    } else {
        catalogue::lookup(spec).map(|e| e.params).map_err(bad_input)
    }
}

fn calc(args: Vec<String>) -> Result<ExitCode, CliError> {
    let (mut model, mut all) = (None, false);
    let mut inputs: Vec<(String, Vec<u8>)> = Vec::new();
    let mut files = Vec::new();
    let mut args = Args::new(args);
    while let Some(arg) = args.next() {
        let (name, inline) = match arg {
            Arg::Pos(p) => {
                files.push(p);
                continue;
            }
            Arg::Opt(name, inline) => (name, inline),
        };
        match name.as_str() {
            "-m" | "--model" => model = Some(args.value(&name, inline)?),
            "-a" | "--all" => all = flag(&name, inline)?,
            "-x" | "--hex" => {
                let v = args.value(&name, inline)?;
                let bytes = input::parse_hex(&v).map_err(|e| bad_input(format!("--hex {v:?}: {e}")))?;
                inputs.push((v, bytes));
            }
            "--text" => {
                let v = args.value(&name, inline)?;
                inputs.push((format!("{v:?}"), v.into_bytes()));
            }
            "-h" | "--help" => {
                emit(CALC_HELP);
                return Ok(ExitCode::SUCCESS);
            }
            _ => return Err(usage(format!("calc: unknown option {name}"))),
        }
    }
    let params = match (&model, all) {
        (None, false) => return Err(usage("calc needs -m MODEL or --all")),
        (Some(_), true) => return Err(usage("calc takes -m MODEL or --all, not both")),
        (Some(m), false) => Some(resolve_model(m)?),
        (None, true) => None,
    };
    for f in files {
        let bytes = read_bytes(&f)?;
        inputs.push((f, bytes));
    }
    if inputs.is_empty() {
        if std::io::stdin().is_terminal() {
            return Err(usage("calc needs input: --hex, --text, file names, or standard input"));
        }
        inputs.push(("-".to_string(), read_bytes("-")?));
    }

    let mut s = String::new();
    match params {
        Some(p) => {
            let eng = Engine::new(p);
            for (label, data) in &inputs {
                let v = hex(eng.checksum(data), p.width);
                let _ = if inputs.len() == 1 { writeln!(s, "{v}") } else { writeln!(s, "{v}  {label}") };
            }
        }
        None => {
            for (i, (label, data)) in inputs.iter().enumerate() {
                if inputs.len() > 1 {
                    let _ = writeln!(s, "{}{label}:", if i > 0 { "\n" } else { "" });
                }
                for e in CATALOGUE {
                    let _ =
                        writeln!(s, "{:<24}  {}", e.name, hex(Engine::new(e.params).checksum(data), e.params.width));
                }
            }
        }
    }
    emit(&s);
    Ok(ExitCode::SUCCESS)
}

fn list(args: Vec<String>) -> Result<ExitCode, CliError> {
    let mut width = None;
    let mut names = Vec::new();
    let mut args = Args::new(args);
    while let Some(arg) = args.next() {
        let (name, inline) = match arg {
            Arg::Pos(p) => {
                names.push(p);
                continue;
            }
            Arg::Opt(name, inline) => (name, inline),
        };
        match name.as_str() {
            "-w" | "--width" => {
                let v = args.value(&name, inline)?;
                width = Some(*parse_widths(&v)?.first().expect("one width"));
            }
            "-h" | "--help" => {
                emit(LIST_HELP);
                return Ok(ExitCode::SUCCESS);
            }
            _ => return Err(usage(format!("list: unknown option {name}"))),
        }
    }
    let mut s = String::new();
    match names.as_slice() {
        [] => {
            let shown: Vec<_> = CATALOGUE.iter().filter(|e| width.is_none_or(|w| e.params.width == w)).collect();
            if shown.is_empty() {
                return Err(bad_input(format!("the catalogue has no models of width {}", width.unwrap_or(0))));
            }
            for e in shown {
                let _ = writeln!(s, "{:<24}  {} check={}", e.name, e.params, hex(e.check, e.params.width));
            }
        }
        [name] => {
            let e = catalogue::lookup(name).map_err(bad_input)?;
            let w = e.params.width;
            let _ = writeln!(s, "{}", e.name);
            let _ = writeln!(s, "  {}", e.params);
            let _ = writeln!(s, "  check={} residue={}", hex(e.check, w), hex(e.residue, w));
            if !e.aliases.is_empty() {
                let _ = writeln!(s, "  aliases: {}", e.aliases.join(", "));
            }
        }
        _ => return Err(usage("list takes at most one name")),
    }
    emit(&s);
    Ok(ExitCode::SUCCESS)
}
