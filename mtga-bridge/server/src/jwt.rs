//! A structurally-valid but unsigned JWT.
//!
//! The MTGA client treats the platform login token as a JWT: it base64url-decodes
//! the payload and reads claims (`wotc-rols`, `exp`, `sub`), but it does **not**
//! verify the signature against any key. So we mint `header.payload.sig` with a
//! real (parseable) header + payload and a throwaway, non-empty signature segment.
//!
//! The claims that matter:
//!   - `wotc-rols`: a non-empty role array (we grant `MTGA_DEBUG`).
//!   - `exp`: a unix-seconds expiry in the future (we use now + 10 years).
//!   - `sub`: the persona id.

use std::time::{SystemTime, UNIX_EPOCH};

const TEN_YEARS_SECS: u64 = 10 * 365 * 24 * 60 * 60;

/// Mint an unsigned JWT for `persona_id`. Returns `header.payload.sig`.
pub fn mint(persona_id: &str) -> String {
    let header = r#"{"alg":"none","typ":"JWT"}"#;
    let exp = now_unix_secs() + TEN_YEARS_SECS;
    // Minimal hand-built JSON; persona ids are UUID-like so no escaping needed,
    // but escape quotes/backslashes defensively in case a caller passes oddities.
    let sub = json_escape(persona_id);
    let payload = format!(
        r#"{{"wotc-rols":["MTGA_DEBUG"],"exp":{exp},"sub":"{sub}","iss":"mtga-bridge"}}"#
    );

    let h = b64url(header.as_bytes());
    let p = b64url(payload.as_bytes());
    // Non-empty signature segment. The literal "stub" is valid base64url and its
    // length (4) is not %4==1, satisfying the well-formedness guard.
    let s = "stub".to_string();

    debug_assert_ne!(h.len() % 4, 1, "header segment length must not be %4==1");
    debug_assert_ne!(p.len() % 4, 1, "payload segment length must not be %4==1");
    debug_assert_ne!(s.len() % 4, 1, "sig segment length must not be %4==1");
    debug_assert!(!s.is_empty(), "sig segment must be non-empty");

    format!("{h}.{p}.{s}")
}

/// Current unix time in seconds. This is a normal binary (not a sandboxed
/// workflow), so `SystemTime::now()` is fine.
fn now_unix_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Unpadded base64url (RFC 4648 §5, no `=`).
fn b64url(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity((input.len() * 4 + 2) / 3);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[((n >> 18) & 0x3f) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[((n >> 6) & 0x3f) as usize] as char);
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(n & 0x3f) as usize] as char);
        }
    }
    out
}

/// Minimal JSON string-body escaping for the few control chars / quotes.
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decode unpadded base64url back to bytes (test-only helper).
    fn b64url_decode(s: &str) -> Vec<u8> {
        fn val(c: u8) -> u32 {
            match c {
                b'A'..=b'Z' => (c - b'A') as u32,
                b'a'..=b'z' => (c - b'a' + 26) as u32,
                b'0'..=b'9' => (c - b'0' + 52) as u32,
                b'-' => 62,
                b'_' => 63,
                _ => panic!("bad base64url char {c:?}"),
            }
        }
        let bytes = s.as_bytes();
        let mut out = Vec::new();
        for chunk in bytes.chunks(4) {
            let mut acc = 0u32;
            for &c in chunk {
                acc = (acc << 6) | val(c);
            }
            acc <<= 6 * (4 - chunk.len());
            match chunk.len() {
                4 => {
                    out.push((acc >> 16) as u8);
                    out.push((acc >> 8) as u8);
                    out.push(acc as u8);
                }
                3 => {
                    out.push((acc >> 16) as u8);
                    out.push((acc >> 8) as u8);
                }
                2 => out.push((acc >> 16) as u8),
                _ => {}
            }
        }
        out
    }

    #[test]
    fn three_segments() {
        let jwt = mint("P1");
        let parts: Vec<&str> = jwt.split('.').collect();
        assert_eq!(parts.len(), 3, "JWT must have header.payload.sig");
        assert!(!parts[2].is_empty(), "sig segment must be non-empty");
        for p in &parts {
            assert_ne!(p.len() % 4, 1, "no base64url segment may be %4==1 length");
            assert!(!p.contains('='), "segments must be unpadded");
        }
    }

    #[test]
    fn header_decodes() {
        let jwt = mint("P1");
        let header = jwt.split('.').next().unwrap();
        let bytes = b64url_decode(header);
        let text = String::from_utf8(bytes).unwrap();
        assert_eq!(text, r#"{"alg":"none","typ":"JWT"}"#);
    }

    #[test]
    fn payload_decodes_with_future_exp_and_roles() {
        let jwt = mint("PERSONA-XYZ");
        let payload = jwt.split('.').nth(1).unwrap();
        let bytes = b64url_decode(payload);
        let text = String::from_utf8(bytes).unwrap();

        assert!(text.contains(r#""wotc-rols":["MTGA_DEBUG"]"#), "payload: {text}");
        assert!(text.contains(r#""sub":"PERSONA-XYZ""#), "payload: {text}");
        assert!(text.contains(r#""iss":"mtga-bridge""#), "payload: {text}");

        // Extract exp and confirm it's in the future.
        let exp_str = text.split(r#""exp":"#).nth(1).unwrap();
        let exp_num: u64 = exp_str.split(|c: char| !c.is_ascii_digit()).next().unwrap().parse().unwrap();
        let now = now_unix_secs();
        assert!(exp_num > now, "exp {exp_num} must be after now {now}");
        // And roughly ~10 years out (allow a wide band).
        assert!(exp_num >= now + TEN_YEARS_SECS - 5, "exp should be ~now+10y");
    }

    #[test]
    fn b64url_known_vectors() {
        // RFC 4648 test vectors, unpadded.
        assert_eq!(b64url(b""), "");
        assert_eq!(b64url(b"f"), "Zg");
        assert_eq!(b64url(b"fo"), "Zm8");
        assert_eq!(b64url(b"foo"), "Zm9v");
        assert_eq!(b64url(b"foob"), "Zm9vYg");
        assert_eq!(b64url(b"fooba"), "Zm9vYmE");
        assert_eq!(b64url(b"foobar"), "Zm9vYmFy");
    }
}
