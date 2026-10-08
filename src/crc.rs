//! The parametrised CRC model used by the CRC catalogue (Ross Williams'
//! "Rocksoft" model) and a table-driven engine for any width from 1 to 64.

use std::fmt;

/// One CRC algorithm. `poly` leaves out the x^width term, as the catalogue does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Params {
    pub width: u32,
    pub poly: u64,
    pub init: u64,
    pub refin: bool,
    pub refout: bool,
    pub xorout: u64,
}

/// The low `width` bits set.
pub fn mask(width: u32) -> u64 {
    if width >= 64 {
        u64::MAX
    } else {
        (1u64 << width) - 1
    }
}

/// Reverse the order of the low `width` bits of `v`.
pub fn reflect(v: u64, width: u32) -> u64 {
    v.reverse_bits() >> (64 - width)
}

/// `v` in hex, zero-padded to the digits a `width`-bit value needs.
pub fn hex(v: u64, width: u32) -> String {
    format!("0x{:0digits$x}", v, digits = width.div_ceil(4) as usize)
}

/// Shift one message bit into a `width`-bit register, MSB first.
fn step(r: u64, bit: u64, poly: u64, width: u32) -> u64 {
    let feedback = ((r >> (width - 1)) & 1) ^ bit;
    let r = (r << 1) & mask(width);
    if feedback == 1 {
        r ^ poly
    } else {
        r
    }
}

impl Params {
    pub fn checksum(&self, data: &[u8]) -> u64 {
        Engine::new(*self).checksum(data)
    }

    /// The CRC of the ASCII string "123456789", the catalogue's standard check.
    pub fn check(&self) -> u64 {
        self.checksum(b"123456789")
    }

    /// What the register holds after reading any error-free codeword, before
    /// the final XOR: (xorout * x^width) mod poly, in output bit order.
    pub fn residue(&self) -> u64 {
        let w = self.width;
        let x = if self.refout { reflect(self.xorout, w) } else { self.xorout };
        let mut r = 0;
        for i in (0..w).rev() {
            r = step(r, (x >> i) & 1, self.poly, w);
        }
        if self.refout {
            reflect(r, w)
        } else {
            r
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if !(1..=64).contains(&self.width) {
            return Err(format!("width must be 1 to 64, not {}", self.width));
        }
        let m = mask(self.width);
        for (name, v) in [("poly", self.poly), ("init", self.init), ("xorout", self.xorout)] {
            if v & !m != 0 {
                return Err(format!("{name}={v:#x} does not fit in {} bits", self.width));
            }
        }
        Ok(())
    }
}

impl fmt::Display for Params {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let w = self.width;
        write!(
            f,
            "width={} poly={} init={} refin={} refout={} xorout={}",
            w,
            hex(self.poly, w),
            hex(self.init, w),
            self.refin,
            self.refout,
            hex(self.xorout, w)
        )
    }
}

/// Bit-at-a-time reference implementation, straight from the model's definition.
pub fn checksum_bitwise(p: &Params, data: &[u8]) -> u64 {
    let mut r = p.init;
    for &byte in data {
        let b = if p.refin { byte.reverse_bits() } else { byte };
        for i in (0..8).rev() {
            r = step(r, u64::from((b >> i) & 1), p.poly, p.width);
        }
    }
    if p.refout {
        r = reflect(r, p.width);
    }
    r ^ p.xorout
}

/// Table-driven CRC, eight bytes per step ("slicing-by-8"). The register is
/// kept left-aligned in a u64, so one layout serves every width from 1 to 64:
/// eight message bytes XOR straight into the register, and table k says
/// where a byte ends up after k more bytes have gone through.
pub struct Engine {
    params: Params,
    shift: u32,
    tables: Box<[[u64; 256]; 8]>,
}

impl Engine {
    pub fn new(params: Params) -> Engine {
        let shift = 64 - params.width;
        let poly = params.poly << shift;
        let mut tables = Box::new([[0u64; 256]; 8]);
        for i in 0..256 {
            let mut r = (i as u64) << 56;
            for _ in 0..8 {
                r = if r >> 63 == 1 { (r << 1) ^ poly } else { r << 1 };
            }
            tables[0][i] = r;
        }
        for k in 1..8 {
            for i in 0..256 {
                let prev = tables[k - 1][i];
                tables[k][i] = (prev << 8) ^ tables[0][(prev >> 56) as usize];
            }
        }
        Engine { params, shift, tables }
    }

    /// Run `data` through the register starting from `reg`, without the
    /// output reflection or final XOR.
    pub fn update(&self, reg: u64, data: &[u8]) -> u64 {
        let t = &self.tables;
        let refin = self.params.refin;
        let mut r = reg << self.shift;
        let mut chunks = data.chunks_exact(8);
        for chunk in &mut chunks {
            let mut d = u64::from_be_bytes(chunk.try_into().expect("8 bytes"));
            if refin {
                // Reverse the bits within each byte, keeping the byte order.
                d = d.reverse_bits().swap_bytes();
            }
            let v = r ^ d;
            r = t[7][(v >> 56) as usize]
                ^ t[6][(v >> 48) as u8 as usize]
                ^ t[5][(v >> 40) as u8 as usize]
                ^ t[4][(v >> 32) as u8 as usize]
                ^ t[3][(v >> 24) as u8 as usize]
                ^ t[2][(v >> 16) as u8 as usize]
                ^ t[1][(v >> 8) as u8 as usize]
                ^ t[0][v as u8 as usize];
        }
        for &b in chunks.remainder() {
            let b = if refin { b.reverse_bits() } else { b };
            r = (r << 8) ^ t[0][usize::from((r >> 56) as u8 ^ b)];
        }
        r >> self.shift
    }

    pub fn finish(&self, reg: u64) -> u64 {
        let p = &self.params;
        let r = if p.refout { reflect(reg, p.width) } else { reg };
        r ^ p.xorout
    }

    pub fn checksum(&self, data: &[u8]) -> u64 {
        self.finish(self.update(self.params.init, data))
    }
}

/// Parse a parameter line such as
/// `width=16 poly=0x1021 init=0xffff refin=false refout=false xorout=0`.
/// init and xorout default to 0, refin to false, refout to refin. Catalogue
/// lines also carry check= and residue=; when present they must agree.
pub fn parse_params(s: &str) -> Result<Params, String> {
    let (mut width, mut poly, mut init, mut xorout) = (None, None, 0, 0);
    let (mut refin, mut refout) = (false, None);
    let (mut check, mut residue) = (None, None);
    for tok in s.split(|c: char| c.is_whitespace() || c == ',') {
        if tok.is_empty() {
            continue;
        }
        let (key, val) = tok.split_once('=').ok_or_else(|| format!("expected key=value, got {tok:?}"))?;
        let val = val.trim_matches('"');
        match key.to_ascii_lowercase().as_str() {
            "width" => width = Some(parse_int(val)?),
            "poly" => poly = Some(parse_int(val)?),
            "init" => init = parse_int(val)?,
            "refin" => refin = parse_bool(val)?,
            "refout" => refout = Some(parse_bool(val)?),
            "xorout" => xorout = parse_int(val)?,
            "check" => check = Some(parse_int(val)?),
            "residue" => residue = Some(parse_int(val)?),
            "name" => {}
            _ => return Err(format!("unknown parameter {key:?}")),
        }
    }
    let width = width.ok_or("the parameters need width=")?;
    let poly = poly.ok_or("the parameters need poly=")?;
    if !(1..=64).contains(&width) {
        return Err(format!("width must be 1 to 64, not {width}"));
    }
    let p = Params { width: width as u32, poly, init, refin, refout: refout.unwrap_or(refin), xorout };
    p.validate()?;
    if let Some(c) = check {
        if p.check() != c {
            return Err(format!(
                "check mismatch: these parameters give check={}, the line says {}",
                hex(p.check(), p.width),
                hex(c, p.width)
            ));
        }
    }
    if let Some(r) = residue {
        if p.residue() != r {
            return Err(format!(
                "residue mismatch: these parameters give residue={}, the line says {}",
                hex(p.residue(), p.width),
                hex(r, p.width)
            ));
        }
    }
    Ok(p)
}

fn parse_int(s: &str) -> Result<u64, String> {
    let parsed = match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(h) => u64::from_str_radix(h, 16),
        None => s.parse(),
    };
    parsed.map_err(|_| format!("{s:?} is not a number (hex needs a 0x prefix)"))
}

fn parse_bool(s: &str) -> Result<bool, String> {
    match s.to_ascii_lowercase().as_str() {
        "true" | "1" => Ok(true),
        "false" | "0" => Ok(false),
        _ => Err(format!("{s:?} is not true or false")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xorshift(state: &mut u64) -> u64 {
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        *state
    }

    #[test]
    fn reflect_reverses_only_the_low_bits() {
        assert_eq!(reflect(0b0001, 4), 0b1000);
        assert_eq!(reflect(0x8005, 16), 0xa001);
        assert_eq!(reflect(0x04c11db7, 32), 0xedb88320);
        assert_eq!(reflect(1, 64), 1 << 63);
        assert_eq!(reflect(1, 1), 1);
    }

    #[test]
    fn hex_pads_to_the_width() {
        assert_eq!(hex(0x7, 3), "0x7");
        assert_eq!(hex(0x1, 12), "0x001");
        assert_eq!(hex(0xbeef, 32), "0x0000beef");
    }

    #[test]
    fn table_engine_matches_the_bitwise_definition_at_every_width() {
        let mut s = 0x2545f4914f6cdd1d;
        for width in 1..=64u32 {
            for _ in 0..20 {
                let m = mask(width);
                let p = Params {
                    width,
                    poly: xorshift(&mut s) & m,
                    init: xorshift(&mut s) & m,
                    refin: xorshift(&mut s) & 1 == 1,
                    refout: xorshift(&mut s) & 1 == 1,
                    xorout: xorshift(&mut s) & m,
                };
                let len = (xorshift(&mut s) % 40) as usize;
                let data: Vec<u8> = (0..len).map(|_| xorshift(&mut s) as u8).collect();
                assert_eq!(p.checksum(&data), checksum_bitwise(&p, &data), "{p}");
            }
        }
    }

    #[test]
    fn empty_message_gives_init_through_the_output_stage() {
        let p = parse_params("width=16 poly=0x1021 init=0x1234 refin=true refout=true xorout=0xff00").unwrap();
        assert_eq!(p.checksum(b""), reflect(0x1234, 16) ^ 0xff00);
    }

    #[test]
    fn parses_catalogue_lines_and_checks_them() {
        let line = r#"width=16  poly=0x8005  init=0xffff  refin=true  refout=true  xorout=0x0000  check=0x4b37  residue=0x0000  name="CRC-16/MODBUS""#;
        let p = parse_params(line).unwrap();
        assert_eq!((p.width, p.poly, p.init, p.refin, p.refout, p.xorout), (16, 0x8005, 0xffff, true, true, 0));
        let wrong = line.replace("check=0x4b37", "check=0x4b38");
        assert!(parse_params(&wrong).unwrap_err().contains("check mismatch"));
    }

    #[test]
    fn parse_defaults_and_errors() {
        let p = parse_params("width=8 poly=7 refin=true").unwrap();
        assert_eq!((p.init, p.refout, p.xorout), (0, true, 0));
        assert!(parse_params("poly=0x07").is_err());
        assert!(parse_params("width=8").is_err());
        assert!(parse_params("width=65 poly=1").is_err());
        assert!(parse_params("width=8 poly=0x1ff").unwrap_err().contains("does not fit"));
        assert!(parse_params("width=8 poly=ff").unwrap_err().contains("0x prefix"));
        assert!(parse_params("width=8 poly=0x07 colour=blue").unwrap_err().contains("unknown"));
        assert!(parse_params("width=8 poly=0x07 refin=maybe").is_err());
    }

    #[test]
    fn display_round_trips_through_parse() {
        let p = Params { width: 12, poly: 0x80f, init: 0, refin: false, refout: true, xorout: 0 };
        assert_eq!(p.to_string(), "width=12 poly=0x80f init=0x000 refin=false refout=true xorout=0x000");
        assert_eq!(parse_params(&p.to_string()).unwrap(), p);
    }
}
