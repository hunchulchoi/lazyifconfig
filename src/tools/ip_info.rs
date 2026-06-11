use std::net::IpAddr;

use super::{run_command, ToolCommandSpec, ToolInput, ToolResult, ToolResultSection};
use crate::tools::whois::{first_field, parse_whois_output};

pub fn reverse_dns_command_candidates(ip: &str) -> Vec<ToolCommandSpec> {
    vec![
        ToolCommandSpec {
            display: format!("dig -x {ip} +short"),
            program: "dig".to_string(),
            args: vec!["-x".to_string(), ip.to_string(), "+short".to_string()],
        },
        ToolCommandSpec {
            display: format!("host {ip}"),
            program: "host".to_string(),
            args: vec![ip.to_string()],
        },
        ToolCommandSpec {
            display: format!("nslookup {ip}"),
            program: "nslookup".to_string(),
            args: vec![ip.to_string()],
        },
    ]
}

pub async fn run(input: ToolInput) -> Result<ToolResult, String> {
    let ip = input.get("ip").unwrap_or("").trim();
    if ip.is_empty() {
        return Err("IP is required.".to_string());
    }
    ip.parse::<IpAddr>()
        .map_err(|_| "IP must be a valid IPv4 or IPv6 address.".to_string())?;

    let mut raw_chunks = Vec::new();
    let mut reverse_name = None;
    let mut reverse_error = None;

    for spec in reverse_dns_command_candidates(ip) {
        match run_command(&spec).await {
            Ok((stdout, stderr, _code)) => {
                raw_chunks.push(format!("$ {}\n{}{}", spec.display, stdout, stderr));
                reverse_name =
                    parse_reverse_dns_output(&stdout).or_else(|| parse_reverse_dns_output(&stderr));
                if reverse_name.is_some() {
                    break;
                }
            }
            Err(err) => reverse_error = Some(format!("{} ({err})", spec.program)),
        }
    }

    let whois_spec = super::whois::command_spec(ip);
    let (whois_stdout, whois_stderr, whois_code) = match run_command(&whois_spec).await {
        Ok(output) => output,
        Err(err) => {
            raw_chunks.push(format!("$ {}\n{}", whois_spec.display, err));
            ("".to_string(), err, None)
        }
    };
    if !whois_stdout.is_empty() || !whois_stderr.is_empty() {
        raw_chunks.push(format!(
            "$ {}\n{}{}",
            whois_spec.display, whois_stdout, whois_stderr
        ));
    }

    let combined = if whois_stdout.is_empty() {
        whois_stderr.as_str()
    } else {
        whois_stdout.as_str()
    };
    let organization = first_field(
        combined,
        &[
            "OrgName",
            "org-name",
            "org",
            "Organization",
            "descr",
            "owner",
        ],
    );
    let country = first_field(combined, &["Country", "country"]);
    let asn = first_field(combined, &["origin", "originas", "aut-num", "ASNumber"]);

    let mut sections = vec![ToolResultSection {
        label: "Summary".to_string(),
        lines: vec![
            format!("IP: {ip}"),
            format!(
                "Reverse DNS: {}",
                reverse_name
                    .clone()
                    .unwrap_or_else(|| "Unavailable".to_string())
            ),
            format!(
                "Organization: {}",
                organization
                    .clone()
                    .unwrap_or_else(|| "Unavailable".to_string())
            ),
            format!(
                "ASN: {}",
                asn.clone().unwrap_or_else(|| "Unavailable".to_string())
            ),
            format!(
                "Country: {}",
                country.clone().unwrap_or_else(|| "Unavailable".to_string())
            ),
        ],
    }];

    let whois_sections = parse_whois_output(ip, &whois_stdout, &whois_stderr, whois_code);
    if let Some(dates) = whois_sections
        .into_iter()
        .find(|section| section.label == "Dates")
    {
        sections.push(dates);
    }

    let mut diagnostics = Vec::new();
    if reverse_name.is_none() {
        diagnostics
            .push(reverse_error.unwrap_or_else(|| "No reverse DNS answer was parsed.".to_string()));
    }
    if organization.is_none() && asn.is_none() && country.is_none() {
        diagnostics.push("Whois ownership details were limited for this IP.".to_string());
    }
    if diagnostics.is_empty() {
        diagnostics.push("Reverse DNS and registration metadata collected.".to_string());
    }
    sections.push(ToolResultSection {
        label: "Diagnostics".to_string(),
        lines: diagnostics,
    });

    Ok(ToolResult {
        title: "IP Information".to_string(),
        sections,
        raw_output: raw_chunks.join("\n"),
    })
}

fn parse_reverse_dns_output(text: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim().trim_end_matches('.');
        if trimmed.is_empty() {
            continue;
        }
        if let Some(value) = trimmed.strip_prefix("domain name pointer ") {
            return Some(value.trim().trim_end_matches('.').to_string());
        }
        if trimmed.contains(" domain name pointer ") {
            return trimmed
                .split(" domain name pointer ")
                .nth(1)
                .map(|value| value.trim().trim_end_matches('.').to_string());
        }
        if trimmed.contains('.') && !trimmed.contains(' ') {
            return Some(trimmed.to_string());
        }
    }
    None
}
