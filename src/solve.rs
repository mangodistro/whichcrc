//! Recover CRC parameters from samples by algebra instead of search.
//!
//! Undo the reflections first (bit-reverse each input byte if refin, the CRC
//! if refout). What is left is a plain MSB-first CRC, and for a message M of
//! L bytes it is
//!
//! ```text
//! crc = (M(x)·x^w + init·x^(8L)) mod P(x)  +  xorout
//! ```
//!
//! Two samples of the same length cancel init and xorout, so P divides
//! (M1 + M2)·x^w + crc1 + crc2, a polynomial we can write down. The gcd of
//! several of those is P times (usually) a little junk, and factoring the gcd
//! lists every possible P. Samples of different lengths can be used too after
//! eliminating init between them. Once P is fixed, the CRC is affine in the
//! bits of init and xorout, so those come out of a linear system over GF(2).
//!
//! That system never has a unique solution when x + 1 divides P: adding
//! P/(x + 1) to both init and xorout changes no output at all. Those "twins"
//! are the same function written two ways, so they are reported as such
//! rather than as an ambiguity.

use crate::catalogue::CATALOGUE;
use crate::crc::{hex, mask, reflect, Engine, Params};
use crate::gf2::{divisors_of_degree, Poly};
use crate::linear;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sample {
    pub msg: Vec<u8>,
    pub crc: u64,
}

#[derive(Clone, Debug)]
pub struct Fit {
    /// The form to show: a catalogue model if one fits, otherwise the form
    /// with init 0, xorout 0, init all ones or xorout all ones, in that order.
    pub params: Params,
    /// Other init/xorout pairs that give the same CRC as `params` for every
    /// message, not just these samples.
    pub twins: Vec<Params>,
    /// How many bits of init/xorout the samples leave undecided. Zero means
    /// the samples pin down the function. Otherwise 2^open different CRCs fit
    /// (typically because every sample has the same length).
    pub open: usize,
    /// When `open` > 0: other conventional forms that fit these samples but
    /// differ from `params` on other messages.
    pub alternatives: Vec<Params>,
    pub trace: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    /// Fewer samples than there are unknowns: anything would fit.
    TooFewSamples { have: usize, need: usize },
    /// Every constraint polynomial was too large to work with.
    TooLong,
    /// The polynomial is only known to divide one of this degree, which is
    /// too big to factor. More samples of one length shrink it.
    Underdetermined { degree: usize },
}

/// Constraints above this degree are skipped (about 16 KiB of message).
pub const MAX_CONSTRAINT_DEGREE: usize = 1 << 17;
/// A gcd above this degree is not factored.
pub const MAX_FACTOR_DEGREE: usize = 1 << 13;
/// At most this many candidate polynomials are tried.
pub const CANDIDATE_LIMIT: usize = 4096;

/// The fewest samples that leave something over to confirm a fit: three when
/// they all have one length (unknowns: poly and one combined constant),
/// otherwise four (poly, init and xorout).
pub fn min_samples(distinct_lengths: usize) -> usize {
    if distinct_lengths <= 1 {
        3
    } else {
        4
    }
}

/// True when `a` and `b` give the same CRC for every byte string. Beyond
/// a == b, that happens when they differ only in init and xorout, by the
/// same amount d (after undoing output reflection), with d·(x^8 + 1) ≡ 0
/// mod P, so that d·x^(8L) ≡ d for every length L.
pub fn equivalent(a: &Params, b: &Params) -> bool {
    if (a.width, a.poly, a.refin, a.refout) != (b.width, b.poly, b.refin, b.refout) {
        return false;
    }
    let w = a.width;
    let raw = |p: &Params| if p.refout { reflect(p.xorout, w) } else { p.xorout };
    let d = a.init ^ b.init;
    if raw(a) ^ raw(b) != d {
        return false;
    }
    let p = Poly::from_u128((1u128 << w) | u128::from(a.poly));
    Poly::from_u64(d).mul(&Poly::from_u64(0x101)).rem(&p).is_zero()
}

/// Find every (poly, init, xorout) of this width and reflection that fits all
/// samples. The samples must be distinct, and no two may share a message.
pub fn solve(samples: &[Sample], width: u32, refin: bool, refout: bool) -> Result<Vec<Fit>, Failure> {
    let w = width as usize;
    let mut by_len: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (i, s) in samples.iter().enumerate() {
        by_len.entry(s.msg.len()).or_default().push(i);
    }
    let need = min_samples(by_len.len());
    if samples.len() < need {
        return Err(Failure::TooFewSamples { have: samples.len(), need });
    }
    let norm: Vec<Sample> = samples
        .iter()
        .map(|s| Sample {
            msg: if refin { s.msg.iter().map(|b| b.reverse_bits()).collect() } else { s.msg.clone() },
            crc: if refout { reflect(s.crc, width) } else { s.crc },
        })
        .collect();

    // e_i = M_i·x^w + crc_i, which is congruent to init·x^(8L_i) + xorout mod P.
    let e: Vec<Poly> = norm.iter().map(|s| Poly::from_bytes(&s.msg).shl(w).add(&Poly::from_u64(s.crc))).collect();
    let mut cons = Vec::new();
    for group in by_len.values() {
        cons.extend(group[1..].iter().map(|&i| e[i].add(&e[group[0]])));
    }
    let same_length = cons.len();
    // Across lengths, v_k = e_k + e_0 is congruent to init·b_k with
    // b_k = x^(8L_k) + x^(8L_0); v_1·b_k + v_k·b_1 cancels init.
    let reps: Vec<usize> = by_len.values().map(|g| g[0]).collect();
    if reps.len() >= 3 {
        let x8 = |i: usize| Poly::monomial(8 * norm[i].msg.len());
        let v = |k: usize| e[reps[k]].add(&e[reps[0]]);
        let b = |k: usize| x8(reps[k]).add(&x8(reps[0]));
        let (v1, b1) = (v(1), b(1));
        cons.extend((2..reps.len()).map(|k| v1.mul(&b(k)).add(&v(k).mul(&b1))));
    }
    let cross_length = cons.len() - same_length;

    // P has a +1 term, so powers of x can be divided out; small ones first
    // keeps every gcd step cheap.
    let mut cons: Vec<Poly> = cons.into_iter().filter(|c| !c.is_zero()).map(|c| c.shr(c.trailing_zeros())).collect();
    let usable = cons.len();
    cons.retain(|c| c.degree().is_some_and(|d| d <= MAX_CONSTRAINT_DEGREE));
    if cons.is_empty() {
        return Err(Failure::TooLong);
    }
    cons.sort_by_key(|c| c.degree());
    let mut g = cons[0].clone();
    for c in &cons[1..] {
        if g.degree().is_some_and(|d| d < w) {
            break;
        }
        g = Poly::gcd(&g, c);
    }
    let gd = g.degree().expect("gcd of nonzero polynomials");
    let mut trace = vec![format!(
        "{} constraint polynomials ({} same-length, {} cross-length{}); their gcd has degree {}",
        cons.len(),
        same_length,
        cross_length,
        if usable > cons.len() { format!(", {} too long to use", usable - cons.len()) } else { String::new() },
        gd
    )];
    if gd < w {
        return Ok(Vec::new());
    }
    let candidates = if gd == w {
        vec![g]
    } else if gd > MAX_FACTOR_DEGREE {
        return Err(Failure::Underdetermined { degree: gd });
    } else {
        let (found, truncated) = divisors_of_degree(&g, w, CANDIDATE_LIMIT);
        trace.push(format!(
            "{}{} {} of degree {} to try",
            if truncated { "the first " } else { "" },
            found.len(),
            if found.len() == 1 { "divisor" } else { "divisors" },
            w
        ));
        found
    };

    let fits = candidates
        .iter()
        .filter_map(|p| {
            let poly = (p.low_u128() as u64) & mask(width);
            fit_init_xorout(samples, &norm, width, poly, refin, refout, &trace)
        })
        .collect();
    Ok(fits)
}

/// With the polynomial fixed, solve for init and xorout.
fn fit_init_xorout(
    samples: &[Sample],
    norm: &[Sample],
    width: u32,
    poly: u64,
    refin: bool,
    refout: bool,
    trace: &[String],
) -> Option<Fit> {
    let w = width as usize;
    let rows = equations(norm, width, poly);
    let sol = linear::solve(&rows, 2 * w)?;
    // Unknowns: init in bits 0..w, xorout before output reflection in w..2w.
    let make = |v: u128| {
        let init = v as u64 & mask(width);
        let x = (v >> w) as u64 & mask(width);
        Params { width, poly, init, refin, refout, xorout: if refout { reflect(x, width) } else { x } }
    };
    let pack = |p: &Params| {
        let x = if refout { reflect(p.xorout, width) } else { p.xorout };
        u128::from(p.init) | (u128::from(x) << w)
    };
    let fits_all = |p: &Params| {
        let eng = Engine::new(*p);
        samples.iter().all(|s| eng.checksum(&s.msg) == s.crc)
    };
    let particular = make(sol.particular);
    // The algebra says it fits; the plain engine has the final word.
    if !fits_all(&particular) {
        return None;
    }
    let twin_basis = twin_basis(poly, width);
    let open = sol.kernel.len().saturating_sub(twin_basis.len());

    // Forms worth showing, most conventional first; twins collapse together.
    let mut forms: Vec<Params> = CATALOGUE
        .iter()
        .map(|e| e.params)
        .filter(|p| (p.width, p.poly, p.refin, p.refout) == (width, poly, refin, refout) && fits_all(p))
        .collect();
    for (pin_init, value) in [(true, 0), (false, 0), (true, mask(width)), (false, mask(width))] {
        let mut pinned = rows.clone();
        let offset = if pin_init { 0 } else { w };
        pinned.extend((0..w).map(|k| (1u128 << (offset + k), (value >> k) & 1 == 1)));
        if let Some(s) = linear::solve(&pinned, 2 * w) {
            forms.push(make(s.particular));
        }
    }
    forms.push(particular);
    let mut distinct: Vec<Params> = Vec::new();
    for f in forms {
        if !distinct.iter().any(|d| equivalent(d, &f)) {
            distinct.push(f);
        }
    }
    let params = distinct[0];
    let twins = (1u32..(1 << twin_basis.len()))
        .map(|combo| {
            let delta = (0..twin_basis.len()).filter(|i| combo >> i & 1 == 1).fold(0, |d, i| d ^ twin_basis[i]);
            make(pack(&params) ^ delta)
        })
        .collect();
    let mut trace = trace.to_vec();
    trace.push(format!(
        "poly {}: {} equations in {} unknowns (init, xorout); {} free, {} of them from x+1 dividing the poly",
        hex(poly, width),
        rows.len(),
        2 * w,
        sol.kernel.len(),
        twin_basis.len()
    ));
    let alternatives = if open > 0 { distinct[1..].to_vec() } else { Vec::new() };
    Some(Fit { params, twins, open, alternatives, trace })
}

/// Changes to (init, raw xorout) that alter no CRC of any byte string: d in
/// both halves, for every d with d·(x^8 + 1) ≡ 0 mod P. If (x + 1)^k is the
/// largest power dividing P (k at most 8), those d are the multiples of
/// P/(x + 1)^k below degree w: a k-dimensional space.
fn twin_basis(poly: u64, width: u32) -> Vec<u128> {
    let w = width as usize;
    let x1 = Poly::from_u64(0b11);
    let mut q = Poly::from_u128((1u128 << w) | u128::from(poly));
    let mut k = 0;
    while k < 8 {
        let (quot, rem) = q.divrem(&x1);
        if !rem.is_zero() {
            break;
        }
        q = quot;
        k += 1;
    }
    (0..k)
        .map(|j| {
            let d = q.shl(j).low_u128();
            d | (d << w)
        })
        .collect()
}

/// One equation per CRC bit per sample, over the unknowns init (bits 0..w)
/// and xorout as it stands before output reflection (bits w..2w).
fn equations(norm: &[Sample], width: u32, poly: u64) -> Vec<(u128, bool)> {
    let w = width as usize;
    let plain = Engine::new(Params { width, poly, init: 0, refin: false, refout: false, xorout: 0 });
    let mut rows = Vec::with_capacity(norm.len() * w);
    for s in norm {
        let rhs = s.crc ^ plain.update(0, &s.msg);
        // Column j: where init bit j ends up after the message, x^(j + 8L) mod P.
        let mut cols = Vec::with_capacity(w);
        let mut c = xpow_mod(8 * s.msg.len() as u64, poly, width);
        for _ in 0..w {
            cols.push(c);
            c = mulx(c, poly, width);
        }
        for k in 0..w {
            let mut coef = 1u128 << (w + k);
            for (j, col) in cols.iter().enumerate() {
                if (col >> k) & 1 == 1 {
                    coef |= 1u128 << j;
                }
            }
            rows.push((coef, (rhs >> k) & 1 == 1));
        }
    }
    rows
}

/// v·x mod P, for v of degree below the width.
fn mulx(v: u64, poly: u64, width: u32) -> u64 {
    let top = (v >> (width - 1)) & 1;
    let v = (v << 1) & mask(width);
    if top == 1 {
        v ^ poly
    } else {
        v
    }
}

fn mulmod(a: u64, b: u64, poly: u64, width: u32) -> u64 {
    let mut r = 0;
    for i in (0..width).rev() {
        r = mulx(r, poly, width);
        if (b >> i) & 1 == 1 {
            r ^= a;
        }
    }
    r
}

/// x^n mod P by repeated squaring.
fn xpow_mod(mut n: u64, poly: u64, width: u32) -> u64 {
    // For width 1, x mod P is already a constant, and so is everything else.
    let mut result = 1;
    let mut base = mulx(1, poly, width);
    while n > 0 {
        if n & 1 == 1 {
            result = mulmod(result, base, poly, width);
        }
        base = mulmod(base, base, poly, width);
        n >>= 1;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::lookup;

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn bytes(&mut self, n: usize) -> Vec<u8> {
            (0..n).map(|_| self.next() as u8).collect()
        }
    }

    fn samples_for(p: &Params, msgs: &[Vec<u8>]) -> Vec<Sample> {
        msgs.iter().map(|m| Sample { msg: m.clone(), crc: p.checksum(m) }).collect()
    }

    /// Some fit is pinned down by the samples and computes the same function as p.
    fn pinned(fits: &[Fit], p: &Params) -> bool {
        fits.iter().any(|f| f.open == 0 && equivalent(&f.params, p))
    }

    #[test]
    fn xpow_mod_agrees_with_shifting() {
        let (poly, width) = (0x1021, 16);
        let mut v = 1u64;
        for n in 0..300u64 {
            assert_eq!(xpow_mod(n, poly, width), v, "x^{n}");
            v = mulx(v, poly, width);
        }
    }

    #[test]
    fn twins_really_are_the_same_function() {
        // CRC-16/ARC's poly is (x + 1)(x^15 + x + 1), so it has one twin:
        // init 0x8003, and xorout 0x8003 reflected, which is 0xc001.
        let arc = lookup("CRC-16/ARC").unwrap().params;
        let twin = Params { init: 0x8003, xorout: 0xc001, ..arc };
        assert!(equivalent(&arc, &twin));
        let mut rng = Rng(8);
        for n in 0..200 {
            let m = rng.bytes(n);
            assert_eq!(arc.checksum(&m), twin.checksum(&m));
        }
        // CRC-32's poly has an odd number of terms: no twin.
        let crc32 = lookup("CRC-32").unwrap().params;
        assert!(twin_basis(crc32.poly, 32).is_empty());
        assert!(!equivalent(&crc32, &Params { init: 1, xorout: reflect(1, 32) ^ crc32.xorout, ..crc32 }));
    }

    #[test]
    fn most_catalogue_models_have_a_twin() {
        let with_twin = CATALOGUE.iter().filter(|e| !twin_basis(e.params.poly, e.params.width).is_empty()).count();
        assert_eq!(with_twin, 74);
    }

    #[test]
    fn no_two_catalogue_entries_are_the_same_function() {
        for (i, a) in CATALOGUE.iter().enumerate() {
            for b in &CATALOGUE[i + 1..] {
                assert!(!equivalent(&a.params, &b.params), "{} and {}", a.name, b.name);
            }
        }
    }

    #[test]
    fn recovers_random_models_from_mixed_length_samples() {
        let mut rng = Rng(0xfeed_beef_dead_cafe);
        for width in [1u32, 3, 5, 7, 8, 11, 12, 16, 21, 24, 31, 32, 40, 63, 64] {
            for _ in 0..4 {
                let p = Params {
                    width,
                    poly: (rng.next() & mask(width)) | 1,
                    init: rng.next() & mask(width),
                    refin: rng.next() & 1 == 1,
                    refout: rng.next() & 1 == 1,
                    xorout: rng.next() & mask(width),
                };
                // Consecutive lengths: their differences rule out every
                // init/xorout pair except the twins.
                let msgs: Vec<Vec<u8>> = (0..7).map(|i| rng.bytes(4 + i)).collect();
                let fits = solve(&samples_for(&p, &msgs), width, p.refin, p.refout).unwrap();
                assert!(pinned(&fits, &p), "{p} not among {fits:?}");
            }
        }
    }

    #[test]
    fn recovers_catalogue_models_from_their_published_codewords() {
        // AUTOSAR's test messages for CRC-16/ARC, with no catalogue lookup.
        let arc = lookup("CRC-16/ARC").unwrap().params;
        let msgs: Vec<Vec<u8>> = [
            &[0, 0, 0, 0][..],
            &[0xf2, 0x01, 0x83],
            &[0x0f, 0xaa, 0x00, 0x55],
            &[0x00, 0xff, 0x55, 0x11],
            &[0x33, 0x22, 0x55, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff],
            &[0x92, 0x6b, 0x55],
            &[0xff, 0xff, 0xff, 0xff],
        ]
        .iter()
        .map(|m| m.to_vec())
        .collect();
        let fits = solve(&samples_for(&arc, &msgs), 16, true, true).unwrap();
        assert_eq!(fits.len(), 1);
        assert_eq!(fits[0].params, arc, "the catalogue form is preferred over its twin");
        assert_eq!(fits[0].open, 0);
        assert_eq!(fits[0].twins, vec![Params { init: 0x8003, xorout: 0xc001, ..arc }]);
    }

    #[test]
    fn same_length_samples_leave_init_and_xorout_open() {
        let modbus = lookup("CRC-16/MODBUS").unwrap().params;
        let mut rng = Rng(5);
        let msgs: Vec<Vec<u8>> = (0..5).map(|_| rng.bytes(6)).collect();
        let fits = solve(&samples_for(&modbus, &msgs), 16, true, true).unwrap();
        assert_eq!(fits.len(), 1);
        let fit = &fits[0];
        // 16 free bits, one of which is the twin: 2^15 different CRCs fit.
        assert_eq!(fit.open, 15);
        assert_eq!(fit.params, modbus);
        assert!(fit.alternatives.iter().any(|a| a.init == 0));
        for a in &fit.alternatives {
            assert!(!equivalent(a, &modbus));
            for m in &msgs {
                assert_eq!(a.checksum(m), modbus.checksum(m));
            }
        }
        // ...and one more sample of another length settles it.
        let mut more = msgs.clone();
        more.push(rng.bytes(9));
        let fits = solve(&samples_for(&modbus, &more), 16, true, true).unwrap();
        assert!(pinned(&fits, &modbus));
    }

    #[test]
    fn lengths_in_lockstep_with_the_poly_also_leave_it_open() {
        // x^3 + 1: x has order 3, so lengths 4, 7, 10... bytes all put init
        // through the same x^(8L) and cannot separate it from xorout.
        let p = Params { width: 3, poly: 0x1, init: 0, refin: true, refout: false, xorout: 1 };
        let mut rng = Rng(3);
        let msgs: Vec<Vec<u8>> = (0..7).map(|i| rng.bytes(4 + 3 * i)).collect();
        let fits = solve(&samples_for(&p, &msgs), 3, true, false).unwrap();
        assert!(fits
            .iter()
            .any(|f| f.open > 0 && (equivalent(&f.params, &p) || f.alternatives.iter().any(|a| equivalent(a, &p)))));
    }

    #[test]
    fn needs_enough_samples() {
        let p = lookup("CRC-32").unwrap().params;
        let same = samples_for(&p, &[b"abcd".to_vec(), b"abce".to_vec()]);
        assert_eq!(solve(&same, 32, true, true).unwrap_err(), Failure::TooFewSamples { have: 2, need: 3 });
        let mixed = samples_for(&p, &[b"a".to_vec(), b"ab".to_vec(), b"abc".to_vec()]);
        assert_eq!(solve(&mixed, 32, true, true).unwrap_err(), Failure::TooFewSamples { have: 3, need: 4 });
    }

    #[test]
    fn wrong_reflection_finds_nothing() {
        let p = lookup("CRC-16/KERMIT").unwrap().params;
        let mut rng = Rng(77);
        let msgs: Vec<Vec<u8>> = (0..6).map(|i| rng.bytes(5 + i)).collect();
        let s = samples_for(&p, &msgs);
        assert!(pinned(&solve(&s, 16, true, true).unwrap(), &p));
        assert!(solve(&s, 16, false, false).unwrap().is_empty());
        assert!(solve(&s, 16, true, false).unwrap().is_empty());
    }

    #[test]
    fn random_data_fits_nothing() {
        let mut rng = Rng(31337);
        let s: Vec<Sample> = (0..6).map(|i| Sample { msg: rng.bytes(8 + i % 2), crc: rng.next() & 0xffff }).collect();
        for (refin, refout) in [(false, false), (true, true), (false, true), (true, false)] {
            assert!(solve(&s, 16, refin, refout).unwrap().is_empty());
        }
    }

    #[test]
    fn three_distinct_lengths_are_enough_without_any_shared_length() {
        let p = lookup("CRC-24/OPENPGP").unwrap().params;
        let msgs: Vec<Vec<u8>> = (1..=6).map(|n| (0..n * 2).map(|i| (i * 37 + n) as u8).collect()).collect();
        let fits = solve(&samples_for(&p, &msgs), 24, false, false).unwrap();
        assert!(pinned(&fits, &p));
    }

    #[test]
    fn width_under_eight_and_empty_messages() {
        let p = lookup("CRC-5/USB").unwrap().params;
        let msgs: Vec<Vec<u8>> =
            vec![vec![], vec![0x12], vec![0x34, 0x56], vec![0x78, 0x9a, 0xbc], vec![0xde], vec![0xf0, 0x0d]];
        assert!(pinned(&solve(&samples_for(&p, &msgs), 5, true, true).unwrap(), &p));
    }

    #[test]
    fn long_messages_with_a_shared_prefix() {
        let p = lookup("CRC-64/XZ").unwrap().params;
        let mut rng = Rng(2024);
        let base = rng.bytes(3000);
        let msgs: Vec<Vec<u8>> = (0..5)
            .map(|i| {
                let mut m = base.clone();
                m[2990 + i] ^= 0x5a;
                m.truncate(2995 + i % 2);
                m
            })
            .collect();
        assert!(pinned(&solve(&samples_for(&p, &msgs), 64, true, true).unwrap(), &p));
    }
}
