//! The CRC catalogue: every model of width 64 or less from Greg Cook's
//! "Catalogue of parametrised CRC algorithms" (reveng.sourceforge.io), with
//! its published check and residue values and its aliases.

use crate::crc::Params;

#[derive(Debug)]
pub struct Entry {
    pub name: &'static str,
    pub params: Params,
    pub check: u64,
    pub residue: u64,
    pub aliases: &'static [&'static str],
}

#[allow(clippy::too_many_arguments)]
const fn m(
    name: &'static str,
    width: u32,
    poly: u64,
    init: u64,
    refin: bool,
    refout: bool,
    xorout: u64,
    check: u64,
    residue: u64,
    aliases: &'static [&'static str],
) -> Entry {
    Entry { name, params: Params { width, poly, init, refin, refout, xorout }, check, residue, aliases }
}

/// Find a model by name or alias, ignoring case. The part after the slash is
/// enough when only one model has it ("modbus", "ibm-3740").
pub fn lookup(name: &str) -> Result<&'static Entry, String> {
    let want = name.trim().to_ascii_uppercase();
    if let Some(e) = CATALOGUE.iter().find(|e| e.name == want || e.aliases.contains(&want.as_str())) {
        return Ok(e);
    }
    let after_slash = |s: &str| s.rsplit('/').next().unwrap_or(s).to_string();
    let hits: Vec<&Entry> = CATALOGUE
        .iter()
        .filter(|e| after_slash(e.name) == want || e.aliases.iter().any(|a| after_slash(a) == want))
        .collect();
    match hits.as_slice() {
        [one] => Ok(one),
        [] => Err(format!("no catalogue model is called {name:?}; `whichcrc list` shows them all")),
        many => {
            Err(format!("{name:?} could be any of: {}", many.iter().map(|e| e.name).collect::<Vec<_>>().join(", ")))
        }
    }
}

/// The catalogue entry that computes the same function as `p`, if any: the
/// same parameters, or a twin of them (see `solve::equivalent`).
pub fn identify(p: &Params) -> Option<&'static Entry> {
    CATALOGUE.iter().find(|e| crate::solve::equivalent(&e.params, p))
}

#[rustfmt::skip]
pub static CATALOGUE: &[Entry] = &[
    m("CRC-3/GSM", 3, 0x3, 0x0, false, false, 0x7, 0x4, 0x2, &[]),
    m("CRC-3/ROHC", 3, 0x3, 0x7, true, true, 0x0, 0x6, 0x0, &[]),
    m("CRC-4/G-704", 4, 0x3, 0x0, true, true, 0x0, 0x7, 0x0, &["CRC-4/ITU"]),
    m("CRC-4/INTERLAKEN", 4, 0x3, 0xf, false, false, 0xf, 0xb, 0x2, &[]),
    m("CRC-5/EPC-C1G2", 5, 0x09, 0x09, false, false, 0x00, 0x00, 0x00, &["CRC-5/EPC"]),
    m("CRC-5/G-704", 5, 0x15, 0x00, true, true, 0x00, 0x07, 0x00, &["CRC-5/ITU"]),
    m("CRC-5/USB", 5, 0x05, 0x1f, true, true, 0x1f, 0x19, 0x06, &[]),
    m("CRC-6/CDMA2000-A", 6, 0x27, 0x3f, false, false, 0x00, 0x0d, 0x00, &[]),
    m("CRC-6/CDMA2000-B", 6, 0x07, 0x3f, false, false, 0x00, 0x3b, 0x00, &[]),
    m("CRC-6/DARC", 6, 0x19, 0x00, true, true, 0x00, 0x26, 0x00, &[]),
    m("CRC-6/G-704", 6, 0x03, 0x00, true, true, 0x00, 0x06, 0x00, &["CRC-6/ITU"]),
    m("CRC-6/GSM", 6, 0x2f, 0x00, false, false, 0x3f, 0x13, 0x3a, &[]),
    m("CRC-7/MMC", 7, 0x09, 0x00, false, false, 0x00, 0x75, 0x00, &["CRC-7"]),
    m("CRC-7/ROHC", 7, 0x4f, 0x7f, true, true, 0x00, 0x53, 0x00, &[]),
    m("CRC-7/UMTS", 7, 0x45, 0x00, false, false, 0x00, 0x61, 0x00, &[]),
    m("CRC-8/AUTOSAR", 8, 0x2f, 0xff, false, false, 0xff, 0xdf, 0x42, &[]),
    m("CRC-8/BLUETOOTH", 8, 0xa7, 0x00, true, true, 0x00, 0x26, 0x00, &[]),
    m("CRC-8/CDMA2000", 8, 0x9b, 0xff, false, false, 0x00, 0xda, 0x00, &[]),
    m("CRC-8/DARC", 8, 0x39, 0x00, true, true, 0x00, 0x15, 0x00, &[]),
    m("CRC-8/DVB-S2", 8, 0xd5, 0x00, false, false, 0x00, 0xbc, 0x00, &[]),
    m("CRC-8/GSM-A", 8, 0x1d, 0x00, false, false, 0x00, 0x37, 0x00, &[]),
    m("CRC-8/GSM-B", 8, 0x49, 0x00, false, false, 0xff, 0x94, 0x53, &[]),
    m("CRC-8/HITAG", 8, 0x1d, 0xff, false, false, 0x00, 0xb4, 0x00, &[]),
    m("CRC-8/I-432-1", 8, 0x07, 0x00, false, false, 0x55, 0xa1, 0xac, &["CRC-8/ITU"]),
    m("CRC-8/I-CODE", 8, 0x1d, 0xfd, false, false, 0x00, 0x7e, 0x00, &[]),
    m("CRC-8/LTE", 8, 0x9b, 0x00, false, false, 0x00, 0xea, 0x00, &[]),
    m("CRC-8/MAXIM-DOW", 8, 0x31, 0x00, true, true, 0x00, 0xa1, 0x00, &["CRC-8/MAXIM", "DOW-CRC"]),
    m("CRC-8/MIFARE-MAD", 8, 0x1d, 0xc7, false, false, 0x00, 0x99, 0x00, &[]),
    m("CRC-8/NRSC-5", 8, 0x31, 0xff, false, false, 0x00, 0xf7, 0x00, &[]),
    m("CRC-8/OPENSAFETY", 8, 0x2f, 0x00, false, false, 0x00, 0x3e, 0x00, &[]),
    m("CRC-8/ROHC", 8, 0x07, 0xff, true, true, 0x00, 0xd0, 0x00, &[]),
    m("CRC-8/SAE-J1850", 8, 0x1d, 0xff, false, false, 0xff, 0x4b, 0xc4, &[]),
    m("CRC-8/SMBUS", 8, 0x07, 0x00, false, false, 0x00, 0xf4, 0x00, &["CRC-8"]),
    m("CRC-8/TECH-3250", 8, 0x1d, 0xff, true, true, 0x00, 0x97, 0x00, &["CRC-8/AES", "CRC-8/EBU"]),
    m("CRC-8/WCDMA", 8, 0x9b, 0x00, true, true, 0x00, 0x25, 0x00, &[]),
    m("CRC-10/ATM", 10, 0x233, 0x000, false, false, 0x000, 0x199, 0x000, &["CRC-10", "CRC-10/I-610"]),
    m("CRC-10/CDMA2000", 10, 0x3d9, 0x3ff, false, false, 0x000, 0x233, 0x000, &[]),
    m("CRC-10/GSM", 10, 0x175, 0x000, false, false, 0x3ff, 0x12a, 0x0c6, &[]),
    m("CRC-11/FLEXRAY", 11, 0x385, 0x01a, false, false, 0x000, 0x5a3, 0x000, &["CRC-11"]),
    m("CRC-11/UMTS", 11, 0x307, 0x000, false, false, 0x000, 0x061, 0x000, &[]),
    m("CRC-12/CDMA2000", 12, 0xf13, 0xfff, false, false, 0x000, 0xd4d, 0x000, &[]),
    m("CRC-12/DECT", 12, 0x80f, 0x000, false, false, 0x000, 0xf5b, 0x000, &["X-CRC-12"]),
    m("CRC-12/GSM", 12, 0xd31, 0x000, false, false, 0xfff, 0xb34, 0x178, &[]),
    m("CRC-12/UMTS", 12, 0x80f, 0x000, false, true, 0x000, 0xdaf, 0x000, &["CRC-12/3GPP"]),
    m("CRC-13/BBC", 13, 0x1cf5, 0x0000, false, false, 0x0000, 0x04fa, 0x0000, &[]),
    m("CRC-14/DARC", 14, 0x0805, 0x0000, true, true, 0x0000, 0x082d, 0x0000, &[]),
    m("CRC-14/GSM", 14, 0x202d, 0x0000, false, false, 0x3fff, 0x30ae, 0x031e, &[]),
    m("CRC-15/CAN", 15, 0x4599, 0x0000, false, false, 0x0000, 0x059e, 0x0000, &["CRC-15"]),
    m("CRC-15/MPT1327", 15, 0x6815, 0x0000, false, false, 0x0001, 0x2566, 0x6815, &[]),
    m("CRC-16/ARC", 16, 0x8005, 0x0000, true, true, 0x0000, 0xbb3d, 0x0000, &["ARC", "CRC-16", "CRC-16/LHA", "CRC-IBM"]),
    m("CRC-16/CDMA2000", 16, 0xc867, 0xffff, false, false, 0x0000, 0x4c06, 0x0000, &[]),
    m("CRC-16/CMS", 16, 0x8005, 0xffff, false, false, 0x0000, 0xaee7, 0x0000, &[]),
    m("CRC-16/DDS-110", 16, 0x8005, 0x800d, false, false, 0x0000, 0x9ecf, 0x0000, &[]),
    m("CRC-16/DECT-R", 16, 0x0589, 0x0000, false, false, 0x0001, 0x007e, 0x0589, &["R-CRC-16"]),
    m("CRC-16/DECT-X", 16, 0x0589, 0x0000, false, false, 0x0000, 0x007f, 0x0000, &["X-CRC-16"]),
    m("CRC-16/DNP", 16, 0x3d65, 0x0000, true, true, 0xffff, 0xea82, 0x66c5, &[]),
    m("CRC-16/EN-13757", 16, 0x3d65, 0x0000, false, false, 0xffff, 0xc2b7, 0xa366, &[]),
    m("CRC-16/GENIBUS", 16, 0x1021, 0xffff, false, false, 0xffff, 0xd64e, 0x1d0f, &["CRC-16/DARC", "CRC-16/EPC", "CRC-16/EPC-C1G2", "CRC-16/I-CODE"]),
    m("CRC-16/GSM", 16, 0x1021, 0x0000, false, false, 0xffff, 0xce3c, 0x1d0f, &[]),
    m("CRC-16/IBM-3740", 16, 0x1021, 0xffff, false, false, 0x0000, 0x29b1, 0x0000, &["CRC-16/AUTOSAR", "CRC-16/CCITT-FALSE"]),
    m("CRC-16/IBM-SDLC", 16, 0x1021, 0xffff, true, true, 0xffff, 0x906e, 0xf0b8, &["CRC-16/ISO-HDLC", "CRC-16/ISO-IEC-14443-3-B", "CRC-16/X-25", "CRC-B", "X-25"]),
    m("CRC-16/ISO-IEC-14443-3-A", 16, 0x1021, 0xc6c6, true, true, 0x0000, 0xbf05, 0x0000, &["CRC-A"]),
    m("CRC-16/KERMIT", 16, 0x1021, 0x0000, true, true, 0x0000, 0x2189, 0x0000, &["CRC-16/BLUETOOTH", "CRC-16/CCITT", "CRC-16/CCITT-TRUE", "CRC-16/V-41-LSB", "CRC-CCITT", "KERMIT"]),
    m("CRC-16/LJ1200", 16, 0x6f63, 0x0000, false, false, 0x0000, 0xbdf4, 0x0000, &[]),
    m("CRC-16/M17", 16, 0x5935, 0xffff, false, false, 0x0000, 0x772b, 0x0000, &[]),
    m("CRC-16/MAXIM-DOW", 16, 0x8005, 0x0000, true, true, 0xffff, 0x44c2, 0xb001, &["CRC-16/MAXIM"]),
    m("CRC-16/MCRF4XX", 16, 0x1021, 0xffff, true, true, 0x0000, 0x6f91, 0x0000, &[]),
    m("CRC-16/MODBUS", 16, 0x8005, 0xffff, true, true, 0x0000, 0x4b37, 0x0000, &["MODBUS"]),
    m("CRC-16/NRSC-5", 16, 0x080b, 0xffff, true, true, 0x0000, 0xa066, 0x0000, &[]),
    m("CRC-16/OPENSAFETY-A", 16, 0x5935, 0x0000, false, false, 0x0000, 0x5d38, 0x0000, &[]),
    m("CRC-16/OPENSAFETY-B", 16, 0x755b, 0x0000, false, false, 0x0000, 0x20fe, 0x0000, &[]),
    m("CRC-16/PROFIBUS", 16, 0x1dcf, 0xffff, false, false, 0xffff, 0xa819, 0xe394, &["CRC-16/IEC-61158-2"]),
    m("CRC-16/RIELLO", 16, 0x1021, 0xb2aa, true, true, 0x0000, 0x63d0, 0x0000, &[]),
    m("CRC-16/SPI-FUJITSU", 16, 0x1021, 0x1d0f, false, false, 0x0000, 0xe5cc, 0x0000, &["CRC-16/AUG-CCITT"]),
    m("CRC-16/T10-DIF", 16, 0x8bb7, 0x0000, false, false, 0x0000, 0xd0db, 0x0000, &[]),
    m("CRC-16/TELEDISK", 16, 0xa097, 0x0000, false, false, 0x0000, 0x0fb3, 0x0000, &[]),
    m("CRC-16/TMS37157", 16, 0x1021, 0x89ec, true, true, 0x0000, 0x26b1, 0x0000, &[]),
    m("CRC-16/UMTS", 16, 0x8005, 0x0000, false, false, 0x0000, 0xfee8, 0x0000, &["CRC-16/BUYPASS", "CRC-16/VERIFONE"]),
    m("CRC-16/USB", 16, 0x8005, 0xffff, true, true, 0xffff, 0xb4c8, 0xb001, &[]),
    m("CRC-16/XMODEM", 16, 0x1021, 0x0000, false, false, 0x0000, 0x31c3, 0x0000, &["CRC-16/ACORN", "CRC-16/LTE", "CRC-16/V-41-MSB", "XMODEM", "ZMODEM"]),
    m("CRC-17/CAN-FD", 17, 0x1685b, 0x00000, false, false, 0x00000, 0x04f03, 0x00000, &[]),
    m("CRC-21/CAN-FD", 21, 0x102899, 0x000000, false, false, 0x000000, 0x0ed841, 0x000000, &[]),
    m("CRC-24/BLE", 24, 0x00065b, 0x555555, true, true, 0x000000, 0xc25a56, 0x000000, &[]),
    m("CRC-24/FLEXRAY-A", 24, 0x5d6dcb, 0xfedcba, false, false, 0x000000, 0x7979bd, 0x000000, &[]),
    m("CRC-24/FLEXRAY-B", 24, 0x5d6dcb, 0xabcdef, false, false, 0x000000, 0x1f23b8, 0x000000, &[]),
    m("CRC-24/INTERLAKEN", 24, 0x328b63, 0xffffff, false, false, 0xffffff, 0xb4f3e6, 0x144e63, &[]),
    m("CRC-24/LTE-A", 24, 0x864cfb, 0x000000, false, false, 0x000000, 0xcde703, 0x000000, &[]),
    m("CRC-24/LTE-B", 24, 0x800063, 0x000000, false, false, 0x000000, 0x23ef52, 0x000000, &[]),
    m("CRC-24/OPENPGP", 24, 0x864cfb, 0xb704ce, false, false, 0x000000, 0x21cf02, 0x000000, &["CRC-24"]),
    m("CRC-24/OS-9", 24, 0x800063, 0xffffff, false, false, 0xffffff, 0x200fa5, 0x800fe3, &[]),
    m("CRC-30/CDMA", 30, 0x2030b9c7, 0x3fffffff, false, false, 0x3fffffff, 0x04c34abf, 0x34efa55a, &[]),
    m("CRC-31/PHILIPS", 31, 0x04c11db7, 0x7fffffff, false, false, 0x7fffffff, 0x0ce9e46c, 0x4eaf26f1, &[]),
    m("CRC-32/AIXM", 32, 0x814141ab, 0x00000000, false, false, 0x00000000, 0x3010bf7f, 0x00000000, &["CRC-32Q"]),
    m("CRC-32/AUTOSAR", 32, 0xf4acfb13, 0xffffffff, true, true, 0xffffffff, 0x1697d06a, 0x904cddbf, &[]),
    m("CRC-32/BASE91-D", 32, 0xa833982b, 0xffffffff, true, true, 0xffffffff, 0x87315576, 0x45270551, &["CRC-32D"]),
    m("CRC-32/BZIP2", 32, 0x04c11db7, 0xffffffff, false, false, 0xffffffff, 0xfc891918, 0xc704dd7b, &["CRC-32/AAL5", "CRC-32/DECT-B", "B-CRC-32"]),
    m("CRC-32/CD-ROM-EDC", 32, 0x8001801b, 0x00000000, true, true, 0x00000000, 0x6ec2edc4, 0x00000000, &[]),
    m("CRC-32/CKSUM", 32, 0x04c11db7, 0x00000000, false, false, 0xffffffff, 0x765e7680, 0xc704dd7b, &["CKSUM", "CRC-32/POSIX"]),
    m("CRC-32/ISCSI", 32, 0x1edc6f41, 0xffffffff, true, true, 0xffffffff, 0xe3069283, 0xb798b438, &["CRC-32/BASE91-C", "CRC-32/CASTAGNOLI", "CRC-32/INTERLAKEN", "CRC-32C", "CRC-32/NVME"]),
    m("CRC-32/ISO-HDLC", 32, 0x04c11db7, 0xffffffff, true, true, 0xffffffff, 0xcbf43926, 0xdebb20e3, &["CRC-32", "CRC-32/ADCCP", "CRC-32/V-42", "CRC-32/XZ", "PKZIP"]),
    m("CRC-32/JAMCRC", 32, 0x04c11db7, 0xffffffff, true, true, 0x00000000, 0x340bc6d9, 0x00000000, &["JAMCRC"]),
    m("CRC-32/MEF", 32, 0x741b8cd7, 0xffffffff, true, true, 0x00000000, 0xd2c22f51, 0x00000000, &[]),
    m("CRC-32/MPEG-2", 32, 0x04c11db7, 0xffffffff, false, false, 0x00000000, 0x0376e6e7, 0x00000000, &[]),
    m("CRC-32/XFER", 32, 0x000000af, 0x00000000, false, false, 0x00000000, 0xbd0be338, 0x00000000, &["XFER"]),
    m("CRC-40/GSM", 40, 0x0004820009, 0x0000000000, false, false, 0xffffffffff, 0xd4164fc646, 0xc4ff8071ff, &[]),
    m("CRC-64/ECMA-182", 64, 0x42f0e1eba9ea3693, 0x0000000000000000, false, false, 0x0000000000000000, 0x6c40df5f0b497347, 0x0000000000000000, &["CRC-64"]),
    m("CRC-64/GO-ISO", 64, 0x000000000000001b, 0xffffffffffffffff, true, true, 0xffffffffffffffff, 0xb90956c775a41001, 0x5300000000000000, &[]),
    m("CRC-64/MS", 64, 0x259c84cba6426349, 0xffffffffffffffff, true, true, 0x0000000000000000, 0x75d4b74f024eceea, 0x0000000000000000, &[]),
    m("CRC-64/NVME", 64, 0xad93d23594c93659, 0xffffffffffffffff, true, true, 0xffffffffffffffff, 0xae8b14860a799888, 0xf310303b2b6f6e42, &[]),
    m("CRC-64/REDIS", 64, 0xad93d23594c935a9, 0x0000000000000000, true, true, 0x0000000000000000, 0xe9c6d914c4b8d9ca, 0x0000000000000000, &[]),
    m("CRC-64/WE", 64, 0x42f0e1eba9ea3693, 0xffffffffffffffff, false, false, 0xffffffffffffffff, 0x62ec59e3f1a4f00a, 0xfcacbebd5931a992, &[]),
    m("CRC-64/XZ", 64, 0x42f0e1eba9ea3693, 0xffffffffffffffff, true, true, 0xffffffffffffffff, 0x995dc9bbdf1939fa, 0x49958c9abd7d353f, &["CRC-64/GO-ECMA"]),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crc::{checksum_bitwise, Engine};
    use std::collections::HashSet;

    fn bytes(hex: &str) -> Vec<u8> {
        (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()).collect()
    }

    #[test]
    fn every_entry_reproduces_its_published_check_and_residue() {
        for e in CATALOGUE {
            let p = &e.params;
            p.validate().unwrap();
            assert_eq!(p.check(), e.check, "{} check", e.name);
            assert_eq!(checksum_bitwise(p, b"123456789"), e.check, "{} bitwise check", e.name);
            assert_eq!(p.residue(), e.residue, "{} residue", e.name);
        }
    }

    #[test]
    fn names_and_aliases_are_unique_and_upper_case() {
        let mut seen = HashSet::new();
        for e in CATALOGUE {
            for n in std::iter::once(&e.name).chain(e.aliases) {
                assert!(seen.insert(*n), "{n} appears twice");
                assert_eq!(*n, n.to_ascii_uppercase());
            }
        }
        assert_eq!(CATALOGUE.len(), 112);
        assert!(CATALOGUE.iter().all(|e| e.params.width <= 64));
    }

    #[test]
    fn no_two_entries_share_parameters() {
        let set: HashSet<_> = CATALOGUE.iter().map(|e| e.params).collect();
        assert_eq!(set.len(), CATALOGUE.len());
    }

    #[test]
    fn lookup_by_name_alias_and_suffix() {
        assert_eq!(lookup("crc-16/modbus").unwrap().name, "CRC-16/MODBUS");
        assert_eq!(lookup("CRC-32").unwrap().name, "CRC-32/ISO-HDLC");
        assert_eq!(lookup("crc-32c").unwrap().name, "CRC-32/ISCSI");
        assert_eq!(lookup("X-25").unwrap().name, "CRC-16/IBM-SDLC");
        assert_eq!(lookup("ibm-3740").unwrap().name, "CRC-16/IBM-3740");
        assert_eq!(lookup(" ccitt-false ").unwrap().name, "CRC-16/IBM-3740");
        let err = lookup("xz").unwrap_err();
        assert!(err.contains("CRC-32/ISO-HDLC") && err.contains("CRC-64/XZ"), "{err}");
        assert!(lookup("CRC-16/NOPE").unwrap_err().contains("whichcrc list"));
    }

    #[test]
    fn identify_knows_a_model_by_its_twin_too() {
        let modbus = lookup("modbus").unwrap();
        assert_eq!(identify(&modbus.params).unwrap().name, "CRC-16/MODBUS");
        // 0x8005 = (x + 1)(x^15 + x + 1): shifting init by 0x8003 and xorout
        // by its reflection, 0xc001, changes nothing.
        let twin = Params { init: 0xffff ^ 0x8003, xorout: 0xc001, ..modbus.params };
        assert_eq!(identify(&twin).unwrap().name, "CRC-16/MODBUS");
        let mut p = modbus.params;
        p.init = 0x1234;
        assert!(identify(&p).is_none());
    }

    /// Codewords (message followed by its CRC) quoted in the catalogue from
    /// the standards themselves: AUTOSAR's CRC library spec, ISO 14443,
    /// T10, Bluetooth and others. Reflected CRCs are sent low byte first.
    #[test]
    fn published_codewords_check_out() {
        let cases = [
            ("CRC-8/SAE-J1850", "0000000059"),
            ("CRC-8/SAE-J1850", "332255AABBCCDDEEFFCB"),
            ("CRC-8/AUTOSAR", "0FAA0055C6"),
            ("CRC-8/AUTOSAR", "926B5533"),
            ("CRC-16/ARC", "0FAA0055E30B"),
            ("CRC-16/ARC", "332255AABBCCDDEEFF98AE"),
            ("CRC-16/IBM-3740", "0000000084C0"),
            ("CRC-16/IBM-3740", "F20183D374"),
            ("CRC-16/XMODEM", "4361744D6F757365393837363534333231E556"),
            ("CRC-16/USB", "00010203EF7A"),
            ("CRC-16/ISO-IEC-14443-3-A", "AABBCCDDEE0011227B09"),
            ("CRC-16/T10-DIF", "000102030405060708090A0B0C0D0E0F101112131415161718191A1B1C1D1E1F0224"),
            ("CRC-16/GENIBUS", "2800111122223333444455552A91"),
            ("CRC-24/BLE", "0003424C45290ACE"),
            ("CRC-24/FLEXRAY-A", "600A0248C80102646D70"),
            ("CRC-32/ISO-HDLC", "000000001CDF4421"),
            ("CRC-32/ISO-HDLC", "332255AABBCCDDEEFF3D86AEB0"),
            ("CRC-32/AUTOSAR", "0FAA0055F82D6620"),
            ("CRC-32/ISCSI", "000102030405060708090A0B0C0D0E0F101112131415161718191A1B1C1D1E1F4E79DD46"),
            ("CRC-32/BZIP2", "6173640A86FA4F5B"),
            ("CRC-64/XZ", "F20183C6F1648166279C31"),
            ("CRC-64/GO-ISO", "6162636465000020C47667D332"),
        ];
        for (name, word) in cases {
            let e = lookup(name).unwrap();
            let w = bytes(word);
            let n = (e.params.width as usize).div_ceil(8);
            let (msg, tail) = w.split_at(w.len() - n);
            let fold = |acc: u64, b: &u8| (acc << 8) | u64::from(*b);
            let crc = if e.params.refout { tail.iter().rev().fold(0, fold) } else { tail.iter().fold(0, fold) };
            assert_eq!(Engine::new(e.params).checksum(msg), crc, "{name} {word}");
        }
    }
}
