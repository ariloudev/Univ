//! Byte encodings shared by ingest, the server and the project's tasks.

use std::fmt::Write;

/// `bytes` as lower-case hexadecimal, two digits a byte.
#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut text, byte| {
            let _ = write!(text, "{byte:02x}");
            text
        })
}

#[cfg(test)]
mod tests {
    #[test]
    fn bytes_are_two_lower_case_hex_digits_each() {
        assert_eq!(super::hex(&[0x00, 0x0f, 0xab, 0xff]), "000fabff");
        assert_eq!(super::hex(&[]), "");
    }
}
