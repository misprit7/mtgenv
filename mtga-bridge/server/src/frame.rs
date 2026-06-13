//! MTGA TcpConnection frame codec.
//!
//! Every message on both the FrontDoor (meta) and Match/GRE channels is wrapped
//! in the same small frame (recovered from `Wizards.Arena.TcpConnection`):
//!
//! ```text
//! v4 (current):  [ver=4][type|format:1][bodyLen:4 LE i32][body]
//!                  type   = byte & 0x0F   (Msg=1, Ping=2, Pong=3)
//!                  format = byte >> 4     (Json=1, Protobuf=2)
//! v3:            [ver=3][type:1 (low nibble)][bodyLen:4 LE][body]   (no format nibble)
//! v1:            [ver=1][bodyLen:4 LE][body]                        (type implicitly Msg)
//! ```
//!
//! `bodyLen` is a little-endian i32 of the body length (header excluded). The
//! reader is length-delimited and reassembles across TCP segments. We write v4
//! and accept v1/v3/v4 on read.

/// Logical message kind (frame header low nibble).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MsgType {
    Msg,
    Ping,
    Pong,
    Other(u8),
}

impl MsgType {
    pub fn from_nibble(n: u8) -> Self {
        match n & 0x0F {
            1 => MsgType::Msg,
            2 => MsgType::Ping,
            3 => MsgType::Pong,
            other => MsgType::Other(other),
        }
    }
    pub fn to_nibble(self) -> u8 {
        match self {
            MsgType::Msg => 1,
            MsgType::Ping => 2,
            MsgType::Pong => 3,
            MsgType::Other(o) => o & 0x0F,
        }
    }
}

/// Body serialization (frame header high nibble). Only meaningful for v4 frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// No format nibble was present (v1/v3 frames).
    Unspecified,
    Json,
    Protobuf,
    Other(u8),
}

impl Format {
    pub fn from_nibble(n: u8) -> Self {
        match n & 0x0F {
            1 => Format::Json,
            2 => Format::Protobuf,
            other => Format::Other(other),
        }
    }
    pub fn to_nibble(self) -> u8 {
        match self {
            Format::Unspecified => 0,
            Format::Json => 1,
            Format::Protobuf => 2,
            Format::Other(o) => o & 0x0F,
        }
    }
}

/// A decoded frame. `body` is the raw payload (a serialized envelope for `Msg`
/// frames; a 4-byte tick for `Ping`/`Pong`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub version: u8,
    pub msg_type: MsgType,
    pub format: Format,
    pub body: Vec<u8>,
}

impl Frame {
    /// A v4 protobuf `Msg` frame (the common gameplay/FrontDoor case).
    pub fn protobuf_msg(body: Vec<u8>) -> Self {
        Frame { version: 4, msg_type: MsgType::Msg, format: Format::Protobuf, body }
    }

    /// A v4 Pong echoing a 4-byte tick from a received Ping.
    pub fn pong(tick: [u8; 4]) -> Self {
        Frame { version: 4, msg_type: MsgType::Pong, format: Format::Unspecified, body: tick.to_vec() }
    }

    /// Serialize this frame (always written as v4).
    pub fn encode(&self) -> Vec<u8> {
        let len = self.body.len() as i32;
        let header_byte = (self.format.to_nibble() << 4) | self.msg_type.to_nibble();
        let mut out = Vec::with_capacity(6 + self.body.len());
        out.push(4); // _sendVersion = 4
        out.push(header_byte);
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&self.body);
        out
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum FrameError {
    UnknownVersion(u8),
    BadLength(i32),
}

/// Try to decode a single frame from the front of `buf`.
///
/// Returns:
/// - `Ok(Some((frame, consumed)))` — a complete frame; `consumed` bytes may be drained.
/// - `Ok(None)` — not enough bytes yet; wait for more and retry.
/// - `Err(_)` — a malformed/unsupported frame (caller should drop the connection).
pub fn decode(buf: &[u8]) -> Result<Option<(Frame, usize)>, FrameError> {
    if buf.is_empty() {
        return Ok(None);
    }
    let version = buf[0];
    // header layout per version
    let (msg_type, format, len_off) = match version {
        4 => {
            if buf.len() < 2 {
                return Ok(None);
            }
            let h = buf[1];
            (MsgType::from_nibble(h), Format::from_nibble(h >> 4), 2usize)
        }
        3 => {
            if buf.len() < 2 {
                return Ok(None);
            }
            (MsgType::from_nibble(buf[1]), Format::Unspecified, 2usize)
        }
        1 => (MsgType::Msg, Format::Unspecified, 1usize),
        other => return Err(FrameError::UnknownVersion(other)),
    };

    let len_end = len_off + 4;
    if buf.len() < len_end {
        return Ok(None);
    }
    let len = i32::from_le_bytes([buf[len_off], buf[len_off + 1], buf[len_off + 2], buf[len_off + 3]]);
    if len < 0 {
        return Err(FrameError::BadLength(len));
    }
    let len = len as usize;
    let total = len_end + len;
    if buf.len() < total {
        return Ok(None);
    }
    let body = buf[len_end..total].to_vec();
    Ok(Some((Frame { version, msg_type, format, body }, total)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_v4_protobuf_msg() {
        let f = Frame::protobuf_msg(vec![0xde, 0xad, 0xbe, 0xef]);
        let bytes = f.encode();
        // [ver=4][format<<4|type = 0x21][len=4 LE][body]
        assert_eq!(bytes[0], 4);
        assert_eq!(bytes[1], 0x21);
        assert_eq!(&bytes[2..6], &[4, 0, 0, 0]);
        let (decoded, consumed) = decode(&bytes).unwrap().unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(decoded, f);
    }

    #[test]
    fn partial_then_complete() {
        let f = Frame::protobuf_msg(vec![1, 2, 3, 4, 5]);
        let bytes = f.encode();
        // header-only, length-only, and body-short all return None
        assert_eq!(decode(&bytes[..1]).unwrap(), None);
        assert_eq!(decode(&bytes[..2]).unwrap(), None);
        assert_eq!(decode(&bytes[..6]).unwrap(), None);
        assert_eq!(decode(&bytes[..8]).unwrap(), None);
        assert!(decode(&bytes).unwrap().is_some());
    }

    #[test]
    fn two_frames_back_to_back() {
        let a = Frame::protobuf_msg(vec![0xaa]);
        let b = Frame::pong([1, 0, 0, 0]);
        let mut stream = a.encode();
        stream.extend(b.encode());

        let (fa, na) = decode(&stream).unwrap().unwrap();
        assert_eq!(fa, a);
        let (fb, _nb) = decode(&stream[na..]).unwrap().unwrap();
        assert_eq!(fb.msg_type, MsgType::Pong);
        assert_eq!(fb.body, vec![1, 0, 0, 0]);
    }

    #[test]
    fn ping_nibbles() {
        // type=Ping(2), format=Json(1) -> 0x12
        let f = Frame { version: 4, msg_type: MsgType::Ping, format: Format::Json, body: vec![] };
        let bytes = f.encode();
        assert_eq!(bytes[1], 0x12);
        let (d, _) = decode(&bytes).unwrap().unwrap();
        assert_eq!(d.msg_type, MsgType::Ping);
        assert_eq!(d.format, Format::Json);
    }

    #[test]
    fn v1_and_v3_accepted() {
        // v1: [1][len][body]
        let mut v1 = vec![1u8];
        v1.extend_from_slice(&3i32.to_le_bytes());
        v1.extend_from_slice(&[7, 8, 9]);
        let (f1, c1) = decode(&v1).unwrap().unwrap();
        assert_eq!(f1.version, 1);
        assert_eq!(f1.msg_type, MsgType::Msg);
        assert_eq!(f1.body, vec![7, 8, 9]);
        assert_eq!(c1, v1.len());

        // v3: [3][type=1][len][body]
        let mut v3 = vec![3u8, 1u8];
        v3.extend_from_slice(&2i32.to_le_bytes());
        v3.extend_from_slice(&[42, 43]);
        let (f3, _c3) = decode(&v3).unwrap().unwrap();
        assert_eq!(f3.version, 3);
        assert_eq!(f3.format, Format::Unspecified);
        assert_eq!(f3.body, vec![42, 43]);
    }

    #[test]
    fn unknown_version_errors() {
        assert_eq!(decode(&[9, 0, 0, 0, 0, 0]), Err(FrameError::UnknownVersion(9)));
    }
}
