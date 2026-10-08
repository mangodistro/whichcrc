//! Try every reading of the samples (CRC width, byte order of the CRC field,
//! leading bytes it does not cover), match each against the catalogue and
//! run the solver on it, and say how much the result can be trusted.

use crate::catalogue::{identify, Entry, CATALOGUE};
use crate::crc::{mask, Engine, Params};
use crate::solve::{self, equivalent, Failure, Fit, Sample};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawSample {
    pub bytes: Vec<u8>,
    /// None: `bytes` is a whole frame with the CRC at the end.
    pub crc: Option<Explicit>,
}

/// A CRC given as a number, and how many hex digits it was written with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Explicit {
    pub value: u64,
    pub digits: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    Big,
    Little,
}

/// One reading of the frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub width: u32,
    /// None when the CRC field is one byte or every sample gave its CRC as a number.
    pub order: Option<Order>,
    /// Leading bytes the CRC does not cover.
    pub skip: usize,
    /// Bytes after the CRC field.
    pub tail: usize,
}

pub struct Options {
    /// Empty: 8, 16, 24, 32 and 64 for frames, 4 bits per digit for MESSAGE=CRC.
    pub widths: Vec<u32>,
    pub orders: Vec<Order>,
    /// Inclusive range of skip values to try.
    pub skip: (usize, usize),
    pub tail: usize,
    pub catalogue: bool,
    pub solver: bool,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            widths: Vec::new(),
            orders: vec![Order::Big, Order::Little],
            skip: (0, 0),
            tail: 0,
            catalogue: true,
            solver: true,
        }
    }
}

pub const DEFAULT_WIDTHS: [u32; 5] = [8, 16, 24, 32, 64];

/// The reflection settings the solver tries, the common pairs first.
const REFLECTIONS: [(bool, bool); 4] = [(true, true), (false, false), (false, true), (true, false)];

#[derive(Clone, Debug)]
pub struct Match {
    pub layout: Layout,
    pub params: Params,
    /// The catalogue name of this function, however it was found.
    pub name: Option<&'static str>,
    pub from_catalogue: bool,
    /// The solver reached this model from the samples alone.
    pub solved: bool,
    pub twins: Vec<Params>,
    pub open: usize,
    pub alternatives: Vec<Params>,
    pub samples: usize,
    pub one_length: bool,
    /// log2 of the odds against a fit like this happening by chance.
    pub evidence: f64,
    pub trace: Vec<String>,
    /// The same computation read with a different skip: the skipped bytes
    /// are identical in every sample, and a CRC cannot tell a constant
    /// prefix from a different init. (skip, init) pairs.
    pub readings: Vec<(usize, u64)>,
}

#[derive(Default)]
pub struct Outcome {
    pub matches: Vec<Match>,
    pub notes: Vec<String>,
    pub widths: Vec<u32>,
    pub layouts: usize,
}

impl Outcome {
    fn note(&mut self, s: String) {
        if !self.notes.contains(&s) {
            self.notes.push(s);
        }
    }
}

pub fn search(raw: &[RawSample], opts: &Options) -> Outcome {
    let mut out = Outcome::default();
    let has_frames = raw.iter().any(|r| r.crc.is_none());
    out.widths = if opts.widths.is_empty() { default_widths(raw) } else { opts.widths.clone() };
    for width in out.widths.clone() {
        let nbytes = width.div_ceil(8) as usize;
        let orders: Vec<Option<Order>> =
            if has_frames && nbytes > 1 { opts.orders.iter().map(|&o| Some(o)).collect() } else { vec![None] };
        let layouts = (opts.skip.1 - opts.skip.0 + 1) * orders.len();
        let entries: Vec<&Entry> =
            if opts.catalogue { CATALOGUE.iter().filter(|e| e.params.width == width).collect() } else { Vec::new() };
        for skip in opts.skip.0..=opts.skip.1 {
            let items = match split(raw, nbytes, skip, opts.tail) {
                Ok(items) => items,
                Err(why) => {
                    out.note(format!("width {width}{}: {why}", skip_text(skip)));
                    continue;
                }
            };
            let one_length = items.windows(2).all(|p| p[0].msg.len() == p[1].msg.len());
            let need = solve::min_samples(if one_length { 1 } else { 2 });
            // Each catalogue CRC once per message, shared by both byte orders.
            let crcs: Vec<Vec<u64>> = entries
                .iter()
                .map(|e| {
                    let eng = Engine::new(e.params);
                    items.iter().map(|it| eng.checksum(it.msg)).collect()
                })
                .collect();
            for &order in &orders {
                let layout = Layout { width, order, skip, tail: opts.tail };
                let Some(values) = crc_values(&items, width, order) else {
                    continue;
                };
                out.layouts += 1;
                let n = values.len();
                let mut here: Vec<Match> = Vec::new();
                for (e, crcs) in entries.iter().zip(&crcs) {
                    if *crcs == values {
                        let tried = (entries.len() * layouts) as f64;
                        here.push(Match {
                            layout,
                            params: e.params,
                            name: Some(e.name),
                            from_catalogue: true,
                            solved: false,
                            twins: Vec::new(),
                            open: 0,
                            alternatives: Vec::new(),
                            samples: n,
                            one_length,
                            evidence: n as f64 * f64::from(width) - tried.log2(),
                            trace: Vec::new(),
                            readings: Vec::new(),
                        });
                    }
                }
                if opts.solver && n < need {
                    out.note(failure_text(width, &Failure::TooFewSamples { have: n, need }));
                } else if opts.solver {
                    // Owned copies only now that the solver will actually run.
                    let samples: Vec<Sample> =
                        items.iter().zip(&values).map(|(it, &crc)| Sample { msg: it.msg.to_vec(), crc }).collect();
                    for (refin, refout) in REFLECTIONS {
                        match solve::solve(&samples, width, refin, refout) {
                            Ok(fits) => {
                                for fit in fits {
                                    merge(&mut here, fit, layout, n, one_length, layouts);
                                }
                            }
                            Err(f) => out.note(failure_text(width, &f)),
                        }
                    }
                }
                out.matches.extend(here);
            }
        }
    }
    // Named models first, then conventional inits, then the strongest evidence.
    let rank = |m: &Match| (m.name.is_some(), m.params.init == 0 || m.params.init == mask(m.params.width), m.evidence);
    out.matches.sort_by(|a, b| rank(b).partial_cmp(&rank(a)).expect("finite"));
    out.matches = fold_readings(std::mem::take(&mut out.matches), raw);
    out
}

/// Fold each match into an earlier one when it is the same computation with
/// a different number of leading bytes left out.
fn fold_readings(matches: Vec<Match>, raw: &[RawSample]) -> Vec<Match> {
    let mut kept: Vec<Match> = Vec::new();
    for m in matches {
        match kept.iter_mut().find(|k| same_reading(k, &m, raw)) {
            Some(k) => k.readings.push((m.layout.skip, m.params.init)),
            None => kept.push(m),
        }
    }
    kept
}

fn same_reading(a: &Match, b: &Match, raw: &[RawSample]) -> bool {
    let (pa, pb) = (&a.params, &b.params);
    if a.layout.skip == b.layout.skip
        || (a.layout.width, a.layout.order, a.layout.tail) != (b.layout.width, b.layout.order, b.layout.tail)
        || (pa.poly, pa.refin, pa.refout) != (pb.poly, pb.refin, pb.refout)
    {
        return false;
    }
    let (short, long) = if a.layout.skip < b.layout.skip { (a, b) } else { (b, a) };
    let (lo, hi) = (short.layout.skip, long.layout.skip);
    let Some(prefix) = raw.first().and_then(|r| r.bytes.get(lo..hi)) else {
        return false;
    };
    if !raw.iter().all(|r| r.bytes.get(lo..hi) == Some(prefix)) {
        return false;
    }
    // Run the shared bytes through the shorter-skip model: the register it
    // reaches is the init the longer-skip model needs.
    let reached = Engine::new(short.params).update(short.params.init, prefix);
    equivalent(&Params { init: reached, ..short.params }, &long.params)
}

/// Fold a solver result into the catalogue matches for the same layout, or
/// add it as a model of its own.
fn merge(here: &mut Vec<Match>, fit: Fit, layout: Layout, n: usize, one_length: bool, layouts: usize) {
    let listed = |p: &Params, here: &[Match]| here.iter().any(|m| m.from_catalogue && equivalent(&m.params, p));
    let alternatives: Vec<Params> = fit.alternatives.iter().filter(|a| !listed(a, here)).copied().collect();
    for m in here.iter_mut() {
        if m.from_catalogue && fit.alternatives.iter().any(|a| equivalent(a, &m.params)) {
            m.solved = true;
        }
    }
    if let Some(m) = here.iter_mut().find(|m| m.from_catalogue && equivalent(&m.params, &fit.params)) {
        m.solved = true;
        m.twins = fit.twins;
        m.open = fit.open;
        m.alternatives = alternatives;
        m.trace = fit.trace;
        return;
    }
    let w = layout.width as usize;
    let twin_dims = (fit.twins.len() + 1).trailing_zeros() as usize;
    // The solver fitted the poly plus whatever of init and xorout it pinned down.
    let fitted = 3 * w - fit.open - twin_dims;
    let evidence = (n * w) as f64 - fitted as f64 - ((4 * layouts) as f64).log2();
    here.push(Match {
        layout,
        params: fit.params,
        name: identify(&fit.params).map(|e| e.name),
        from_catalogue: false,
        solved: true,
        twins: fit.twins,
        open: fit.open,
        alternatives,
        samples: n,
        one_length,
        evidence,
        trace: fit.trace,
        readings: Vec::new(),
    });
}

fn default_widths(raw: &[RawSample]) -> Vec<u32> {
    let mut widths = Vec::new();
    if raw.iter().any(|r| r.crc.is_none()) {
        widths.extend(DEFAULT_WIDTHS);
    }
    if let Some(d) = raw.iter().filter_map(|r| r.crc.map(|e| e.digits)).max() {
        widths.push((4 * d).min(64) as u32);
    }
    widths.sort_unstable();
    widths.dedup();
    widths
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Field<'a> {
    Bytes(&'a [u8]),
    Value(u64),
}

/// A sample cut into message and CRC field, borrowing from the input.
#[derive(Debug)]
struct Item<'a> {
    msg: &'a [u8],
    field: Field<'a>,
}

/// Cut each sample into message and CRC field. Repeated samples are dropped;
/// two samples with one message and different CRCs rule this reading out.
fn split(raw: &[RawSample], nbytes: usize, skip: usize, tail: usize) -> Result<Vec<Item<'_>>, String> {
    let mut items: Vec<Item> = Vec::new();
    // message -> (index in items, sample number)
    let mut seen: HashMap<&[u8], (usize, usize)> = HashMap::new();
    for (i, r) in raw.iter().enumerate() {
        let len = r.bytes.len();
        let item = match r.crc {
            Some(e) => {
                if len < skip {
                    return Err(format!("sample {} is shorter than the bytes to skip", i + 1));
                }
                Item { msg: &r.bytes[skip..], field: Field::Value(e.value) }
            }
            None => {
                let need = skip + nbytes + tail;
                if len < need {
                    return Err(format!("sample {} has {len} bytes, this reading needs at least {need}", i + 1));
                }
                let end = len - tail;
                Item { msg: &r.bytes[skip..end - nbytes], field: Field::Bytes(&r.bytes[end - nbytes..end]) }
            }
        };
        match seen.get(&item.msg) {
            Some(&(j, _)) if items[j].field == item.field => continue,
            Some(&(_, first)) => {
                return Err(format!("samples {first} and {} have the same message but different CRCs", i + 1));
            }
            None => {
                seen.insert(item.msg, (items.len(), i + 1));
                items.push(item);
            }
        }
    }
    Ok(items)
}

/// The CRC of each item as a number, read in the given byte order. None if
/// any of them needs more bits than the width.
fn crc_values(items: &[Item], width: u32, order: Option<Order>) -> Option<Vec<u64>> {
    items
        .iter()
        .map(|it| {
            let crc = match it.field {
                Field::Value(v) => v,
                Field::Bytes(b) => {
                    let fold = |acc: u64, x: &u8| (acc << 8) | u64::from(*x);
                    match order {
                        Some(Order::Little) => b.iter().rev().fold(0, fold),
                        _ => b.iter().fold(0, fold),
                    }
                }
            };
            (crc <= mask(width)).then_some(crc)
        })
        .collect()
}

fn skip_text(skip: usize) -> String {
    if skip == 0 {
        String::new()
    } else {
        format!(", skipping {skip}")
    }
}

fn failure_text(width: u32, f: &Failure) -> String {
    match f {
        Failure::TooFewSamples { have, need } => format!(
            "the solver needs at least 3 samples of one length or 4 of mixed lengths; {have} here, {need} needed"
        ),
        Failure::TooLong => format!("width {width}: the messages are too long for the solver to use"),
        Failure::Underdetermined { degree } => format!(
            "width {width}: the samples only narrow the polynomial down to the divisors of one of degree {degree}; \
             more samples of a single length would help"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::parse_sample;

    fn frames(lines: &[&str]) -> Vec<RawSample> {
        lines.iter().map(|l| parse_sample(l, false).unwrap()).collect()
    }

    // CRC-16/MODBUS, CRC sent low byte first. Built with a separate
    // implementation of the CRC model, not with this crate.
    const MODBUS: [&str; 4] =
        ["01 04 02 00 e7 f9 7a", "01 04 02 01 02 39 61", "02 04 04 00 e7 01 9c 79 4a", "03 06 00 10 00 2a 08 32"];

    #[test]
    fn split_drops_repeats_and_rejects_contradictions() {
        let raw = frames(&["01 02 03 aa", "01 02 03 aa", "04 05 06 bb"]);
        assert_eq!(split(&raw, 1, 0, 0).unwrap().len(), 2);
        let raw = frames(&["01 02 03 aa", "04 05 06 bb", "01 02 03 ab"]);
        assert_eq!(split(&raw, 1, 0, 0).err().unwrap(), "samples 1 and 3 have the same message but different CRCs");
        assert!(split(&raw, 8, 0, 0).unwrap_err().contains("has 4 bytes, this reading needs at least 8"));
    }

    #[test]
    fn split_honours_skip_and_tail() {
        let raw = frames(&["7e 01 02 03 aa bb 7e"]);
        let items = split(&raw, 2, 1, 1).unwrap();
        assert_eq!(items[0].msg, [1, 2, 3]);
        assert_eq!(items[0].field, Field::Bytes(&[0xaa, 0xbb]));
    }

    #[test]
    fn crc_fields_read_in_either_order_and_must_fit_the_width() {
        let raw = frames(&["00 12 34"]);
        let items = split(&raw, 2, 0, 0).unwrap();
        assert_eq!(crc_values(&items, 16, Some(Order::Big)), Some(vec![0x1234]));
        assert_eq!(crc_values(&items, 16, Some(Order::Little)), Some(vec![0x3412]));
        assert_eq!(crc_values(&items, 12, Some(Order::Big)), None, "0x1234 needs 13 bits");
        assert!(crc_values(&items, 14, Some(Order::Big)).is_some());
    }

    #[test]
    fn widths_come_from_frames_or_from_digits() {
        assert_eq!(default_widths(&frames(&["0102"])), DEFAULT_WIDTHS.to_vec());
        assert_eq!(default_widths(&frames(&["31=0a", "32=00b0"])), vec![16]);
        assert_eq!(default_widths(&frames(&["31=0123456789"])), vec![40]);
    }

    #[test]
    fn modbus_frames_match_only_the_little_endian_reading() {
        let out = search(&frames(&MODBUS), &Options::default());
        let m = &out.matches[0];
        assert_eq!(m.name, Some("CRC-16/MODBUS"));
        assert_eq!(m.layout.order, Some(Order::Little));
        assert!(m.from_catalogue && m.solved);
        assert_eq!(m.samples, 4);
        assert_eq!(out.matches.len(), 1, "{:?}", out.matches);
    }

    #[test]
    fn catalogue_and_solver_each_find_it_alone() {
        let raw = frames(&MODBUS);
        let cat = search(&raw, &Options { solver: false, ..Options::default() });
        assert!(cat.matches[0].from_catalogue && !cat.matches[0].solved);
        let solved = search(&raw, &Options { catalogue: false, ..Options::default() });
        let m = &solved.matches[0];
        assert!(!m.from_catalogue && m.solved);
        assert_eq!(m.name, Some("CRC-16/MODBUS"), "named after the fact");
    }

    #[test]
    fn a_constant_header_is_folded_into_init() {
        // width=16 poly=0x2f15 init=0x1d0f xorout=0x5a5a over everything after "aa 55".
        let raw = frames(&[
            "aa 55 01 10 00 e7 07 03",
            "aa 55 01 10 01 02 10 c7",
            "aa 55 02 11 00 e7 01 9c 51 93",
            "aa 55 03 20 2a 91 b0",
            "aa 55 01 10 00 e8 00 13 62",
            "aa 55 04 30 a5 55",
        ]);
        let out = search(&raw, &Options { skip: (0, 3), ..Options::default() });
        assert_eq!(out.matches.len(), 1);
        let m = &out.matches[0];
        assert_eq!((m.layout.skip, m.params.init), (0, 0x41e1));
        assert_eq!(m.readings, vec![(1, 0xbd30), (2, 0x1d0f)]);
    }

    #[test]
    fn a_header_that_varies_is_not_folded() {
        let p = Params { width: 8, poly: 0x07, init: 0, refin: false, refout: false, xorout: 0 };
        let raw: Vec<RawSample> = (0u8..6)
            .map(|i| {
                let body = vec![i, 0x10 + i, 0x20, 0x30 + 2 * i];
                let mut bytes = vec![0x40 + i];
                bytes.extend(&body);
                bytes.push(p.checksum(&body) as u8);
                RawSample { bytes, crc: None }
            })
            .collect();
        let out = search(&raw, &Options { widths: vec![8], skip: (0, 1), ..Options::default() });
        let found: Vec<_> = out.matches.iter().filter(|m| m.params == p).collect();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].layout.skip, 1);
        assert!(found[0].readings.is_empty());
    }

    #[test]
    fn too_few_samples_for_the_solver_is_noted() {
        let out = search(&frames(&MODBUS[..2]), &Options::default());
        assert_eq!(out.matches[0].name, Some("CRC-16/MODBUS"));
        assert!(out.notes.iter().any(|n| n.contains("solver needs at least 3")));
    }
}
