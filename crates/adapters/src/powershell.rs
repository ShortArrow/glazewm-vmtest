//! Text that Windows PowerShell reads.

/// `script` in the form `powershell -EncodedCommand` takes: Base64 of its UTF-16LE.
///
/// The result has no whitespace or quotes, so it survives command lines that split on whitespace and drop quotes,
/// such as glazewm's `shell-exec`.
pub fn encoded_command(script: &str) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut encoded = String::new();
    for group in bytes.chunks(3) {
        let bits = group.iter().enumerate().fold(0u32, |bits, (index, &byte)| {
            bits | u32::from(byte) << (16 - 8 * index)
        });
        for index in 0..4 {
            encoded.push(if index <= group.len() {
                char::from(ALPHABET[(bits >> (18 - 6 * index) & 0x3f) as usize])
            } else {
                '='
            });
        }
    }
    encoded
}

/// `text` as a PowerShell single-quoted string.
pub fn literal(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_a_script_as_base64_of_its_utf16le() {
        assert_eq!(encoded_command("dir"), "ZABpAHIA");
        assert_eq!(encoded_command("ab"), "YQBiAA==");
    }

    #[test]
    fn encodes_characters_outside_ascii_and_pads_the_last_group() {
        assert_eq!(encoded_command("é"), "6QA=");
    }

    #[test]
    fn doubles_single_quotes_in_a_literal() {
        assert_eq!(literal("it's"), "'it''s'");
    }
}
