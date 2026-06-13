//! mtga-bridge entrypoint.
//!
//! The TLS listener + login/FrontDoor stubs are wired in the next milestone
//! (pending the login + startup-command reverse-engineering). For now this binary
//! reports status and runs a codec self-test so the foundation is verifiable.

use mtga_bridge::envelope::{dump_fields, Cmd};
use mtga_bridge::frame::{decode, Frame};

fn main() {
    println!("mtga-bridge — MTGA-client → local-backend stub");
    println!("status: transport foundation only (frame + FrontDoor envelope codec).");
    println!();
    println!("next steps to reach the home screen:");
    println!("  1. enable the redirect:   sudo python3 ../scripts/redirect.py on");
    println!("  2. trust the dev CA + run the TLS listener (added next milestone)");
    println!("  3. stub login + FrontDoor startup commands (research in progress)");
    println!();

    // Self-test: build a frame carrying a Cmd, then decode both layers.
    let inner = sample_cmd_envelope();
    let wire = Frame::protobuf_msg(inner).encode();
    match decode(&wire) {
        Ok(Some((f, _))) => {
            println!("self-test: decoded {:?} frame, {} body byte(s)", f.msg_type, f.body.len());
            match Cmd::decode(&f.body) {
                Ok(cmd) => println!(
                    "self-test: Cmd type={} transId={:?} compressed={} payload={}",
                    cmd.cmd_type, cmd.trans_id, cmd.compressed, dump_fields(&f.body)
                ),
                Err(e) => println!("self-test: envelope decode FAILED: {e:?}"),
            }
        }
        other => println!("self-test: frame decode unexpected: {other:?}"),
    }
}

/// A hand-built sample Cmd (type=24 ~ EventAiBotMatch-ish, json payload).
fn sample_cmd_envelope() -> Vec<u8> {
    use mtga_bridge::envelope::pb::write_varint;
    let mut out = Vec::new();
    out.push(0x08); // field 1 varint (type)
    write_varint(24, &mut out);
    out.push(0x12); // field 2 len (rawTransId)
    let id = b"00000000-0000-0000-0000-000000000001";
    write_varint(id.len() as u64, &mut out);
    out.extend_from_slice(id);
    out.push(0x22); // field 4 len (jsonPayload)
    let json = br#"{"deckId":"demo"}"#;
    write_varint(json.len() as u64, &mut out);
    out.extend_from_slice(json);
    out
}
