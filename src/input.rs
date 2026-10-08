//! Reading samples: hex frames, MESSAGE=CRC pairs, and files of either.

use crate::search::{Explicit, RawSample};

/// Parse hex bytes. Spaces, tabs, commas, colons and semicolons separate
/// groups; a group may have a 0x prefix and must have an even number of
/// digits, except that a lone digit is a byte of its own ("7" is 0x07).
pub fn parse_hex(s: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    for group in s.split(|c: char| c.is_whitespace() || matches!(c, ',' | ':' | ';')) {
        if group.is_empty() {
            continue;
        }
        let digits = group.strip_prefix("0x").or_else(|| group.strip_prefix("0X")).unwrap_or(group);
        if let Some(bad) = digits.chars().find(|c| !c.is_ascii_hexdigit()) {
            return Err(format!("{bad:?} is not a hex digit (in {group:?})"));
        }
        match digits.len() {
            0 => return Err(format!("{group:?} has no digits")),
            1 => out.push(u8::from_str_radix(digits, 16).expect("checked")),
            n if n % 2 == 1 => return Err(format!("{group:?} has an odd number of hex digits")),
            n => out.extend((0..n).step_by(2).map(|i| u8::from_str_radix(&digits[i..i + 2], 16).expect("checked"))),
        }
    }
    Ok(out)
}

/// One sample: a frame in hex with the CRC in its last bytes, or
/// MESSAGE=CRC with the CRC as a hex number. With `text`, MESSAGE is taken
/// literally and the = form is required. The last = splits the two.
pub fn parse_sample(s: &str, text: bool) -> Result<RawSample, String> {
    let Some((msg, crc)) = s.rsplit_once('=') else {
        if text {
            return Err("with --text, samples must be MESSAGE=CRC".into());
        }
        let bytes = parse_hex(s)?;
        if bytes.is_empty() {
            return Err("empty sample".into());
        }
        return Ok(RawSample { bytes, crc: None });
    };
    let crc = crc.trim();
    let digits = crc.strip_prefix("0x").or_else(|| crc.strip_prefix("0X")).unwrap_or(crc);
    if digits.is_empty() || digits.len() > 16 || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("the CRC after '=' should be 1 to 16 hex digits, not {crc:?}"));
    }
    let value = u64::from_str_radix(digits, 16).expect("checked");
    let bytes = if text { msg.as_bytes().to_vec() } else { parse_hex(msg)? };
    Ok(RawSample { bytes, crc: Some(Explicit { value, digits: digits.len() }) })
}

/// Samples one per line. Blank lines and lines starting with # are skipped;
/// in hex mode a # also ends a line early.
pub fn parse_sample_lines(content: &str, text: bool) -> Result<Vec<RawSample>, String> {
    let mut out = Vec::new();
    for (n, line) in content.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let sample = if text { line } else { line.split('#').next().unwrap_or("") };
        out.push(parse_sample(sample, text).map_err(|e| format!("line {}: {e}", n + 1))?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_in_the_shapes_people_paste() {
        let want = vec![0x01, 0x03, 0x00, 0x0a];
        for s in
            ["0103000a", "01 03 00 0a", "01:03:00:0A", "0x01,0x03,0x00,0x0a", "0103 000a", "1 3 0 a", "  01\t03 000a  "]
        {
            assert_eq!(parse_hex(s).unwrap(), want, "{s:?}");
        }
        assert_eq!(parse_hex("").unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn hex_mistakes_are_reported() {
        assert!(parse_hex("0103g0").unwrap_err().contains("'g'"));
        assert!(parse_hex("123").unwrap_err().contains("odd number"));
        assert!(parse_hex("0x").unwrap_err().contains("no digits"));
    }

    #[test]
    fn frames_and_explicit_samples() {
        let f = parse_sample("01 03 c5 cd", false).unwrap();
        assert_eq!((f.bytes, f.crc), (vec![1, 3, 0xc5, 0xcd], None));
        let e = parse_sample("313233=0x29B1", false).unwrap();
        assert_eq!(e.bytes, b"123");
        assert_eq!(e.crc, Some(Explicit { value: 0x29b1, digits: 4 }));
        let empty = parse_sample("=00", false).unwrap();
        assert!(empty.bytes.is_empty());
        assert!(parse_sample("", false).is_err());
        assert!(parse_sample("0102=", false).is_err());
        assert!(parse_sample("0102=12345678123456789", false).is_err());
    }

    #[test]
    fn text_samples_keep_everything_before_the_last_equals() {
        let s = parse_sample("a=b c=1f", true).unwrap();
        assert_eq!(s.bytes, b"a=b c");
        assert_eq!(s.crc.unwrap().value, 0x1f);
        assert!(parse_sample("no crc here", true).is_err());
    }

    #[test]
    fn sample_files_skip_comments_and_name_the_bad_line() {
        let content = "# captured frames\n\n01 02 03  # first\n  # indented comment\n04 05 06\r\n";
        let s = parse_sample_lines(content, false).unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(s[1].bytes, vec![4, 5, 6]);
        let err = parse_sample_lines("0102\nzz\n", false).unwrap_err();
        assert!(err.starts_with("line 2:"), "{err}");
        let t = parse_sample_lines("# comment\nhello # not a comment=12\n", true).unwrap();
        assert_eq!(t[0].bytes, b"hello # not a comment");
    }
}
