//! FrontDoor TLS server (the meta/lobby channel).
//!
//! After login the client connects (TLS over TCP) to the FrontDoor endpoint we
//! advertised in the doorbell response. It exchanges framed messages: `Ping`
//! heartbeats (echo as `Pong`) and `Msg` frames carrying a protobuf [`Cmd`]
//! envelope. We decode each `Cmd`, dispatch via [`cmds::handle`], and reply with a
//! `Response` envelope wrapped in a protobuf `Msg` frame. Recognized commands
//! drive the client from "connected" to the home screen and into a bot match.

use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;

use crate::cmds;
use crate::envelope::{self, Cmd};
use crate::frame::{self, Frame, MsgType};

/// Runtime config for the FrontDoor server.
#[derive(Clone, Copy)]
pub struct FrontdoorConfig {
    /// Port to bind (the FD_PORT advertised by the doorbell; default 27000).
    pub frontdoor_port: u16,
    /// GRE gameplay endpoint port handed to the client on match creation.
    /// The GRE channel itself is a later milestone; this is a placeholder.
    pub gre_port: u16,
}

/// Bind `0.0.0.0:frontdoor_port` and serve framed FrontDoor traffic over TLS.
pub async fn run(acceptor: TlsAcceptor, cfg: FrontdoorConfig) -> std::io::Result<()> {
    let listener = TcpListener::bind(("0.0.0.0", cfg.frontdoor_port)).await?;
    log(&format!("listening on tcp://0.0.0.0:{} (TLS, framed)", cfg.frontdoor_port));

    loop {
        let (tcp, peer) = match listener.accept().await {
            Ok(x) => x,
            Err(e) => {
                log(&format!("accept error: {e}"));
                continue;
            }
        };
        let acceptor = acceptor.clone();
        tokio::spawn(async move {
            log(&format!("connection from {peer}"));
            if let Err(e) = serve_conn(acceptor, tcp, cfg).await {
                log(&format!("conn {peer} ended: {e}"));
            } else {
                log(&format!("conn {peer} closed"));
            }
        });
    }
}

/// Handle one FrontDoor TLS connection: decode frames, dispatch, reply.
async fn serve_conn(
    acceptor: TlsAcceptor,
    tcp: TcpStream,
    cfg: FrontdoorConfig,
) -> std::io::Result<()> {
    let mut tls = acceptor.accept(tcp).await?;

    let mut buf: Vec<u8> = Vec::with_capacity(4096);
    let mut tmp = [0u8; 8192];

    loop {
        // Drain every complete frame currently buffered.
        loop {
            match frame::decode(&buf) {
                Ok(Some((f, consumed))) => {
                    let out = handle_frame(&f, &cfg);
                    buf.drain(..consumed);
                    for bytes in out {
                        tls.write_all(&bytes).await?;
                    }
                    tls.flush().await?;
                }
                Ok(None) => break, // need more bytes
                Err(e) => {
                    log(&format!("frame decode error: {e:?}; dropping connection"));
                    return Ok(());
                }
            }
        }

        let n = tls.read(&mut tmp).await?;
        if n == 0 {
            return Ok(()); // peer closed
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.len() > 8 << 20 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "frame buffer overflow"));
        }
    }
}

/// Produce the response frame(s) for one received frame.
fn handle_frame(f: &Frame, cfg: &FrontdoorConfig) -> Vec<Vec<u8>> {
    match f.msg_type {
        MsgType::Ping => {
            let tick = first_four(&f.body);
            log(&format!("PING tick={tick:?} -> PONG"));
            vec![Frame::pong(tick).encode()]
        }
        MsgType::Msg => handle_msg(&f.body, cfg),
        other => {
            log(&format!("ignoring unexpected frame type {other:?} ({} body bytes)", f.body.len()));
            Vec::new()
        }
    }
}

/// Decode the `Cmd` in a `Msg` frame body, dispatch, and frame the reply.
fn handle_msg(body: &[u8], cfg: &FrontdoorConfig) -> Vec<Vec<u8>> {
    let cmd = match Cmd::decode(body) {
        Ok(c) => c,
        Err(e) => {
            log(&format!("Cmd decode failed: {e:?}; raw fields: {}", envelope::dump_fields(body)));
            return Vec::new();
        }
    };

    // Resolve the (possibly compressed) payload for logging/handling.
    let cmd = maybe_decompress(cmd);

    let name = cmds::cmd_type::name(cmd.cmd_type);
    let preview = payload_preview(&cmd);
    let outcome = cmds::handle(&cmd);
    log(&format!(
        "CMD {name}({}) trans={:?} recognized={} payload={preview}",
        cmd.cmd_type, cmd.trans_id, outcome.recognized
    ));

    let mut out = Vec::new();
    let resp = envelope::encode_response(&cmd.trans_id, &outcome.response_json);
    out.push(Frame::protobuf_msg(resp).encode());

    if outcome.then_push_match {
        let push_json = cmds::match_created_push("127.0.0.1", cfg.gre_port, "stub-match-1");
        log(&format!("PUSH MatchCreated -> GRE tcp://127.0.0.1:{}", cfg.gre_port));
        // Unsolicited push: empty transId.
        let push = envelope::encode_response("", &push_json);
        out.push(Frame::protobuf_msg(push).encode());
    }

    out
}

/// If the `Cmd` payload is gzip-compressed (`[uncompressedLen:4 LE i32][gzip]`),
/// inflate it back into a plain payload. Most boot Cmds are uncompressed; on any
/// trouble we warn and leave the payload as-is (best-effort).
fn maybe_decompress(mut cmd: Cmd) -> Cmd {
    use crate::envelope::Payload;
    if !cmd.compressed {
        return cmd;
    }
    let raw = match &cmd.payload {
        Payload::Protobuf(b) => b.clone(),
        Payload::Json(s) => s.clone().into_bytes(),
        Payload::None => return cmd,
    };
    if raw.len() < 4 {
        log("compressed payload too short to hold length prefix; leaving as-is");
        return cmd;
    }
    let (_len_prefix, gz) = raw.split_at(4);
    match gunzip(gz) {
        Ok(plain) => {
            // Compressed FrontDoor payloads are JSON; decode best-effort.
            cmd.payload = Payload::Json(String::from_utf8_lossy(&plain).into_owned());
            cmd.compressed = false;
        }
        Err(e) => log(&format!("gzip inflate failed ({e}); leaving payload compressed")),
    }
    cmd
}

/// Inflate a raw gzip stream.
fn gunzip(data: &[u8]) -> std::io::Result<Vec<u8>> {
    use flate2::read::GzDecoder;
    use std::io::Read;
    let mut d = GzDecoder::new(data);
    let mut out = Vec::new();
    d.read_to_end(&mut out)?;
    Ok(out)
}

/// A short, log-friendly preview of a `Cmd`'s payload.
fn payload_preview(cmd: &Cmd) -> String {
    use crate::envelope::Payload;
    match &cmd.payload {
        Payload::Json(s) => {
            let s = s.trim();
            if s.len() > 120 {
                format!("json[{}]({}…)", s.len(), &s[..120])
            } else {
                format!("json({s})")
            }
        }
        Payload::Protobuf(b) => format!("protobuf({})", envelope::dump_fields(b)),
        Payload::None => "<none>".to_string(),
    }
}

/// Take the first 4 bytes (zero-padded) for a Ping/Pong tick.
fn first_four(b: &[u8]) -> [u8; 4] {
    let mut t = [0u8; 4];
    let n = b.len().min(4);
    t[..n].copy_from_slice(&b[..n]);
    t
}

fn log(msg: &str) {
    crate::logging::log("frontdoor", msg);
}

/// Build a `TlsAcceptor` from a shared rustls config (same cert as the HTTPS stub).
pub fn acceptor_from(config: Arc<rustls::ServerConfig>) -> TlsAcceptor {
    TlsAcceptor::from(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::{encode_cmd, pb, Payload};

    fn cfg() -> FrontdoorConfig {
        FrontdoorConfig { frontdoor_port: 27000, gre_port: 27001 }
    }

    /// A protobuf `Msg` frame carrying a JSON Cmd.
    fn msg_frame(cmd_type: i32, trans: &str, json: &str) -> Frame {
        let body = encode_cmd(cmd_type, trans, json);
        Frame::protobuf_msg(body)
    }

    /// Decode a Response envelope frame into (transId, jsonPayload).
    fn parse_response_frame(bytes: &[u8]) -> (String, String) {
        let (f, _n) = frame::decode(bytes).unwrap().unwrap();
        assert_eq!(f.msg_type, MsgType::Msg);
        assert_eq!(f.format, crate::frame::Format::Protobuf);
        let fields = pb::fields(&f.body).unwrap();
        let mut trans = String::new();
        let mut json = String::new();
        for (n, v) in fields {
            if let pb::Value::Len(b) = v {
                match n {
                    1 => trans = String::from_utf8_lossy(b).into_owned(),
                    3 => json = String::from_utf8_lossy(b).into_owned(),
                    _ => {}
                }
            }
        }
        (trans, json)
    }

    #[test]
    fn ping_echoes_pong() {
        let ping = Frame { version: 4, msg_type: MsgType::Ping, format: crate::frame::Format::Unspecified, body: vec![9, 8, 7, 6] };
        let out = handle_frame(&ping, &cfg());
        assert_eq!(out.len(), 1);
        let (f, _) = frame::decode(&out[0]).unwrap().unwrap();
        assert_eq!(f.msg_type, MsgType::Pong);
        assert_eq!(f.body, vec![9, 8, 7, 6]);
    }

    #[test]
    fn authenticate_cmd_gets_response() {
        let f = msg_frame(cmds::cmd_type::AUTHENTICATE, "tx-9", "{}");
        let out = handle_frame(&f, &cfg());
        assert_eq!(out.len(), 1, "authenticate yields a single response frame");
        let (trans, json) = parse_response_frame(&out[0]);
        assert_eq!(trans, "tx-9");
        assert!(json.contains(r#""Attached":true"#), "json: {json}");
    }

    #[test]
    fn bot_match_emits_response_then_push() {
        let f = msg_frame(cmds::cmd_type::EVENT_AI_BOT_MATCH, "tx-bot", r#"{"deckId":"x"}"#);
        let out = handle_frame(&f, &cfg());
        assert_eq!(out.len(), 2, "bot match yields response + MatchCreated push");

        let (trans, json) = parse_response_frame(&out[0]);
        assert_eq!(trans, "tx-bot");
        assert_eq!(json, "\"stub-match-1\"");

        let (push_trans, push_json) = parse_response_frame(&out[1]);
        assert_eq!(push_trans, "", "push has empty transId");
        assert!(push_json.contains(r#""Type":"MatchCreated""#), "push: {push_json}");
        assert!(push_json.contains(r#""MatchEndpointPort":27001"#), "push: {push_json}");
    }

    #[test]
    fn unknown_cmd_still_replies() {
        let f = msg_frame(424242, "tx-u", "{}");
        let out = handle_frame(&f, &cfg());
        assert_eq!(out.len(), 1);
        let (trans, json) = parse_response_frame(&out[0]);
        assert_eq!(trans, "tx-u");
        assert_eq!(json, "{}");
    }

    #[test]
    fn gzip_payload_inflated() {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;

        let plain = br#"{"hello":"world"}"#;
        let mut enc = GzEncoder::new(Vec::new(), Compression::default());
        enc.write_all(plain).unwrap();
        let gz = enc.finish().unwrap();

        // [uncompressedLen:4 LE][gzip]
        let mut payload = (plain.len() as i32).to_le_bytes().to_vec();
        payload.extend_from_slice(&gz);

        let cmd = Cmd {
            cmd_type: 0,
            trans_id: "t".into(),
            payload: Payload::Protobuf(payload),
            compressed: true,
        };
        let out = maybe_decompress(cmd);
        assert!(!out.compressed);
        assert_eq!(out.payload, Payload::Json(r#"{"hello":"world"}"#.into()));
    }

    #[test]
    fn first_four_pads_short_body() {
        assert_eq!(first_four(&[1, 2]), [1, 2, 0, 0]);
        assert_eq!(first_four(&[1, 2, 3, 4, 5]), [1, 2, 3, 4]);
    }
}
