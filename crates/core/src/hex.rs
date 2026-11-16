//! Hex formatting helpers used by the CLI and tests.

/// Lower-case hexadecimal, no separators.
pub fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Lower-case hexadecimal with a separator between each byte.
pub fn to_hex_spaced(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().saturating_mul(3));
    for (index, byte) in bytes.iter().enumerate() {
        use std::fmt::Write as _;
        if index > 0 {
            out.push(' ');
        }
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Canonical 16-bytes-per-row hexdump.
pub fn hexdump(base: u64, bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut out = String::new();
    for (row, chunk) in bytes.chunks(16).enumerate() {
        let address = base.saturating_add((row * 16) as u64);
        let _ = write!(out, "{address:016x}  ");

        for index in 0..16 {
            match chunk.get(index) {
                Some(byte) => {
                    let _ = write!(out, "{byte:02x} ");
                }
                None => out.push_str("   "),
            }
            if index == 7 {
                out.push(' ');
            }
        }

        out.push_str(" |");
        for byte in chunk {
            out.push(if byte.is_ascii_graphic() || *byte == b' ' {
                char::from(*byte)
            } else {
                '.'
            });
        }
        out.push('|');
        if row + 1 != bytes.chunks(16).count() {
            out.push('\n');
        }
    }
    out
}

/// Lossy printable rendering, one character per byte.
pub fn ascii_preview(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| {
            if byte.is_ascii_graphic() || *byte == b' ' {
                char::from(*byte)
            } else {
                '.'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hexdump_contains_offset_and_ascii() {
        let dump = hexdump(0x1000, b"FC27");
        assert!(dump.starts_with("0000000000001000"));
        assert!(dump.ends_with("|FC27|"));
    }

    #[test]
    fn spaced_hex_is_stable() {
        assert_eq!(to_hex_spaced(&[0, 0xab, 0xff]), "00 ab ff");
        assert_eq!(to_hex(&[0, 0xab, 0xff]), "00abff");
    }

    #[test]
    fn preview_replaces_control_bytes() {
        assert_eq!(ascii_preview(b"a\0b"), "a.b");
    }
}
