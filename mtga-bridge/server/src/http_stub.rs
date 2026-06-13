//! HTTPS login/doorbell stub (port 443).
//!
//! The MTGA client first talks plain HTTPS to the WotC platform: it logs in
//! (OAuth), reads its profile, and rings the "doorbell" to discover the FrontDoor
//! TCP endpoint. We answer just enough of those routes — with redirected hosts
//! pointed here via the OS hosts file — to hand the client a token and our
//! FrontDoor address.
//!
//! Hand-rolled HTTP/1.1: read the request line + headers up to `\r\n\r\n`, read
//! `Content-Length` bytes of body, route on method+path, write a response, close.
//! Requests here are tiny, so this is simpler than pulling in hyper.

use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;

use crate::jwt;

/// Runtime config for the HTTPS stub.
#[derive(Clone, Copy)]
pub struct HttpConfig {
    /// Port to bind (normally 443).
    pub https_port: u16,
    /// FrontDoor port advertised by the doorbell.
    pub frontdoor_port: u16,
}

/// Bind `0.0.0.0:https_port` and serve the login/doorbell stub forever.
///
/// Binding 443 needs root; that's expected (the user runs with sudo).
pub async fn run(acceptor: TlsAcceptor, cfg: HttpConfig) -> std::io::Result<()> {
    let addr = ("0.0.0.0", cfg.https_port);
    let listener = TcpListener::bind(addr).await?;
    log(&format!("listening on https://0.0.0.0:{} (TLS)", cfg.https_port));

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
            if let Err(e) = serve_conn(acceptor, tcp, cfg).await {
                log(&format!("conn {peer} closed: {e}"));
            }
        });
    }
}

/// Handle a single TLS connection: one or more keep-alive requests until close.
async fn serve_conn(
    acceptor: TlsAcceptor,
    tcp: TcpStream,
    cfg: HttpConfig,
) -> std::io::Result<()> {
    let mut tls = acceptor.accept(tcp).await?;

    // We answer one request then close (Connection: close). The MTGA login flow
    // opens a fresh connection per request, so keep-alive buys us nothing here.
    if let Some(req) = read_request(&mut tls).await? {
        let resp = route(&req, &cfg);
        tls.write_all(&resp).await?;
        tls.flush().await?;
    }
    Ok(())
}

/// A parsed HTTP request (only the bits we route on).
struct Request {
    method: String,
    path: String,
    host: String,
    body: Vec<u8>,
}

/// Read and parse a single HTTP/1.1 request from the TLS stream.
/// Returns `Ok(None)` on a clean EOF before any bytes.
async fn read_request<S>(stream: &mut S) -> std::io::Result<Option<Request>>
where
    S: AsyncReadExt + Unpin,
{
    let mut buf: Vec<u8> = Vec::with_capacity(1024);
    let mut tmp = [0u8; 4096];

    // Read until we have the full header block (\r\n\r\n).
    let header_end = loop {
        if let Some(pos) = find_subslice(&buf, b"\r\n\r\n") {
            break pos + 4;
        }
        let n = stream.read(&mut tmp).await?;
        if n == 0 {
            if buf.is_empty() {
                return Ok(None);
            }
            // EOF mid-headers: malformed, treat as empty request.
            return Ok(Some(Request {
                method: String::new(),
                path: String::new(),
                host: String::new(),
                body: Vec::new(),
            }));
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.len() > 1 << 20 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "request too large"));
        }
    };

    let header_text = String::from_utf8_lossy(&buf[..header_end]).into_owned();
    let mut lines = header_text.split("\r\n");
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();

    let mut host = String::new();
    let mut content_length = 0usize;
    for line in lines {
        if line.is_empty() {
            continue;
        }
        if let Some((k, v)) = line.split_once(':') {
            let key = k.trim().to_ascii_lowercase();
            let val = v.trim();
            match key.as_str() {
                "host" => host = val.to_string(),
                "content-length" => content_length = val.parse().unwrap_or(0),
                _ => {}
            }
        }
    }

    // Body bytes already buffered past the header block.
    let mut body: Vec<u8> = buf[header_end..].to_vec();
    while body.len() < content_length {
        let n = stream.read(&mut tmp).await?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&tmp[..n]);
    }
    body.truncate(content_length.min(body.len()));

    Ok(Some(Request { method, path, host, body }))
}

/// Route a request to a JSON response (full HTTP/1.1 response bytes).
fn route(req: &Request, cfg: &HttpConfig) -> Vec<u8> {
    // Normalize path (strip query string for matching).
    let path_only = req.path.split('?').next().unwrap_or(&req.path);
    let m = req.method.as_str();

    let json: String = match (m, path_only) {
        ("POST", "/auth/oauth/token") => {
            log(&format!("LOGIN  POST {} host={} (grant: {})", req.path, req.host, form_grant(&req.body)));
            oauth_token_json()
        }
        ("GET", "/profile") => {
            log(&format!("LOGIN  GET {} host={}", req.path, req.host));
            r#"{"Email":"stub@example.com","ExternalID":"X1","CountryCode":"US"}"#.to_string()
        }
        ("POST", "/api/v2/ring") | ("POST", "/api/ring") => {
            log(&format!("DOORBELL {} host={} -> FrontDoor tcp://127.0.0.1:{}", req.path, req.host, cfg.frontdoor_port));
            ring_json(cfg.frontdoor_port)
        }
        _ => {
            // Unknown surface — this is exactly what we want to discover by
            // watching the client. Log it loudly and return an empty object.
            log(&format!(
                "UNKNOWN >>> {} {} host={} (body {} bytes) <<< returning {{}}",
                m, req.path, req.host, req.body.len()
            ));
            "{}".to_string()
        }
    };

    http_200_json(&json)
}

/// Extract the `grant_type` from a urlencoded form body (for logging).
fn form_grant(body: &[u8]) -> String {
    let s = String::from_utf8_lossy(body);
    for pair in s.split('&') {
        if let Some(v) = pair.strip_prefix("grant_type=") {
            return v.to_string();
        }
    }
    "?".to_string()
}

/// `POST /auth/oauth/token` body.
fn oauth_token_json() -> String {
    let token = jwt::mint("P1");
    format!(
        r#"{{"access_token":"{token}","refresh_token":"{token}","expires_in":86400,"token_type":"bearer","account_id":"A1","game_id":"G1","persona_id":"P1","display_name":"StubPlayer"}}"#
    )
}

/// Doorbell body: hands the client our FrontDoor TCP endpoint.
fn ring_json(frontdoor_port: u16) -> String {
    format!(
        r#"{{"FdURI":"tcp://127.0.0.1:{port}","fdURI":"tcp://127.0.0.1:{port}","contentHash":"","preloadContentHashes":[],"configurationRoot":""}}"#,
        port = frontdoor_port
    )
}

/// Build a complete `HTTP/1.1 200 OK` response with a JSON body.
fn http_200_json(json: &str) -> Vec<u8> {
    let body = json.as_bytes();
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let mut out = head.into_bytes();
    out.extend_from_slice(body);
    out
}

/// Find the first index of `needle` in `haystack`.
fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn log(msg: &str) {
    eprintln!("[https] {msg}");
}

/// Re-export so `main` can construct an acceptor without depending on tokio-rustls
/// directly in its imports.
pub fn acceptor_from(config: Arc<rustls::ServerConfig>) -> TlsAcceptor {
    TlsAcceptor::from(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> HttpConfig {
        HttpConfig { https_port: 443, frontdoor_port: 27000 }
    }

    fn req(method: &str, path: &str, body: &[u8]) -> Request {
        Request {
            method: method.to_string(),
            path: path.to_string(),
            host: "api.platform.wizards.com".to_string(),
            body: body.to_vec(),
        }
    }

    /// Split a raw HTTP response into (status line, body).
    fn parse_resp(bytes: &[u8]) -> (String, String) {
        let text = String::from_utf8_lossy(bytes);
        let (head, body) = text.split_once("\r\n\r\n").unwrap();
        let status = head.lines().next().unwrap().to_string();
        (status, body.to_string())
    }

    #[test]
    fn oauth_returns_jwt_token() {
        let r = route(&req("POST", "/auth/oauth/token", b"grant_type=password"), &cfg());
        let (status, body) = parse_resp(&r);
        assert_eq!(status, "HTTP/1.1 200 OK");
        assert!(body.contains(r#""token_type":"bearer""#));
        assert!(body.contains(r#""persona_id":"P1""#));
        // access_token must be a 3-segment JWT.
        let tok = body.split(r#""access_token":""#).nth(1).unwrap();
        let tok = tok.split('"').next().unwrap();
        assert_eq!(tok.split('.').count(), 3);
    }

    #[test]
    fn profile_route() {
        let (status, body) = parse_resp(&route(&req("GET", "/profile", b""), &cfg()));
        assert_eq!(status, "HTTP/1.1 200 OK");
        assert!(body.contains(r#""Email":"stub@example.com""#));
    }

    #[test]
    fn doorbell_advertises_frontdoor() {
        let c = HttpConfig { https_port: 443, frontdoor_port: 27123 };
        for path in ["/api/v2/ring", "/api/ring"] {
            let (_s, body) = parse_resp(&route(&req("POST", path, b""), &c));
            assert!(body.contains("tcp://127.0.0.1:27123"), "body: {body}");
            assert!(body.contains(r#""FdURI""#));
            assert!(body.contains(r#""fdURI""#));
        }
    }

    #[test]
    fn unknown_path_returns_empty_object() {
        let (status, body) = parse_resp(&route(&req("GET", "/assets/foo.json", b""), &cfg()));
        assert_eq!(status, "HTTP/1.1 200 OK");
        assert_eq!(body, "{}");
    }

    #[test]
    fn content_type_always_json() {
        let r = route(&req("GET", "/whatever", b""), &cfg());
        let head = String::from_utf8_lossy(&r);
        assert!(head.contains("Content-Type: application/json"));
    }

    #[test]
    fn find_subslice_works() {
        assert_eq!(find_subslice(b"abc\r\n\r\nxyz", b"\r\n\r\n"), Some(3));
        assert_eq!(find_subslice(b"no terminator", b"\r\n\r\n"), None);
    }
}
