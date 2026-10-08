//! Polynomials over GF(2), stored as bit vectors: bit i is the coefficient of
//! x^i. Just enough arithmetic to answer one question: which polynomials of
//! degree w divide this one? That takes a gcd and a factorisation
//! (square-free split, distinct-degree split, then Cantor-Zassenhaus).

use std::fmt;

#[derive(Clone, PartialEq, Eq, Hash, Default)]
pub struct Poly {
    /// Little-endian words, never with a zero word on top.
    w: Vec<u64>,
}

impl Poly {
    pub fn zero() -> Poly {
        Poly { w: Vec::new() }
    }

    pub fn one() -> Poly {
        Poly::from_u64(1)
    }

    pub fn from_u64(v: u64) -> Poly {
        Poly { w: vec![v] }.trimmed()
    }

    pub fn from_u128(v: u128) -> Poly {
        Poly { w: vec![v as u64, (v >> 64) as u64] }.trimmed()
    }

    /// x^n.
    pub fn monomial(n: usize) -> Poly {
        let mut w = vec![0; n / 64 + 1];
        w[n / 64] = 1 << (n % 64);
        Poly { w }
    }

    /// The polynomial of a message read MSB first: the first bit of the first
    /// byte is the highest power, the last bit of the last byte is x^0.
    pub fn from_bytes(bytes: &[u8]) -> Poly {
        let n = bytes.len();
        let mut w = vec![0u64; n.div_ceil(8)];
        for (j, &b) in bytes.iter().enumerate() {
            let pos = 8 * (n - 1 - j);
            w[pos / 64] |= u64::from(b) << (pos % 64);
        }
        Poly { w }.trimmed()
    }

    fn trimmed(mut self) -> Poly {
        self.trim();
        self
    }

    fn trim(&mut self) {
        while self.w.last() == Some(&0) {
            self.w.pop();
        }
    }

    pub fn is_zero(&self) -> bool {
        self.w.is_empty()
    }

    pub fn is_one(&self) -> bool {
        self.w == [1]
    }

    /// None for the zero polynomial.
    pub fn degree(&self) -> Option<usize> {
        let top = self.w.last()?;
        Some(64 * (self.w.len() - 1) + 63 - top.leading_zeros() as usize)
    }

    pub fn bit(&self, i: usize) -> bool {
        self.w.get(i / 64).is_some_and(|x| (x >> (i % 64)) & 1 == 1)
    }

    /// The low 128 coefficients.
    pub fn low_u128(&self) -> u128 {
        let lo = self.w.first().copied().unwrap_or(0);
        let hi = self.w.get(1).copied().unwrap_or(0);
        u128::from(lo) | (u128::from(hi) << 64)
    }

    /// The largest k with x^k dividing self (0 for the zero polynomial).
    pub fn trailing_zeros(&self) -> usize {
        match self.w.iter().position(|&x| x != 0) {
            Some(i) => 64 * i + self.w[i].trailing_zeros() as usize,
            None => 0,
        }
    }

    pub fn weight(&self) -> u32 {
        self.w.iter().map(|x| x.count_ones()).sum()
    }

    pub fn shl(&self, n: usize) -> Poly {
        if self.is_zero() {
            return Poly::zero();
        }
        let (ws, bs) = (n / 64, n % 64);
        let mut w = vec![0u64; self.w.len() + ws + 1];
        for (i, &x) in self.w.iter().enumerate() {
            w[i + ws] |= x << bs;
            if bs > 0 {
                w[i + ws + 1] |= x >> (64 - bs);
            }
        }
        Poly { w }.trimmed()
    }

    pub fn shr(&self, n: usize) -> Poly {
        let (ws, bs) = (n / 64, n % 64);
        if ws >= self.w.len() {
            return Poly::zero();
        }
        let w = (ws..self.w.len())
            .map(|i| {
                let hi = match self.w.get(i + 1) {
                    Some(&next) if bs > 0 => next << (64 - bs),
                    _ => 0,
                };
                (self.w[i] >> bs) | hi
            })
            .collect();
        Poly { w }.trimmed()
    }

    /// self ^= other * x^shift, growing as needed. Leaves the result untrimmed.
    fn xor_shifted(&mut self, other: &Poly, shift: usize) {
        let Some(d) = other.degree() else { return };
        let need = (d + shift) / 64 + 1;
        if self.w.len() < need {
            self.w.resize(need, 0);
        }
        let (ws, bs) = (shift / 64, shift % 64);
        for (i, &x) in other.w.iter().enumerate() {
            self.w[i + ws] ^= x << bs;
            if bs > 0 && i + ws + 1 < need {
                self.w[i + ws + 1] ^= x >> (64 - bs);
            }
        }
    }

    pub fn add(&self, other: &Poly) -> Poly {
        let mut r = self.clone();
        r.xor_shifted(other, 0);
        r.trimmed()
    }

    pub fn mul(&self, other: &Poly) -> Poly {
        if self.is_zero() || other.is_zero() {
            return Poly::zero();
        }
        // Walk the set bits of whichever operand has fewer of them.
        let (a, b) = if self.weight() <= other.weight() { (self, other) } else { (other, self) };
        let mut r = Poly { w: vec![0; a.w.len() + b.w.len() + 1] };
        for (i, &x) in a.w.iter().enumerate() {
            let mut bits = x;
            while bits != 0 {
                r.xor_shifted(b, 64 * i + bits.trailing_zeros() as usize);
                bits &= bits - 1;
            }
        }
        r.trimmed()
    }

    /// Squaring in characteristic 2 just spreads the bits out: (sum a_i x^i)^2 = sum a_i x^2i.
    pub fn square(&self) -> Poly {
        let w = self.w.iter().flat_map(|&x| [spread(x as u32), spread((x >> 32) as u32)]).collect();
        Poly { w }.trimmed()
    }

    /// The square root of a polynomial with only even powers (any odd ones are dropped).
    pub fn sqrt(&self) -> Poly {
        let w = self
            .w
            .chunks(2)
            .map(|pair| u64::from(compact(pair[0])) | u64::from(compact(*pair.get(1).unwrap_or(&0))) << 32)
            .collect();
        Poly { w }.trimmed()
    }

    /// d/dx: x^i becomes i * x^(i-1), and i is 0 or 1 mod 2.
    pub fn derivative(&self) -> Poly {
        let odd = Poly { w: self.w.iter().map(|x| x & 0xaaaa_aaaa_aaaa_aaaa).collect() };
        odd.shr(1)
    }

    pub fn divrem(&self, m: &Poly) -> (Poly, Poly) {
        let dm = m.degree().expect("division by the zero polynomial");
        let mut q = Poly::zero();
        let mut r = self.clone();
        while let Some(dr) = r.degree() {
            if dr < dm {
                break;
            }
            q.xor_shifted(&Poly::one(), dr - dm);
            r.xor_shifted(m, dr - dm);
            r.trim();
        }
        (q.trimmed(), r)
    }

    pub fn rem(&self, m: &Poly) -> Poly {
        let dm = m.degree().expect("division by the zero polynomial");
        let mut r = self.clone();
        while let Some(dr) = r.degree() {
            if dr < dm {
                break;
            }
            r.xor_shifted(m, dr - dm);
            r.trim();
        }
        r
    }

    pub fn gcd(a: &Poly, b: &Poly) -> Poly {
        let (mut a, mut b) = (a.clone(), b.clone());
        while !b.is_zero() {
            let r = a.rem(&b);
            a = b;
            b = r;
        }
        a
    }
}

impl fmt::Debug for Poly {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.degree() {
            None => write!(f, "0"),
            Some(d) if d < 128 => write!(f, "{:#x}", self.low_u128()),
            Some(d) => write!(f, "<degree {d}>"),
        }
    }
}

/// Interleave zeros: bit i of x moves to bit 2i.
fn spread(x: u32) -> u64 {
    let mut x = u64::from(x);
    x = (x | (x << 16)) & 0x0000_ffff_0000_ffff;
    x = (x | (x << 8)) & 0x00ff_00ff_00ff_00ff;
    x = (x | (x << 4)) & 0x0f0f_0f0f_0f0f_0f0f;
    x = (x | (x << 2)) & 0x3333_3333_3333_3333;
    (x | (x << 1)) & 0x5555_5555_5555_5555
}

/// The inverse of `spread`: bit 2i moves to bit i, odd bits are dropped.
fn compact(x: u64) -> u32 {
    let mut x = x & 0x5555_5555_5555_5555;
    x = (x | (x >> 1)) & 0x3333_3333_3333_3333;
    x = (x | (x >> 2)) & 0x0f0f_0f0f_0f0f_0f0f;
    x = (x | (x >> 4)) & 0x00ff_00ff_00ff_00ff;
    x = (x | (x >> 8)) & 0x0000_ffff_0000_ffff;
    ((x | (x >> 16)) & 0xffff_ffff) as u32
}

/// Square-free decomposition: f = product of g^e over the returned (g, e),
/// with each g square-free and the g pairwise coprime.
pub fn squarefree(f: &Poly) -> Vec<(Poly, u32)> {
    let mut out = Vec::new();
    squarefree_into(f, 1, &mut out);
    out
}

fn squarefree_into(f: &Poly, scale: u32, out: &mut Vec<(Poly, u32)>) {
    if f.degree().unwrap_or(0) == 0 {
        return;
    }
    // c collects the factors whose multiplicity is even, w the rest.
    let mut c = Poly::gcd(f, &f.derivative());
    let mut w = f.divrem(&c).0;
    let mut i = 1;
    while !w.is_one() {
        let y = Poly::gcd(&w, &c);
        let fac = w.divrem(&y).0;
        if !fac.is_one() {
            out.push((fac, i * scale));
        }
        c = c.divrem(&y).0;
        w = y;
        i += 1;
    }
    if !c.is_one() {
        // Everything left is a perfect square.
        squarefree_into(&c.sqrt(), 2 * scale, out);
    }
}

/// For square-free f: the product of its irreducible factors of each degree
/// d, for every d up to `max_degree` that has any. Larger factors are dropped.
pub fn distinct_degree(f: &Poly, max_degree: usize) -> Vec<(Poly, usize)> {
    let x = Poly::monomial(1);
    let mut out = Vec::new();
    let mut rest = f.clone();
    let mut h = x.rem(&rest);
    for d in 1..=max_degree {
        let r = rest.degree().unwrap_or(0);
        if r == 0 {
            break;
        }
        if r < 2 * d {
            // No room for two factors of degree >= d: what is left is irreducible.
            if r <= max_degree {
                out.push((rest, r));
            }
            return out;
        }
        h = h.square().rem(&rest); // x^(2^d) mod rest
        let g = Poly::gcd(&rest, &h.add(&x));
        if !g.is_one() {
            rest = rest.divrem(&g).0;
            h = h.rem(&rest);
            out.push((g, d));
        }
    }
    out
}

/// Deterministic xorshift, so factorisations (and output order) repeat exactly.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn poly_below(&mut self, degree: usize) -> Poly {
        let words = degree.div_ceil(64);
        let mut w: Vec<u64> = (0..words).map(|_| self.next()).collect();
        if !degree.is_multiple_of(64) {
            w[words - 1] &= (1u64 << (degree % 64)) - 1;
        }
        Poly { w }.trimmed()
    }
}

/// Split a product of distinct irreducibles, all of degree d, into the
/// factors (Cantor-Zassenhaus). Over GF(2) the splitting map is the trace
/// a + a^2 + a^4 + ... + a^(2^(d-1)), which is 0 or 1 modulo each factor.
fn equal_degree(g: &Poly, d: usize, rng: &mut Rng) -> Vec<Poly> {
    let n = g.degree().expect("nonzero");
    if n == d {
        return vec![g.clone()];
    }
    loop {
        let a = rng.poly_below(n);
        if a.degree().unwrap_or(0) == 0 {
            continue;
        }
        let mut s = a.rem(g);
        let mut t = s.clone();
        for _ in 1..d {
            s = s.square().rem(g);
            t = t.add(&s);
        }
        let h = Poly::gcd(g, &t);
        let hd = h.degree().unwrap_or(0);
        if hd > 0 && hd < n {
            let mut out = equal_degree(&h, d, rng);
            out.extend(equal_degree(&g.divrem(&h).0, d, rng));
            return out;
        }
    }
}

/// The irreducible factors of f of degree at most `max_degree`, with their
/// multiplicities.
pub fn small_factors(f: &Poly, max_degree: usize) -> Vec<(Poly, u32)> {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut out = Vec::new();
    for (part, mult) in squarefree(f) {
        for (g, d) in distinct_degree(&part, max_degree) {
            out.extend(equal_degree(&g, d, &mut rng).into_iter().map(|p| (p, mult)));
        }
    }
    out.sort_by_key(|(p, _)| (p.degree(), p.low_u128()));
    out
}

/// Every divisor of f of exactly `degree` with a constant term (that is, not
/// divisible by x), up to `limit` of them. The flag says whether it stopped early.
pub fn divisors_of_degree(f: &Poly, degree: usize, limit: usize) -> (Vec<Poly>, bool) {
    let x = Poly::monomial(1);
    let factors: Vec<(Poly, usize, u32)> = small_factors(f, degree)
        .into_iter()
        .filter(|(p, _)| *p != x)
        .map(|(p, m)| {
            let d = p.degree().expect("factor");
            (p, d, m)
        })
        .collect();
    // reach[i][r]: can the factors from i on multiply up to degree r exactly?
    let n = factors.len();
    let mut reach = vec![vec![false; degree + 1]; n + 1];
    reach[n][0] = true;
    for i in (0..n).rev() {
        let (_, d, m) = &factors[i];
        for r in 0..=degree {
            reach[i][r] = (0..=*m as usize).take_while(|k| k * d <= r).any(|k| reach[i + 1][r - k * d]);
        }
    }
    let mut found = Vec::new();
    let mut truncated = false;
    if reach[0][degree] {
        collect(&factors, &reach, 0, degree, Poly::one(), limit, &mut found, &mut truncated);
    }
    (found, truncated)
}

#[allow(clippy::too_many_arguments)]
fn collect(
    factors: &[(Poly, usize, u32)],
    reach: &[Vec<bool>],
    i: usize,
    left: usize,
    acc: Poly,
    limit: usize,
    found: &mut Vec<Poly>,
    truncated: &mut bool,
) {
    if found.len() >= limit {
        *truncated = true;
        return;
    }
    if i == factors.len() {
        found.push(acc);
        return;
    }
    let (p, d, m) = &factors[i];
    let mut acc = acc;
    for k in 0..=*m as usize {
        if k * d > left {
            break;
        }
        if reach[i + 1][left - k * d] {
            collect(factors, reach, i + 1, left - k * d, acc.clone(), limit, found, truncated);
        }
        acc = acc.mul(p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(v: u128) -> Poly {
        Poly::from_u128(v)
    }

    fn random_poly(rng: &mut Rng, degree: usize) -> Poly {
        rng.poly_below(degree).add(&Poly::monomial(degree))
    }

    /// Slow but obviously right: trial division by every lower-degree polynomial.
    fn is_irreducible(f: &Poly) -> bool {
        let d = f.degree().unwrap();
        (2u128..(1u128 << (d / 2 + 1))).all(|g| !f.rem(&p(g)).is_zero())
    }

    #[test]
    fn degree_and_bits() {
        assert_eq!(Poly::zero().degree(), None);
        assert_eq!(Poly::one().degree(), Some(0));
        assert_eq!(Poly::monomial(200).degree(), Some(200));
        assert!(Poly::monomial(200).bit(200) && !Poly::monomial(200).bit(199));
        assert_eq!(p(0b1011_0000).trailing_zeros(), 4);
    }

    #[test]
    fn from_bytes_puts_the_first_bit_highest() {
        assert_eq!(Poly::from_bytes(&[0x80, 0x01]), p(0x8001));
        assert_eq!(Poly::from_bytes(&[0x01, 0, 0, 0, 0, 0, 0, 0, 0]).degree(), Some(64));
        assert!(Poly::from_bytes(&[0, 0]).is_zero());
    }

    #[test]
    fn shifts_round_trip() {
        let a = p(0xdead_beef_1234_5678_9abc_def0_1357_9bdf);
        for n in [0, 1, 63, 64, 65, 130] {
            assert_eq!(a.shl(n).shr(n), a);
            assert_eq!(a.shl(n).degree(), Some(a.degree().unwrap() + n));
        }
        assert!(p(1).shr(1).is_zero());
    }

    #[test]
    fn small_products_by_hand() {
        // (x + 1)^2 = x^2 + 1, and (x^2 + x + 1)(x + 1) = x^3 + 1.
        assert_eq!(p(0b11).mul(&p(0b11)), p(0b101));
        assert_eq!(p(0b111).mul(&p(0b11)), p(0b1001));
        assert_eq!(p(0b111).square(), p(0b10101));
    }

    #[test]
    fn division_identity_on_random_polynomials() {
        let mut rng = Rng(12345);
        for _ in 0..200 {
            let da = (rng.next() % 300) as usize;
            let dm = (rng.next() % 150) as usize;
            let a = random_poly(&mut rng, da);
            let m = random_poly(&mut rng, dm);
            let (q, r) = a.divrem(&m);
            assert_eq!(q.mul(&m).add(&r), a);
            assert!(r.degree().is_none_or(|d| d < dm));
            assert_eq!(a.rem(&m), r);
            assert_eq!(a.square(), a.mul(&a));
            assert_eq!(a.square().sqrt(), a);
        }
    }

    #[test]
    fn gcd_recovers_a_planted_common_factor() {
        let mut rng = Rng(777);
        let common = random_poly(&mut rng, 40);
        let a = common.mul(&random_poly(&mut rng, 200));
        let b = common.mul(&random_poly(&mut rng, 230));
        let g = Poly::gcd(&a, &b);
        assert!(g.rem(&common).is_zero() || common.rem(&g).is_zero());
        assert!(a.rem(&g).is_zero() && b.rem(&g).is_zero());
        assert!(g.degree().unwrap() >= 40);
    }

    #[test]
    fn derivative_of_x5_plus_x2_plus_x() {
        // 5x^4 + 2x + 1 = x^4 + 1 over GF(2).
        assert_eq!(p(0b100110).derivative(), p(0b10001));
    }

    #[test]
    fn squarefree_finds_multiplicities() {
        // (x + 1)^3 (x^2 + x + 1)^2 (x^4 + x + 1)
        let f = p(0b11).mul(&p(0b11)).mul(&p(0b11)).mul(&p(0b111)).mul(&p(0b111)).mul(&p(0b10011));
        let mut parts = squarefree(&f);
        parts.sort_by_key(|(_, e)| *e);
        assert_eq!(parts, vec![(p(0b10011), 1), (p(0b111), 2), (p(0b11), 3)]);
    }

    #[test]
    fn squarefree_of_a_pure_square() {
        // (x^3 + x + 1)^4 has a zero derivative all the way down.
        let g = p(0b1011);
        let f = g.square().square();
        assert_eq!(squarefree(&f), vec![(g, 4)]);
    }

    #[test]
    fn x15_minus_1_factors_into_the_five_known_irreducibles() {
        let f = Poly::monomial(15).add(&Poly::one());
        let factors: Vec<Poly> = small_factors(&f, 15)
            .into_iter()
            .map(|(g, e)| {
                assert_eq!(e, 1);
                g
            })
            .collect();
        assert_eq!(factors, vec![p(0b11), p(0b111), p(0b10011), p(0b11001), p(0b11111)]);
    }

    #[test]
    fn crc16_polynomials_have_their_textbook_factors() {
        // x^16 + x^15 + x^2 + 1 = (x + 1)(x^15 + x + 1)
        let f = p(0x18005);
        assert_eq!(small_factors(&f, 16), vec![(p(0b11), 1), (p(0x8003), 1)]);
        // x^16 + x^12 + x^5 + 1 = (x + 1)(x^15 + x^14 + x^13 + x^12 + x^4 + x^3 + x^2 + x + 1)
        let g = p(0x11021);
        assert_eq!(small_factors(&g, 16), vec![(p(0b11), 1), (p(0xf01f), 1)]);
    }

    #[test]
    fn factors_multiply_back_and_are_irreducible() {
        let mut rng = Rng(4242);
        for _ in 0..40 {
            let d = 2 + (rng.next() % 18) as usize;
            let f = random_poly(&mut rng, d).add(&Poly::one()).mul(&random_poly(&mut rng, 3));
            let f = f.shr(f.trailing_zeros());
            let factors = small_factors(&f, d + 3);
            let mut product = Poly::one();
            for (g, e) in &factors {
                assert!(is_irreducible(g), "{g:?}");
                for _ in 0..*e {
                    product = product.mul(g);
                }
            }
            assert_eq!(product, f);
        }
    }

    #[test]
    fn distinct_degree_drops_factors_above_the_limit() {
        // (x^2 + x + 1)(x^5 + x^2 + 1): with a limit of 3 only the quadratic survives.
        let f = p(0b111).mul(&p(0b100101));
        assert_eq!(distinct_degree(&f, 3), vec![(p(0b111), 2)]);
        assert_eq!(distinct_degree(&f, 5), vec![(p(0b111), 2), (p(0b100101), 5)]);
    }

    #[test]
    fn divisors_of_degree_lists_each_combination_once() {
        // (x + 1)^2 (x^2 + x + 1) x: degree-2 divisors without the x are
        // (x + 1)^2 and x^2 + x + 1.
        let f = p(0b11).mul(&p(0b11)).mul(&p(0b111)).mul(&p(0b10));
        let (mut ds, truncated) = divisors_of_degree(&f, 2, 100);
        ds.sort_by_key(|d| d.low_u128());
        assert_eq!(ds, vec![p(0b101), p(0b111)]);
        assert!(!truncated);
        let (ds, truncated) = divisors_of_degree(&f, 2, 1);
        assert_eq!(ds.len(), 1);
        assert!(truncated);
        assert!(divisors_of_degree(&f, 5, 100).0.is_empty());
    }

    #[test]
    fn divisors_of_a_planted_product() {
        let mut rng = Rng(99);
        let target = p(0x1_04c1_1db7); // the CRC-32 polynomial
        let junk = random_poly(&mut rng, 25).add(&Poly::one());
        let f = target.mul(&junk);
        let (ds, _) = divisors_of_degree(&f, 32, 1000);
        assert!(ds.contains(&target));
        for d in &ds {
            assert_eq!(d.degree(), Some(32));
            assert!(f.rem(d).is_zero());
            assert!(d.bit(0));
        }
    }
}
