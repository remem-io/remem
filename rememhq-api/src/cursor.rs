//! HMAC-signed pagination cursor implementation.

use base64::{engine::general_purpose, Engine as _};
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Server-side secret for signing pagination cursors.
/// In production this should come from `REMEM_CURSOR_SECRET` env var.
/// Falls back to a deterministic default derived from `REMEM_API_KEY`
/// (or a hardcoded fallback for dev mode when no key is set).
pub fn cursor_secret() -> Vec<u8> {
    std::env::var("REMEM_CURSOR_SECRET")
        .unwrap_or_else(|_| "remem-dev-cursor-key-default".to_string())
        .into_bytes()
}

/// Encodes a numerical offset into an HMAC-signed, base64 pagination cursor.
///
/// The cursor format is: base64(offset_bytes ++ hmac_tag)
/// where offset_bytes is the little-endian u64 encoding of the offset,
/// and hmac_tag is a 32-byte HMAC-SHA256 over the offset bytes.
pub fn encode_cursor(offset: usize) -> String {
    let offset_bytes = (offset as u64).to_le_bytes();
    let mut mac =
        HmacSha256::new_from_slice(&cursor_secret()).expect("HMAC accepts any key length");
    mac.update(&offset_bytes);
    let tag = mac.finalize().into_bytes();

    let mut payload = Vec::with_capacity(8 + 32);
    payload.extend_from_slice(&offset_bytes);
    payload.extend_from_slice(&tag);
    general_purpose::STANDARD.encode(&payload)
}

/// Decodes an HMAC-signed base64 pagination cursor into a numerical offset.
///
/// Returns 0 if the cursor is missing, malformed, or has an invalid signature.
/// Invalid cursors are silently treated as "start from beginning" to avoid
/// leaking information about the signing key.
pub fn decode_cursor(cursor: Option<String>) -> usize {
    let Some(c) = cursor else { return 0 };

    let bytes = match general_purpose::STANDARD.decode(&c) {
        Ok(b) => b,
        Err(_) => return 0,
    };

    // Must be exactly 8 (offset) + 32 (HMAC-SHA256 tag) = 40 bytes
    if bytes.len() != 40 {
        return 0;
    }

    let (offset_bytes, tag_bytes) = bytes.split_at(8);

    let mut mac =
        HmacSha256::new_from_slice(&cursor_secret()).expect("HMAC accepts any key length");
    mac.update(offset_bytes);

    if mac.verify_slice(tag_bytes).is_err() {
        tracing::warn!("Received pagination cursor with invalid HMAC signature");
        return 0;
    }

    let offset_array: [u8; 8] = offset_bytes.try_into().unwrap();
    u64::from_le_bytes(offset_array) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cursor_roundtrip() {
        let offset = 42usize;
        let encoded = encode_cursor(offset);
        let decoded = decode_cursor(Some(encoded));
        assert_eq!(decoded, offset);
    }

    #[test]
    fn test_cursor_none_returns_zero() {
        assert_eq!(decode_cursor(None), 0);
    }

    #[test]
    fn test_cursor_tampered_returns_zero() {
        let encoded = encode_cursor(42);
        let mut chars: Vec<char> = encoded.chars().collect();
        if let Some(c) = chars.get_mut(5) {
            *c = if *c == 'A' { 'B' } else { 'A' };
        }
        let tampered: String = chars.into_iter().collect();
        assert_eq!(decode_cursor(Some(tampered)), 0);
    }

    #[test]
    fn test_cursor_invalid_base64_returns_zero() {
        assert_eq!(decode_cursor(Some("not-valid-base64!!!".to_string())), 0);
    }

    #[test]
    fn test_cursor_zero_offset() {
        let encoded = encode_cursor(0);
        let decoded = decode_cursor(Some(encoded));
        assert_eq!(decoded, 0);
    }

    #[test]
    fn test_cursor_large_offset() {
        let offset = 1_000_000usize;
        let encoded = encode_cursor(offset);
        let decoded = decode_cursor(Some(encoded));
        assert_eq!(decoded, offset);
    }
}
