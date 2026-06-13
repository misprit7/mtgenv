//! FrontDoor envelope decode (`Wizards.Arena.Protocol.Cmd` / `Response`).
//!
//! The body of a FrontDoor `Msg` frame is a protobuf envelope. Recovered from
//! `decompiled/ModelsProtobuf/Wizards.Arena.Protocol/Cmd.cs`:
//!
//! ```proto
//! message Cmd {
//!   CmdType type        = 1;   // varint
//!   string  rawTransId  = 2;   // request<->response correlation id
//!   oneof payload { bytes protobufPayload = 3; string jsonPayload = 4; }
//!   bool    compressed  = 5;   // payload is gzip-compressed when true
//! }
//! ```
//!
//! `Response` (server->client reply / push) mirrors this with an added error
//! field; its exact tags are being confirmed by the FrontDoor research pass, so
//! for now we expose the confirmed `Cmd` decode plus a generic field dumper used
//! for logging any envelope. No external protobuf dep — we hand-decode the few
//! fields we need.

/// Minimal protobuf wire reader (proto3, no group support).
pub mod pb {
    #[derive(Debug, Clone, PartialEq)]
    pub enum Value<'a> {
        Varint(u64),
        Len(&'a [u8]),
        Fixed32([u8; 4]),
        Fixed64([u8; 8]),
    }

    #[derive(Debug, PartialEq, Eq)]
    pub enum Error {
        Truncated,
        BadWireType(u8),
        VarintOverflow,
    }

    /// Read a base-128 varint; returns (value, bytes_consumed).
    pub fn read_varint(buf: &[u8]) -> Result<(u64, usize), Error> {
        let mut result: u64 = 0;
        let mut shift = 0u32;
        for (i, &b) in buf.iter().enumerate() {
            if shift >= 64 {
                return Err(Error::VarintOverflow);
            }
            result |= ((b & 0x7F) as u64) << shift;
            if b & 0x80 == 0 {
                return Ok((result, i + 1));
            }
            shift += 7;
        }
        Err(Error::Truncated)
    }

    /// Iterate `(field_number, value)` pairs over a serialized message.
    pub fn fields(mut buf: &[u8]) -> Result<Vec<(u32, Value<'_>)>, Error> {
        let mut out = Vec::new();
        while !buf.is_empty() {
            let (key, n) = read_varint(buf)?;
            buf = &buf[n..];
            let field = (key >> 3) as u32;
            let wire = (key & 0x7) as u8;
            let value = match wire {
                0 => {
                    let (v, n) = read_varint(buf)?;
                    buf = &buf[n..];
                    Value::Varint(v)
                }
                1 => {
                    if buf.len() < 8 {
                        return Err(Error::Truncated);
                    }
                    let mut a = [0u8; 8];
                    a.copy_from_slice(&buf[..8]);
                    buf = &buf[8..];
                    Value::Fixed64(a)
                }
                2 => {
                    let (len, n) = read_varint(buf)?;
                    buf = &buf[n..];
                    let len = len as usize;
                    if buf.len() < len {
                        return Err(Error::Truncated);
                    }
                    let slice = &buf[..len];
                    buf = &buf[len..];
                    Value::Len(slice)
                }
                5 => {
                    if buf.len() < 4 {
                        return Err(Error::Truncated);
                    }
                    let mut a = [0u8; 4];
                    a.copy_from_slice(&buf[..4]);
                    buf = &buf[4..];
                    Value::Fixed32(a)
                }
                other => return Err(Error::BadWireType(other)),
            };
            out.push((field, value));
        }
        Ok(out)
    }

    /// Encode a varint.
    pub fn write_varint(mut v: u64, out: &mut Vec<u8>) {
        loop {
            let mut b = (v & 0x7F) as u8;
            v >>= 7;
            if v != 0 {
                b |= 0x80;
            }
            out.push(b);
            if v == 0 {
                break;
            }
        }
    }

    /// Write a field key (field number + wire type).
    pub fn write_key(field: u32, wire: u8, out: &mut Vec<u8>) {
        write_varint(((field as u64) << 3) | (wire as u64 & 0x7), out);
    }

    /// Write a length-delimited field (wire type 2).
    pub fn write_len_field(field: u32, bytes: &[u8], out: &mut Vec<u8>) {
        write_key(field, 2, out);
        write_varint(bytes.len() as u64, out);
        out.extend_from_slice(bytes);
    }

    /// Write a varint field (wire type 0).
    pub fn write_varint_field(field: u32, v: u64, out: &mut Vec<u8>) {
        write_key(field, 0, out);
        write_varint(v, out);
    }
}

/// Encode a success `Response` envelope (uncompressed JSON payload).
///
/// `Response { rawTransId=1; bytes jsonPayload=3; bool compressed=5 }` — note the
/// field numbers differ from `Cmd` (transId is 1 not 2, json is 3 not 4). We echo
/// the request's transId and leave `error` unset (success).
pub fn encode_response(trans_id: &str, json: &str) -> Vec<u8> {
    let mut out = Vec::new();
    pb::write_len_field(1, trans_id.as_bytes(), &mut out); // rawTransId
    pb::write_len_field(3, json.as_bytes(), &mut out); // jsonPayload (bytes, UTF-8 JSON)
    out // compressed defaults to false (omitted)
}

/// Encode a `Cmd` envelope with a JSON payload (used for tests / a future client).
pub fn encode_cmd(cmd_type: i32, trans_id: &str, json: &str) -> Vec<u8> {
    let mut out = Vec::new();
    pb::write_varint_field(1, cmd_type as u64, &mut out); // type
    pb::write_len_field(2, trans_id.as_bytes(), &mut out); // rawTransId
    pb::write_len_field(4, json.as_bytes(), &mut out); // jsonPayload
    out
}

/// Which arm of the `Cmd`/`Response` payload oneof was set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Payload {
    None,
    /// field 3: a nested protobuf message (e.g. a typed model).
    Protobuf(Vec<u8>),
    /// field 4: a JSON string (the common FrontDoor case).
    Json(String),
}

/// A decoded FrontDoor `Cmd` (client -> server request).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cmd {
    /// CmdType enum value (kept numeric; the catalog lives separately).
    pub cmd_type: i32,
    pub trans_id: String,
    pub payload: Payload,
    pub compressed: bool,
}

impl Cmd {
    pub fn decode(body: &[u8]) -> Result<Cmd, pb::Error> {
        let mut cmd = Cmd { cmd_type: 0, trans_id: String::new(), payload: Payload::None, compressed: false };
        for (field, value) in pb::fields(body)? {
            match (field, value) {
                (1, pb::Value::Varint(v)) => cmd.cmd_type = v as i32,
                (2, pb::Value::Len(b)) => cmd.trans_id = String::from_utf8_lossy(b).into_owned(),
                (3, pb::Value::Len(b)) => cmd.payload = Payload::Protobuf(b.to_vec()),
                (4, pb::Value::Len(b)) => cmd.payload = Payload::Json(String::from_utf8_lossy(b).into_owned()),
                (5, pb::Value::Varint(v)) => cmd.compressed = v != 0,
                _ => {} // ignore unknown fields (forward-compatible)
            }
        }
        Ok(cmd)
    }
}

/// A one-line, log-friendly dump of an envelope's fields (for any message,
/// including ones we don't yet model). Shows field number, wire type, and a
/// short value preview.
pub fn dump_fields(body: &[u8]) -> String {
    match pb::fields(body) {
        Ok(fields) => {
            let parts: Vec<String> = fields
                .iter()
                .map(|(f, v)| match v {
                    pb::Value::Varint(n) => format!("#{f}=varint({n})"),
                    pb::Value::Fixed32(_) => format!("#{f}=fixed32"),
                    pb::Value::Fixed64(_) => format!("#{f}=fixed64"),
                    pb::Value::Len(b) => {
                        let printable = b.iter().take(48).all(|&c| c == 9 || c == 10 || (32..127).contains(&c));
                        if printable {
                            let s = String::from_utf8_lossy(&b[..b.len().min(48)]);
                            let ell = if b.len() > 48 { "…" } else { "" };
                            format!("#{f}=str[{}]({s}{ell})", b.len())
                        } else {
                            format!("#{f}=bytes[{}]", b.len())
                        }
                    }
                })
                .collect();
            parts.join(" ")
        }
        Err(e) => format!("<undecodable: {e:?}>"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hand-build a Cmd: type=42, rawTransId="ab", jsonPayload="{}", compressed=true.
    fn sample_cmd_bytes() -> Vec<u8> {
        let mut out = Vec::new();
        // field 1, varint: tag 0x08, value 42
        out.push(0x08);
        pb::write_varint(42, &mut out);
        // field 2, len: tag 0x12, "ab"
        out.push(0x12);
        out.push(2);
        out.extend_from_slice(b"ab");
        // field 4, len: tag 0x22, "{}"
        out.push(0x22);
        out.push(2);
        out.extend_from_slice(b"{}");
        // field 5, varint: tag 0x28, value 1
        out.push(0x28);
        pb::write_varint(1, &mut out);
        out
    }

    #[test]
    fn decode_cmd() {
        let cmd = Cmd::decode(&sample_cmd_bytes()).unwrap();
        assert_eq!(cmd.cmd_type, 42);
        assert_eq!(cmd.trans_id, "ab");
        assert_eq!(cmd.payload, Payload::Json("{}".to_string()));
        assert!(cmd.compressed);
    }

    #[test]
    fn varint_multibyte() {
        let mut buf = Vec::new();
        pb::write_varint(300, &mut buf);
        assert_eq!(buf, vec![0xAC, 0x02]);
        assert_eq!(pb::read_varint(&buf).unwrap(), (300, 2));
    }

    #[test]
    fn dump_is_readable() {
        let dump = dump_fields(&sample_cmd_bytes());
        assert!(dump.contains("#1=varint(42)"));
        assert!(dump.contains("#2=str[2](ab)"));
        assert!(dump.contains("#4=str[2]({})"));
        assert!(dump.contains("#5=varint(1)"));
    }

    #[test]
    fn response_roundtrip() {
        let bytes = encode_response("tx-7", r#"{"Attached":true}"#);
        let fields = pb::fields(&bytes).unwrap();
        // field 1 = transId, field 3 = jsonPayload
        let trans = fields.iter().find(|(f, _)| *f == 1).unwrap();
        let json = fields.iter().find(|(f, _)| *f == 3).unwrap();
        assert!(matches!(trans.1, pb::Value::Len(b) if b == b"tx-7"));
        assert!(matches!(json.1, pb::Value::Len(b) if b == br#"{"Attached":true}"#));
    }

    #[test]
    fn cmd_encode_decode_roundtrip() {
        let bytes = encode_cmd(612, "abc", r#"{"deckId":"x"}"#);
        let cmd = Cmd::decode(&bytes).unwrap();
        assert_eq!(cmd.cmd_type, 612);
        assert_eq!(cmd.trans_id, "abc");
        assert_eq!(cmd.payload, Payload::Json(r#"{"deckId":"x"}"#.to_string()));
    }

    #[test]
    fn unknown_fields_ignored() {
        // append an unknown field 9 (varint) — should be skipped, not error
        let mut bytes = sample_cmd_bytes();
        bytes.push((9 << 3) | 0);
        pb::write_varint(7, &mut bytes);
        let cmd = Cmd::decode(&bytes).unwrap();
        assert_eq!(cmd.cmd_type, 42);
    }
}
