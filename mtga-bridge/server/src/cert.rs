//! TLS materials for the bridge.
//!
//! The real MTGA client connects to a handful of `*.wizards.com` / `*.mtgarena.*`
//! hosts over TLS. To impersonate them locally we generate a self-signed **CA**
//! and a **leaf** certificate (signed by the CA) whose Subject Alternative Names
//! cover those hosts. The user installs the CA into their OS trust store
//! (printed at startup); the leaf is what we actually serve.
//!
//! Both are cached to `.certs/` so we keep the same CA across runs (re-installing
//! the trust anchor every launch would be annoying). If the cache is missing or
//! unreadable we regenerate.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, KeyPair,
    KeyUsagePurpose, SanType,
};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::ServerConfig;

/// Hostnames the leaf cert must be valid for (the client pins by host).
pub const LEAF_DNS_SANS: &[&str] = &[
    "api.platform.wizards.com",
    "doorbellprod.w2.mtgarena.com",
    "assets.mtgarena.wizards.com",
    "bike.w2.mtgarena.com",
    "localhost",
];

/// IP SANs the leaf cert must be valid for.
pub const LEAF_IP_SANS: &[&str] = &["127.0.0.1"];

/// Directory (relative to the crate root) where the CA + leaf are cached.
pub const CERTS_DIR: &str = ".certs";
const CA_PEM: &str = "mtga-bridge-ca.pem";
const CA_KEY_PEM: &str = "mtga-bridge-ca.key.pem";
const LEAF_PEM: &str = "mtga-bridge-leaf.pem";
const LEAF_KEY_PEM: &str = "mtga-bridge-leaf.key.pem";

/// A generated CA + leaf pair, in PEM form.
pub struct CertBundle {
    /// CA certificate PEM (the trust anchor the user installs).
    pub ca_cert_pem: String,
    /// Leaf certificate PEM (served to the client).
    pub leaf_cert_pem: String,
    /// Leaf private key PEM (PKCS#8).
    pub leaf_key_pem: String,
    /// Absolute path to the written CA PEM (for the install instructions).
    pub ca_cert_path: PathBuf,
}

/// Load the cached CA+leaf if present, otherwise generate and cache them.
///
/// `base_dir` is the crate root; certs land in `base_dir/.certs/`.
pub fn load_or_generate(base_dir: &Path) -> Result<CertBundle, CertError> {
    let dir = base_dir.join(CERTS_DIR);
    let ca_cert_path = dir.join(CA_PEM);
    let ca_key_path = dir.join(CA_KEY_PEM);
    let leaf_cert_path = dir.join(LEAF_PEM);
    let leaf_key_path = dir.join(LEAF_KEY_PEM);

    if let (Ok(ca_cert_pem), Ok(leaf_cert_pem), Ok(leaf_key_pem)) = (
        fs::read_to_string(&ca_cert_path),
        fs::read_to_string(&leaf_cert_path),
        fs::read_to_string(&leaf_key_path),
    ) {
        // We only need to *serve*, so the CA key on disk is not required to load.
        let _ = &ca_key_path;
        if !ca_cert_pem.is_empty() && !leaf_cert_pem.is_empty() && !leaf_key_pem.is_empty() {
            return Ok(CertBundle { ca_cert_pem, leaf_cert_pem, leaf_key_pem, ca_cert_path });
        }
    }

    let bundle = generate()?;
    fs::create_dir_all(&dir).map_err(CertError::Io)?;
    fs::write(&ca_cert_path, &bundle.ca_cert_pem).map_err(CertError::Io)?;
    fs::write(&leaf_cert_path, &bundle.leaf_cert_pem).map_err(CertError::Io)?;
    fs::write(&leaf_key_path, &bundle.leaf_key_pem).map_err(CertError::Io)?;
    // The CA key is written too so the same CA can re-issue leaves later if needed.
    fs::write(&ca_key_path, &bundle.ca_key_pem_for_cache).map_err(CertError::Io)?;

    Ok(CertBundle {
        ca_cert_pem: bundle.ca_cert_pem,
        leaf_cert_pem: bundle.leaf_cert_pem,
        leaf_key_pem: bundle.leaf_key_pem,
        ca_cert_path,
    })
}

/// Internal generation result, including the CA key (cached but not part of the
/// public `CertBundle`).
struct Generated {
    ca_cert_pem: String,
    ca_key_pem_for_cache: String,
    leaf_cert_pem: String,
    leaf_key_pem: String,
}

/// Generate a fresh self-signed CA and a leaf signed by it.
fn generate() -> Result<Generated, CertError> {
    // --- CA ---
    let ca_key = KeyPair::generate().map_err(CertError::Rcgen)?;
    let mut ca_params = CertificateParams::new(Vec::<String>::new()).map_err(CertError::Rcgen)?;
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params.distinguished_name.push(DnType::CommonName, "mtga-bridge dev CA");
    ca_params
        .distinguished_name
        .push(DnType::OrganizationName, "mtga-bridge (interop research)");
    ca_params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
        KeyUsagePurpose::DigitalSignature,
    ];
    let ca_cert = ca_params.self_signed(&ca_key).map_err(CertError::Rcgen)?;

    // --- leaf ---
    let leaf_key = KeyPair::generate().map_err(CertError::Rcgen)?;
    let mut leaf_params = CertificateParams::new(Vec::<String>::new()).map_err(CertError::Rcgen)?;
    leaf_params.is_ca = IsCa::ExplicitNoCa;
    leaf_params.distinguished_name.push(DnType::CommonName, "api.platform.wizards.com");
    leaf_params.key_usages =
        vec![KeyUsagePurpose::DigitalSignature, KeyUsagePurpose::KeyEncipherment];
    leaf_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    leaf_params.subject_alt_names = build_sans()?;

    let leaf_cert = leaf_params.signed_by(&leaf_key, &ca_cert, &ca_key).map_err(CertError::Rcgen)?;

    Ok(Generated {
        ca_cert_pem: ca_cert.pem(),
        ca_key_pem_for_cache: ca_key.serialize_pem(),
        leaf_cert_pem: leaf_cert.pem(),
        leaf_key_pem: leaf_key.serialize_pem(),
    })
}

/// Build the leaf SAN list (DNS names + IPs).
fn build_sans() -> Result<Vec<SanType>, CertError> {
    let mut sans = Vec::new();
    for dns in LEAF_DNS_SANS {
        sans.push(SanType::DnsName((*dns).try_into().map_err(CertError::Rcgen)?));
    }
    for ip in LEAF_IP_SANS {
        let addr: std::net::IpAddr = ip.parse().map_err(|_| CertError::BadIp(ip.to_string()))?;
        sans.push(SanType::IpAddress(addr));
    }
    Ok(sans)
}

/// Build a rustls [`ServerConfig`] serving the leaf cert. TLS 1.2 **and** 1.3 are
/// enabled — the MTGA client negotiates TLS 1.2, so 1.2 must be present.
pub fn server_config(bundle: &CertBundle) -> Result<Arc<ServerConfig>, CertError> {
    let certs = parse_certs(&bundle.leaf_cert_pem)?;
    let key = parse_pkcs8_key(&bundle.leaf_key_pem)?;

    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS12, &rustls::version::TLS13])
        .map_err(CertError::Rustls)?
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(CertError::Rustls)?;

    Ok(Arc::new(config))
}

/// Parse all CERTIFICATE blocks from a PEM string into DER certs.
fn parse_certs(pem: &str) -> Result<Vec<CertificateDer<'static>>, CertError> {
    let certs = rustls_pemfile_certs(pem.as_bytes()).collect::<Result<Vec<_>, _>>()?;
    if certs.is_empty() {
        return Err(CertError::NoCert);
    }
    Ok(certs)
}

/// Parse a single PKCS#8 private key from a PEM string.
fn parse_pkcs8_key(pem: &str) -> Result<PrivateKeyDer<'static>, CertError> {
    // rcgen emits a PKCS#8 "PRIVATE KEY" block; pull the first one.
    let der = first_pem_block(pem.as_bytes(), "PRIVATE KEY").ok_or(CertError::NoKey)?;
    Ok(PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(der)))
}

// --- tiny PEM reader (avoids adding the rustls-pemfile dep) ---

/// Decode every "CERTIFICATE" PEM block into a [`CertificateDer`].
fn rustls_pemfile_certs(
    pem: &[u8],
) -> impl Iterator<Item = Result<CertificateDer<'static>, CertError>> + '_ {
    PemBlocks::new(pem, "CERTIFICATE").map(|der| der.map(CertificateDer::from))
}

/// Extract the first PEM block of the given label as raw DER bytes.
fn first_pem_block(pem: &[u8], label: &str) -> Option<Vec<u8>> {
    PemBlocks::new(pem, label).next().and_then(Result::ok)
}

/// Minimal PEM block scanner that base64-decodes blocks matching `label`.
struct PemBlocks<'a> {
    text: &'a str,
    begin: String,
    end: String,
    pos: usize,
}

impl<'a> PemBlocks<'a> {
    fn new(pem: &'a [u8], label: &str) -> Self {
        PemBlocks {
            text: std::str::from_utf8(pem).unwrap_or(""),
            begin: format!("-----BEGIN {label}-----"),
            end: format!("-----END {label}-----"),
            pos: 0,
        }
    }
}

impl Iterator for PemBlocks<'_> {
    type Item = Result<Vec<u8>, CertError>;
    fn next(&mut self) -> Option<Self::Item> {
        let rest = &self.text[self.pos..];
        let start = rest.find(&self.begin)?;
        let body_start = start + self.begin.len();
        let end_rel = rest[body_start..].find(&self.end)?;
        let body = &rest[body_start..body_start + end_rel];
        self.pos += body_start + end_rel + self.end.len();
        let b64: String = body.chars().filter(|c| !c.is_whitespace()).collect();
        Some(base64_decode(&b64).map_err(CertError::Base64))
    }
}

/// Standard base64 decode (RFC 4648, with padding), enough for PEM bodies.
fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    fn val(c: u8) -> Result<u8, String> {
        match c {
            b'A'..=b'Z' => Ok(c - b'A'),
            b'a'..=b'z' => Ok(c - b'a' + 26),
            b'0'..=b'9' => Ok(c - b'0' + 52),
            b'+' => Ok(62),
            b'/' => Ok(63),
            _ => Err(format!("bad base64 char {c:?}")),
        }
    }
    let bytes: Vec<u8> = s.bytes().filter(|&b| b != b'=').collect();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        let mut acc: u32 = 0;
        let n = chunk.len();
        for &c in chunk {
            acc = (acc << 6) | val(c)? as u32;
        }
        // pad accumulator to a full 24-bit group
        acc <<= 6 * (4 - n);
        match n {
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
            _ => return Err("invalid base64 length".into()),
        }
    }
    Ok(out)
}

/// Errors from cert generation / loading.
#[derive(Debug)]
pub enum CertError {
    Io(std::io::Error),
    Rcgen(rcgen::Error),
    Rustls(rustls::Error),
    Base64(String),
    BadIp(String),
    NoCert,
    NoKey,
}

impl std::fmt::Display for CertError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CertError::Io(e) => write!(f, "io: {e}"),
            CertError::Rcgen(e) => write!(f, "rcgen: {e}"),
            CertError::Rustls(e) => write!(f, "rustls: {e}"),
            CertError::Base64(e) => write!(f, "base64: {e}"),
            CertError::BadIp(s) => write!(f, "bad ip SAN: {s}"),
            CertError::NoCert => write!(f, "no certificate in PEM"),
            CertError::NoKey => write!(f, "no private key in PEM"),
        }
    }
}

impl std::error::Error for CertError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_and_build_server_config() {
        let g = generate().expect("generate certs");
        let bundle = CertBundle {
            ca_cert_pem: g.ca_cert_pem,
            leaf_cert_pem: g.leaf_cert_pem,
            leaf_key_pem: g.leaf_key_pem,
            ca_cert_path: PathBuf::from("/dev/null"),
        };
        assert!(bundle.ca_cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(bundle.leaf_cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(bundle.leaf_key_pem.contains("PRIVATE KEY"));
        // Building a ServerConfig exercises PEM parsing + key loading + provider.
        let _config = server_config(&bundle).expect("build ServerConfig");
    }

    #[test]
    fn leaf_has_expected_sans() {
        // Re-derive the SAN set and assert the required hosts are present.
        let sans = build_sans().expect("build SANs");
        let dns: Vec<String> = sans
            .iter()
            .filter_map(|s| match s {
                SanType::DnsName(n) => Some(n.to_string()),
                _ => None,
            })
            .collect();
        for required in [
            "api.platform.wizards.com",
            "doorbellprod.w2.mtgarena.com",
            "assets.mtgarena.wizards.com",
            "bike.w2.mtgarena.com",
            "localhost",
        ] {
            assert!(dns.iter().any(|d| d == required), "missing DNS SAN {required}");
        }
        let has_loopback = sans.iter().any(|s| matches!(s, SanType::IpAddress(ip) if ip.is_loopback()));
        assert!(has_loopback, "missing 127.0.0.1 IP SAN");
    }

    #[test]
    fn base64_roundtrip_via_pem() {
        // Generated leaf PEM must round-trip through our PEM/base64 reader into
        // at least one DER cert.
        let g = generate().unwrap();
        let certs = parse_certs(&g.leaf_cert_pem).unwrap();
        assert_eq!(certs.len(), 1);
        assert!(!certs[0].as_ref().is_empty());
    }
}
