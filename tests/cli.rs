//! Run the built binary the way a user would. Every frame here was built with
//! a separate implementation of the CRC model, not with this crate.

use std::io::Write;
use std::process::{Command, Output, Stdio};

fn whichcrc(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_whichcrc")).args(args).output().expect("run whichcrc")
}

fn with_stdin(args: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_whichcrc"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn whichcrc");
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    child.wait_with_output().unwrap()
}

fn stdout(o: &Output) -> String {
    String::from_utf8(o.stdout.clone()).unwrap()
}

fn stderr(o: &Output) -> String {
    String::from_utf8(o.stderr.clone()).unwrap()
}

/// CRC-16/MODBUS frames, CRC low byte first.
const MODBUS: [&str; 4] =
    ["01 04 02 00 e7 f9 7a", "01 04 02 01 02 39 61", "02 04 04 00 e7 01 9c 79 4a", "03 06 00 10 00 2a 08 32"];

/// width=16 poly=0x1021 init=0x5a17 refin=true refout=true xorout=0: CCITT
/// with a made-up init, in no catalogue.
const CUSTOM_INIT: [&str; 5] = [
    "7e 01 10 00 e7 4c 16",
    "7e 01 10 01 02 37 bf",
    "7e 02 11 00 e7 01 9c 75 da",
    "7e 03 20 2a d3 70",
    "7e 01 10 00 e8 00 b6 0b",
];

#[test]
fn help_and_version() {
    let o = whichcrc(&["--help"]);
    assert_eq!(o.status.code(), Some(0));
    for word in ["find", "calc", "list", "Exit status"] {
        assert!(stdout(&o).contains(word), "{word}");
    }
    let o = whichcrc(&["help", "find"]);
    assert!(stdout(&o).contains("--skip"));
    let o = whichcrc(&["find", "--help"]);
    assert_eq!(o.status.code(), Some(0));
    assert!(stdout(&o).contains("--solve-only"));
    assert!(stdout(&whichcrc(&["--version"])).starts_with("whichcrc "));
}

#[test]
fn usage_errors_exit_2() {
    let o = whichcrc(&[]);
    assert_eq!(o.status.code(), Some(2));
    assert!(stderr(&o).contains("usage:"));
    for args in [
        &["frobnicate"][..],
        &["find", "--colour"],
        &["find", "-w", "65", "0102"],
        &["find", "-w"],
        &["find", "--skip", "5-2", "0102"],
        &["find", "--endian", "middle", "0102"],
        &["find", "--catalogue-only", "--solve-only", "0102"],
        &["calc", "--text", "x"],
        &["calc", "-m", "CRC-32", "--all", "--text", "x"],
        &["list", "a", "b"],
    ] {
        let o = whichcrc(args);
        assert_eq!(o.status.code(), Some(2), "{args:?}");
        assert!(stderr(&o).contains("whichcrc --help"), "{args:?}");
    }
}

#[test]
fn bad_input_exits_2_and_says_what_is_wrong() {
    let o = whichcrc(&["find", "01 0g"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(stderr(&o).contains("'g' is not a hex digit"));
    let o = whichcrc(&["calc", "-m", "CRC-99/NOPE", "--text", "x"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(stderr(&o).contains("no catalogue model"));
    let o = whichcrc(&["find", "-f", "/nonexistent/samples.txt"]);
    assert_eq!(o.status.code(), Some(2));
    let o = whichcrc(&["calc", "-m", "width=8 poly=0x07 check=0x00", "--text", "x"]);
    assert!(stderr(&o).contains("check mismatch"));
}

#[test]
fn calc_matches_published_check_values() {
    for (model, check) in [
        ("CRC-32", "0xcbf43926"),
        ("crc-32c", "0xe3069283"),
        ("MODBUS", "0x4b37"),
        ("CRC-5/USB", "0x19"),
        ("CRC-64/XZ", "0x995dc9bbdf1939fa"),
    ] {
        let o = whichcrc(&["calc", "-m", model, "--text", "123456789"]);
        assert_eq!(o.status.code(), Some(0));
        assert_eq!(stdout(&o), format!("{check}\n"), "{model}");
    }
}

#[test]
fn calc_with_a_parameter_line_and_several_inputs() {
    let o = whichcrc(&[
        "calc",
        "-m",
        "width=16 poly=0x8005 init=0xffff refin=true refout=true xorout=0",
        "--hex",
        "01 03 00 00 00 0a",
        "--text",
        "123456789",
    ]);
    assert_eq!(stdout(&o), "0xcdc5  01 03 00 00 00 0a\n0x4b37  \"123456789\"\n");
}

#[test]
fn calc_reads_standard_input() {
    let o = with_stdin(&["calc", "-m", "XMODEM"], "123456789");
    assert_eq!(stdout(&o), "0x31c3\n");
}

#[test]
fn calc_all_agrees_with_the_list() {
    let all = stdout(&whichcrc(&["calc", "--all", "--text", "123456789"]));
    let list = stdout(&whichcrc(&["list"]));
    assert_eq!(all.lines().count(), 112);
    assert_eq!(list.lines().count(), 112);
    for (a, l) in all.lines().zip(list.lines()) {
        let (name, value) = a.split_once(' ').unwrap();
        assert!(l.starts_with(name));
        assert!(l.ends_with(&format!("check={}", value.trim())), "{l}");
    }
}

#[test]
fn list_filters_and_shows_one_model() {
    let o = stdout(&whichcrc(&["list", "-w", "5"]));
    let names: Vec<&str> = o.lines().map(|l| l.split_whitespace().next().unwrap()).collect();
    assert_eq!(names, ["CRC-5/EPC-C1G2", "CRC-5/G-704", "CRC-5/USB"]);
    let o = stdout(&whichcrc(&["list", "x-25"]));
    assert!(o.starts_with("CRC-16/IBM-SDLC\n"));
    assert!(o.contains("residue=0xf0b8"));
    assert!(o.contains("aliases: CRC-16/ISO-HDLC"));
    assert_eq!(whichcrc(&["list", "-w", "9"]).status.code(), Some(2));
}

#[test]
fn find_names_a_catalogue_crc() {
    let o = whichcrc(&[&["find"][..], &MODBUS].concat());
    assert_eq!(o.status.code(), Some(0));
    let out = stdout(&o);
    assert!(out.starts_with("CRC-16/MODBUS  (catalogue + solver)\n"), "{out}");
    assert!(out.contains("width=16 poly=0x8005 init=0xffff refin=true refout=true xorout=0x0000 check=0x4b37"));
    assert!(out.contains("CRC is the last 2 bytes, little-endian"));
    assert!(out.contains("fits all 4 samples"));
}

#[test]
fn find_solves_a_crc_no_catalogue_has() {
    let o = whichcrc(&[&["find"][..], &CUSTOM_INIT].concat());
    assert_eq!(o.status.code(), Some(0));
    let out = stdout(&o);
    assert!(out.starts_with("unnamed CRC  (solver)\n"), "{out}");
    assert!(out.contains("width=16 poly=0x1021 init=0x5a17 refin=true refout=true xorout=0x0000"));
    // 0x1021 is (x + 1)(...), so the twin is shown too.
    assert!(out.contains("init=0xaa08 xorout=0xf80f"));
}

#[test]
fn find_explains_itself() {
    let o = whichcrc(&[&["find", "--explain"][..], &CUSTOM_INIT].concat());
    let out = stdout(&o);
    assert!(out.contains("how: 3 constraint polynomials"));
    assert!(out.contains("note: width 32: samples 1 and 2 have the same message but different CRCs"));
}

#[test]
fn find_with_a_skipped_header() {
    let frames = [
        "aa 55 01 10 00 e7 07 03",
        "aa 55 01 10 01 02 10 c7",
        "aa 55 02 11 00 e7 01 9c 51 93",
        "aa 55 03 20 2a 91 b0",
        "aa 55 01 10 00 e8 00 13 62",
        "aa 55 04 30 a5 55",
    ];
    let o = whichcrc(&[&["find", "--skip", "0-3"][..], &frames].concat());
    let out = stdout(&o);
    assert!(out.contains("poly=0x2f15 init=0x41e1"));
    assert!(out.contains("leaving out the first 2 bytes (aa 55): init=0x1d0f"), "{out}");
}

#[test]
fn find_says_when_one_length_cannot_settle_init() {
    let frames = ["01 04 02 00 e7 3a a4", "01 04 02 01 02 fa bf", "02 04 02 00 99 fe 84", "03 04 02 01 2a 83 61"];
    let out = stdout(&whichcrc(&[&["find"][..], &frames].concat()));
    assert!(out.contains("not pinned down: every sample has the same length, so 2^15 different CRCs fit"));
    assert!(out.contains("init=0x0bad xorout=0x0000"), "the true one is among the alternatives");
}

#[test]
fn find_reads_files_and_standard_input() {
    let text = "# bench capture\n01 04 02 00 e7 f9 7a\n01 04 02 01 02 39 61  # second\n\n02 04 04 00 e7 01 9c 79 4a\n";
    let o = with_stdin(&["find", "--catalogue-only"], text);
    assert_eq!(o.status.code(), Some(0));
    assert!(stdout(&o).starts_with("CRC-16/MODBUS  (catalogue)\n"));
    let o = with_stdin(&["find", "-f", "-"], "zz\n");
    assert_eq!(o.status.code(), Some(2));
    assert!(stderr(&o).contains("standard input, line 1"));
}

#[test]
fn find_text_samples() {
    let o = whichcrc(&["find", "--text", "123456789=29b1", "hello=d26e"]);
    let out = stdout(&o);
    assert!(out.starts_with("CRC-16/IBM-3740  (catalogue)\n"), "{out}");
    assert!(!out.contains("CRC is"), "no frame layout to describe");
}

#[test]
fn find_with_an_odd_width() {
    // CRC-12/UMTS: refin false but refout true, in two bytes, big-endian.
    let frames = ["10 20 30 0a 2b", "10 20 31 40 01 15", "de ad be ef 02 31", "01 0f 01", "99 88 77 66 55 04 97"];
    let out = stdout(&whichcrc(&[&["find", "-w", "12"][..], &frames].concat()));
    assert!(out.starts_with("CRC-12/UMTS  (catalogue + solver)\n"), "{out}");
}

#[test]
fn find_nothing_exits_1_and_explains() {
    let o = whichcrc(&["find", "01 02 03 04 05 06", "aa bb cc dd ee ff", "10 20 30 40 50 60 70"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(stdout(&o).is_empty());
    let err = stderr(&o);
    assert!(err.contains("no CRC fits all 3 samples"));
    assert!(err.contains("the solver needs at least 3 samples"));
    assert!(err.contains("--skip"));
}

fn example(name: &str) -> String {
    format!("{}/examples/{name}", env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn the_example_files_find_what_the_readme_says() {
    let out = stdout(&whichcrc(&["find", "-f", &example("sensor-bus.txt")]));
    assert!(out.starts_with("CRC-16/MODBUS  (catalogue + solver)\n"), "{out}");
    let out = stdout(&whichcrc(&["find", "-f", &example("vendor-init.txt")]));
    assert!(out.contains("poly=0x1021 init=0x5a17 refin=true refout=true xorout=0x0000"), "{out}");
    let out = stdout(&whichcrc(&["find", "--skip", "0-3", "-f", &example("sync-header.txt")]));
    assert!(out.contains("leaving out the first 2 bytes (aa 55): init=0x1d0f"), "{out}");
}
