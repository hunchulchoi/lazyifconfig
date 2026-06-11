use std::net::TcpStream;
use std::sync::Arc;
use std::time::Duration;

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore};
use x509_parser::prelude::*;

use super::{ToolCommandSpec, ToolInput, ToolResult, ToolResultSection};

pub fn command_spec(host: &str, port: u16) -> ToolCommandSpec {
    ToolCommandSpec {
        display: format!("openssl s_client -connect {host}:{port} -servername {host} -showcerts"),
        program: "openssl".to_string(),
        args: vec![
            "s_client".to_string(),
            "-connect".to_string(),
            format!("{host}:{port}"),
            "-servername".to_string(),
            host.to_string(),
            "-showcerts".to_string(),
        ],
    }
}

pub async fn run(input: ToolInput) -> Result<ToolResult, String> {
    let target = input.get("target").unwrap_or("").trim();
    if target.is_empty() {
        return Err("Target is required.".to_string());
    }

    let (host, port) = parse_target(target)?;
    let host_for_task = host.clone();
    let metadata = tokio::task::spawn_blocking(move || fetch_tls_metadata(&host_for_task, port))
        .await
        .map_err(|e| format!("TLS task failed: {e}"))??;
    let raw_output = metadata.raw_output.clone();
    let sections = tls_metadata_sections(&host, port, metadata);

    Ok(ToolResult {
        title: "TLS Inspector".to_string(),
        sections,
        raw_output,
    })
}

struct TlsMetadata {
    protocol: Option<String>,
    cipher: Option<String>,
    subject: Option<String>,
    issuer: Option<String>,
    not_before: Option<String>,
    not_after: Option<String>,
    san_dns: Vec<String>,
    certificate_count: usize,
    raw_output: String,
}

fn fetch_tls_metadata(host: &str, port: u16) -> Result<TlsMetadata, String> {
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    let config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let server_name = ServerName::try_from(host.to_string())
        .map_err(|_| "Target host is not a valid DNS name.".to_string())?;
    let mut conn = ClientConnection::new(Arc::new(config), server_name)
        .map_err(|e| format!("TLS client setup failed: {e}"))?;

    let mut sock =
        TcpStream::connect((host, port)).map_err(|e| format!("TCP connect failed: {e}"))?;
    let timeout = Some(Duration::from_secs(10));
    sock.set_read_timeout(timeout)
        .map_err(|e| format!("Could not set read timeout: {e}"))?;
    sock.set_write_timeout(timeout)
        .map_err(|e| format!("Could not set write timeout: {e}"))?;

    while conn.is_handshaking() {
        conn.complete_io(&mut sock)
            .map_err(|e| format!("TLS handshake failed: {e}"))?;
    }

    let protocol = conn.protocol_version().map(|value| format!("{value:?}"));
    let cipher = conn
        .negotiated_cipher_suite()
        .map(|suite| format!("{:?}", suite.suite()));
    let certs = conn
        .peer_certificates()
        .map(|certs| certs.to_vec())
        .unwrap_or_default();

    let mut metadata = TlsMetadata {
        protocol,
        cipher,
        subject: None,
        issuer: None,
        not_before: None,
        not_after: None,
        san_dns: Vec::new(),
        certificate_count: certs.len(),
        raw_output: format!("native rustls TLS handshake to {host}:{port}\n"),
    };

    if let Some(first) = certs.first() {
        let (_, cert) = X509Certificate::from_der(first.as_ref())
            .map_err(|e| format!("Certificate parse failed: {e}"))?;
        metadata.subject = Some(cert.subject().to_string());
        metadata.issuer = Some(cert.issuer().to_string());
        metadata.not_before = Some(cert.validity().not_before.to_string());
        metadata.not_after = Some(cert.validity().not_after.to_string());
        if let Ok(Some(san)) = cert.subject_alternative_name() {
            metadata.san_dns = san
                .value
                .general_names
                .iter()
                .filter_map(|name| match name {
                    GeneralName::DNSName(value) => Some((*value).to_string()),
                    _ => None,
                })
                .collect();
        }
    }

    metadata.raw_output.push_str(&format!(
        "protocol={}\ncipher={}\ncertificates={}\n",
        metadata.protocol.as_deref().unwrap_or("unknown"),
        metadata.cipher.as_deref().unwrap_or("unknown"),
        metadata.certificate_count
    ));

    Ok(metadata)
}

fn tls_metadata_sections(host: &str, port: u16, metadata: TlsMetadata) -> Vec<ToolResultSection> {
    let mut summary = vec![
        format!("Target: {host}:{port}"),
        "Status: Handshake completed".to_string(),
    ];
    if let Some(value) = metadata.protocol.clone() {
        summary.push(format!("Protocol: {value}"));
    }
    if let Some(value) = metadata.cipher.clone() {
        summary.push(format!("Cipher: {value}"));
    }

    let mut sections = vec![ToolResultSection {
        label: "Summary".to_string(),
        lines: summary,
    }];

    let mut certificate = Vec::new();
    if let Some(value) = metadata.subject {
        certificate.push(format!("Subject: {value}"));
    }
    if let Some(value) = metadata.issuer {
        certificate.push(format!("Issuer: {value}"));
    }
    if let Some(value) = metadata.not_before {
        certificate.push(format!("Not Before: {value}"));
    }
    if let Some(value) = metadata.not_after {
        certificate.push(format!("Not After: {value}"));
    }
    if !metadata.san_dns.is_empty() {
        certificate.push(format!("DNS SANs: {}", metadata.san_dns.join(", ")));
    }
    if !certificate.is_empty() {
        sections.push(ToolResultSection {
            label: "Certificate".to_string(),
            lines: certificate,
        });
    }

    sections.push(ToolResultSection {
        label: "Diagnostics".to_string(),
        lines: vec![format!(
            "Native rustls handshake parsed {} certificate(s).",
            metadata.certificate_count
        )],
    });

    sections
}

fn parse_target(target: &str) -> Result<(String, u16), String> {
    if let Some((host, port_raw)) = target.rsplit_once(':') {
        let port = port_raw
            .parse::<u16>()
            .map_err(|_| "Port must be a number from 1 to 65535.".to_string())?;
        if host.trim().is_empty() || port == 0 {
            return Err("Target must look like host:port.".to_string());
        }
        return Ok((host.trim().to_string(), port));
    }

    Ok((target.to_string(), 443))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_host_port_targets() {
        assert_eq!(
            parse_target("example.com").unwrap(),
            ("example.com".to_string(), 443)
        );
        assert_eq!(
            parse_target("example.com:8443").unwrap(),
            ("example.com".to_string(), 8443)
        );
    }
}
