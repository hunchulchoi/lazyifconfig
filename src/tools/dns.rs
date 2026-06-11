use std::net::IpAddr;
use std::time::Instant;

use super::{run_command, ToolCommandSpec, ToolInput, ToolResult, ToolResultSection};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DnsStatus {
    Success,
    NotFound,
    Timeout,
    ServerFailure,
    NoRecords,
    Failed,
}

impl DnsStatus {
    fn is_success(&self) -> bool {
        matches!(self, Self::Success)
    }

    fn label(&self) -> &'static str {
        match self {
            Self::Success => "Success",
            Self::NotFound => "Domain Not Found",
            Self::Timeout => "DNS Timeout",
            Self::ServerFailure => "DNS Server Failure",
            Self::NoRecords => "No Records Returned",
            Self::Failed => "Lookup Failed",
        }
    }

    fn symbol(&self) -> &'static str {
        match self {
            Self::Success => "✓",
            Self::NotFound
            | Self::Timeout
            | Self::ServerFailure
            | Self::NoRecords
            | Self::Failed => "✗",
        }
    }
}

#[derive(Clone, Debug)]
struct DnsRecord {
    host: String,
    value: String,
    ttl: Option<u64>,
}

#[derive(Clone, Debug)]
struct MxRecord {
    priority: u64,
    host: String,
}

#[derive(Clone, Debug)]
struct DnsParseResult {
    query: String,
    status: DnsStatus,
    response_time_ms: Option<u64>,
    dns_server: Option<String>,
    provider: Option<&'static str>,
    a_records: Vec<DnsRecord>,
    aaaa_records: Vec<DnsRecord>,
    cname_records: Vec<(String, String)>,
    mx_records: Vec<MxRecord>,
    diagnostics: Vec<String>,
}

impl DnsParseResult {
    fn total_record_count(&self) -> usize {
        self.a_records.len()
            + self.aaaa_records.len()
            + self.cname_records.len()
            + self.mx_records.len()
    }

    fn health_label(&self) -> &'static str {
        if !self.status.is_success() {
            return if matches!(self.status, DnsStatus::NotFound) {
                "Unknown"
            } else {
                "Check"
            };
        }

        let ms = match self.response_time_ms {
            Some(ms) => ms,
            None => return "Unknown",
        };

        if ms <= 10 {
            "Excellent"
        } else if ms <= 30 {
            "Good"
        } else if ms <= 100 {
            "Moderate"
        } else {
            "Slow"
        }
    }

    fn health_indicator(&self) -> &'static str {
        match self.health_label() {
            "Excellent" | "Good" => "✓",
            _ => "⚠",
        }
    }

    fn build_sections(&self) -> Vec<ToolResultSection> {
        let mut sections = Vec::new();

        let mut summary = vec![
            format!("Query: {}", self.query),
            format!("Status: {} {}", self.status.symbol(), self.status.label()),
        ];
        if let Some(response_time_ms) = self.response_time_ms {
            summary.push(format!("Response Time: {response_time_ms} ms"));
            summary.push(format!(
                "Health: {} {}",
                self.health_indicator(),
                self.health_label()
            ));
        }
        if let Some(dns_server) = &self.dns_server {
            summary.push(format!("DNS Server: {dns_server}"));
            if let Some(provider) = self.provider {
                summary.push(format!("Provider: {provider}"));
            }
        }
        summary.push(format!("Records: {}", self.total_record_count()));
        sections.push(ToolResultSection {
            label: "Summary".to_string(),
            lines: summary,
        });

        if !self.a_records.is_empty() {
            let mut lines = Vec::new();
            for record in &self.a_records {
                lines.push(format!("Host: {}", record.host));
                lines.push(format!("IPv4 Address: {}", record.value));
                if let Some(ttl) = record.ttl {
                    lines.push(format!("TTL: {ttl} sec"));
                }
            }
            sections.push(ToolResultSection {
                label: "A Records".to_string(),
                lines,
            });
        }

        if !self.aaaa_records.is_empty() {
            let mut lines = Vec::new();
            for record in &self.aaaa_records {
                lines.push(format!("Host: {}", record.host));
                lines.push(format!("IPv6 Address: {}", record.value));
                if let Some(ttl) = record.ttl {
                    lines.push(format!("TTL: {ttl} sec"));
                }
            }
            sections.push(ToolResultSection {
                label: "AAAA Records".to_string(),
                lines,
            });
        }

        if !self.cname_records.is_empty() {
            let mut lines = Vec::new();
            for (alias, target) in &self.cname_records {
                lines.push(format!("Alias: {alias}"));
                lines.push(format!("Points To: {target}"));
            }
            sections.push(ToolResultSection {
                label: "CNAME Records".to_string(),
                lines,
            });
        }

        if !self.mx_records.is_empty() {
            let mut lines = vec!["Priority   Host".to_string()];
            for record in &self.mx_records {
                lines.push(format!("{:<10} {}", record.priority, record.host));
            }
            sections.push(ToolResultSection {
                label: "MX Records".to_string(),
                lines,
            });
        }

        let flow = self.flow_lines();
        if !flow.is_empty() {
            sections.push(ToolResultSection {
                label: "DNS Flow".to_string(),
                lines: flow,
            });
        }

        let mut diagnostics = self.diagnostics.clone();
        if self.status.is_success() && self.total_record_count() == 0 {
            diagnostics.push("⚠ No record lines parsed from command output.".to_string());
        }
        if self.status == DnsStatus::Success
            && self.a_records.is_empty()
            && self.aaaa_records.is_empty()
        {
            diagnostics.push("⚠ No A/AAAA answer parsed; raw output may be useful.".to_string());
        }
        if self
            .provider
            .is_some_and(|provider| provider.contains("Tailscale"))
        {
            diagnostics.push("ℹ Using Tailscale DNS service.".to_string());
        }
        if self.response_time_ms.is_some_and(|ms| ms > 100) {
            diagnostics.push("⚠ Response time is slower than typical local DNS.".to_string());
        }
        sections.push(ToolResultSection {
            label: "Diagnostics".to_string(),
            lines: diagnostics,
        });

        sections
    }

    fn flow_lines(&self) -> Vec<String> {
        let mut nodes = Vec::new();
        nodes.push(self.query.clone());

        for (_alias, target) in &self.cname_records {
            nodes.push(target.clone());
        }

        if let Some(first_a) = self.a_records.first() {
            nodes.push(first_a.value.clone());
            return build_flow_lines(nodes);
        }

        if let Some(first_aaaa) = self.aaaa_records.first() {
            nodes.push(first_aaaa.value.clone());
            return build_flow_lines(nodes);
        }

        nodes.pop();
        build_flow_lines(nodes)
    }
}

pub fn command_candidates(target: &str) -> Vec<ToolCommandSpec> {
    vec![
        ToolCommandSpec {
            display: format!("dig {target}"),
            program: "dig".to_string(),
            args: vec![target.to_string()],
        },
        ToolCommandSpec {
            display: format!("host {target}"),
            program: "host".to_string(),
            args: vec![target.to_string()],
        },
        ToolCommandSpec {
            display: format!("nslookup {target}"),
            program: "nslookup".to_string(),
            args: vec![target.to_string()],
        },
    ]
}

pub async fn run(input: ToolInput) -> Result<ToolResult, String> {
    let target = input.get("target").unwrap_or("").trim();
    if target.is_empty() {
        return Err("Target is required.".to_string());
    }

    let mut failures = Vec::new();

    for spec in command_candidates(target) {
        let started_at = Instant::now();
        match run_command(&spec).await {
            Ok((stdout, stderr, code)) => {
                let response_time_ms = Some(started_at.elapsed().as_millis() as u64);
                let output = format!("$ {}\n{}{}", spec.display, stdout, stderr);
                let parsed = parse_dns_output(target, &stdout, &stderr, code, response_time_ms);
                let mut sections = parsed.build_sections();

                if code == Some(0) {
                    if parsed.status == DnsStatus::Failed {
                        sections.push(ToolResultSection {
                            label: "Command".to_string(),
                            lines: vec![format!("Command exited with status {:?}.", code)],
                        });
                    }
                } else {
                    sections.push(ToolResultSection {
                        label: "Command".to_string(),
                        lines: vec![format!("Command exited with status {:?}.", code)],
                    });
                }

                return Ok(ToolResult {
                    title: "DNS Lookup".to_string(),
                    sections,
                    raw_output: output,
                });
            }
            Err(err) => failures.push(format!("{} ({})", spec.program, err)),
        }
    }

    Err(format!(
        "No DNS command succeeded. Tried: {}",
        failures.join(", ")
    ))
}

fn parse_dns_output(
    query: &str,
    stdout: &str,
    stderr: &str,
    code: Option<i32>,
    response_time_ms: Option<u64>,
) -> DnsParseResult {
    let mut result = DnsParseResult {
        query: query.to_string(),
        status: if code == Some(0) {
            DnsStatus::Success
        } else {
            DnsStatus::Failed
        },
        response_time_ms,
        dns_server: None,
        provider: None,
        a_records: Vec::new(),
        aaaa_records: Vec::new(),
        cname_records: Vec::new(),
        mx_records: Vec::new(),
        diagnostics: Vec::new(),
    };

    let mut saw_dns_server_label = false;
    let mut combined_lines = Vec::new();
    combined_lines.extend(stdout.lines());
    combined_lines.extend(stderr.lines());
    for line in &combined_lines {
        parse_dns_server(line, &mut result, &mut saw_dns_server_label);
        parse_response_time(line, &mut result);
        parse_a_record(line, &mut result);
        parse_aaaa_record(line, &mut result);
        parse_cname_record(line, &mut result);
        parse_mx_record(line, &mut result);
        if saw_dns_server_label {
            parse_server_address_hint(line, &mut result);
        }
    }

    result.status = detect_status(code, &combined_lines, &result);
    if let Some(server) = &result.dns_server {
        result.provider = dns_provider(server);
        if result.provider.is_some() {
            result.diagnostics.push(format!(
                "Provider detected: {}",
                result.provider.unwrap_or("Unknown")
            ));
        }
    }

    if result.status == DnsStatus::Failed && code == Some(0) && !stdout.is_empty() {
        result
            .diagnostics
            .push("Resolver output was present but incomplete.".to_string());
    }

    if result.status == DnsStatus::Success {
        result
            .diagnostics
            .push("✓ DNS resolution successful".to_string());
    }

    result
}

fn parse_dns_server(line: &str, result: &mut DnsParseResult, saw_hint: &mut bool) {
    let trimmed = line.trim();
    let lower = trimmed.to_lowercase();

    if lower.contains("server:") || lower.contains(";; server:") {
        if let Some(server) = parse_ip_token(trimmed) {
            result.dns_server = Some(server);
            *saw_hint = false;
            return;
        }
        *saw_hint = true;
        return;
    }
    if lower.contains("using domain server") {
        *saw_hint = true;
    }
}

fn parse_server_address_hint(line: &str, result: &mut DnsParseResult) {
    if let Some(server) = parse_ip_token(line) {
        result.dns_server.get_or_insert(server);
    }
}

fn parse_response_time(line: &str, result: &mut DnsParseResult) {
    let lower = line.to_lowercase();
    if !(lower.contains("query time") || lower.contains("query time =")) {
        return;
    }
    if let Some(ms) = first_u64_in_text(line) {
        result.response_time_ms = Some(ms);
    }
}

fn parse_a_record(line: &str, result: &mut DnsParseResult) {
    let parsed = parse_record_by_type(line, "A");
    if let Some(record) = parsed {
        result.a_records.push(record);
    }
}

fn parse_aaaa_record(line: &str, result: &mut DnsParseResult) {
    let parsed = parse_record_by_type(line, "AAAA");
    if let Some(record) = parsed {
        result.aaaa_records.push(record);
    }
}

fn parse_cname_record(line: &str, result: &mut DnsParseResult) {
    let parsed = parse_record_by_type_cname(line);
    if let Some((alias, target)) = parsed {
        result.cname_records.push((alias, target));
    }
}

fn parse_mx_record(line: &str, result: &mut DnsParseResult) {
    let lower = line.to_lowercase();
    if !lower.contains(" mx ") && !lower.ends_with(" mx") && !lower.ends_with(" mx.") {
        if !lower.contains(" has mx") {
            return;
        }
    }

    let trimmed = line.trim();
    if trimmed.is_empty() {
        return;
    }

    let tokens = trimmed.split_whitespace().collect::<Vec<_>>();
    let mut index = None;
    for (idx, token) in tokens.iter().enumerate() {
        if *token == "MX" {
            index = Some(idx);
            break;
        }
    }
    let idx = match index {
        Some(idx) => idx,
        None => return,
    };
    if tokens.len() <= idx + 2 {
        return;
    }

    if let Ok(priority) = tokens[idx + 1].parse::<u64>() {
        result.mx_records.push(MxRecord {
            priority,
            host: trim_trailing_dot(tokens[idx + 2]),
        });
        result.mx_records.sort_by_key(|record| record.priority);
    }
}

fn parse_record_by_type(line: &str, record_type: &str) -> Option<DnsRecord> {
    if let Some(record) = parse_host_record(line, record_type) {
        return Some(record);
    }
    parse_dig_record(line, record_type)
}

fn parse_dig_record(line: &str, record_type: &str) -> Option<DnsRecord> {
    if line.starts_with(";;") {
        return None;
    }
    let tokens = line.split_whitespace().collect::<Vec<_>>();
    let idx = tokens.iter().position(|token| *token == record_type)?;
    if tokens.len() <= idx + 1 {
        return None;
    }

    let value = trim_trailing_dot(tokens[idx + 1]);
    if !is_ip_like(&value) {
        return None;
    }

    let host = tokens
        .first()
        .map(|value| trim_trailing_dot(value))
        .unwrap_or_default();
    let ttl = idx
        .checked_sub(2)
        .and_then(|ttl_index| {
            tokens
                .get(ttl_index)
                .and_then(|value| value.parse::<u64>().ok())
        })
        .or_else(|| {
            idx.checked_sub(1).and_then(|ttl_index| {
                tokens.get(ttl_index).and_then(|value| {
                    if *value == "IN" {
                        None
                    } else {
                        value.parse::<u64>().ok()
                    }
                })
            })
        });

    Some(DnsRecord {
        host,
        value: sanitize_address(&value),
        ttl,
    })
}

fn parse_host_record(line: &str, record_type: &str) -> Option<DnsRecord> {
    let lower = line.to_lowercase();
    let mut should_parse = false;
    if lower.contains(" has address ") && record_type == "A" {
        should_parse = true;
    } else if lower.contains(" has ipv6 address ") && record_type == "AAAA" {
        should_parse = true;
    }
    if !should_parse {
        return None;
    }

    let tokens = line.split_whitespace().collect::<Vec<_>>();
    let value = tokens
        .last()
        .map(|value| trim_trailing_dot(*value))
        .and_then(|value| parse_clean_ip(&value))?;
    let host = tokens
        .first()
        .map(|value| trim_trailing_dot(value))
        .unwrap_or_default();
    let ttl = tokens
        .iter()
        .find_map(|token| token.parse::<u64>().ok())
        .filter(|ttl| *ttl > 20 && *ttl < 86400);
    Some(DnsRecord { host, value, ttl })
}

fn parse_record_by_type_cname(line: &str) -> Option<(String, String)> {
    if !line.to_lowercase().contains(" cname ") {
        if !line.to_lowercase().contains(" is an alias for ") {
            return None;
        }
    }

    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    let tokens = trimmed.split_whitespace().collect::<Vec<_>>();
    if tokens.len() >= 5 && tokens[2] == "is" {
        let alias = trim_trailing_dot(tokens[0]);
        let target = trim_trailing_dot(tokens[4]);
        return Some((alias, target));
    }

    let marker = " is an alias for ";
    if let Some(start) = trimmed.to_lowercase().find(marker) {
        let alias = trim_trailing_dot(trimmed[..start].trim());
        let target = trim_trailing_dot(trimmed[start + marker.len()..].trim());
        return Some((alias.to_string(), target.to_string()));
    }

    if let Some(idx) = tokens.iter().position(|token| *token == "CNAME") {
        if tokens.len() <= idx + 1 {
            return None;
        }
        let alias = tokens
            .first()
            .map(|value| trim_trailing_dot(value))
            .unwrap_or_default();
        let target = trim_trailing_dot(tokens[idx + 1]);
        Some((alias, target))
    } else {
        None
    }
}

fn parse_ip_token(line: &str) -> Option<String> {
    line.split_whitespace()
        .find_map(|token| parse_clean_ip(token))
}

fn parse_clean_ip(candidate: &str) -> Option<String> {
    let token = candidate
        .trim_matches(|c| c == '(' || c == ')' || c == ',' || c == ';')
        .split('#')
        .next()?
        .to_string();
    token.parse::<IpAddr>().ok().map(|addr| addr.to_string())
}

fn is_ip_like(value: &str) -> bool {
    parse_clean_ip(value).is_some()
}

fn trim_trailing_dot(value: &str) -> String {
    value.trim_end_matches('.').to_string()
}

fn sanitize_address(value: &str) -> String {
    value.to_string()
}

fn first_u64_in_text(line: &str) -> Option<u64> {
    let mut num = String::new();
    for ch in line.chars() {
        if ch.is_ascii_digit() {
            num.push(ch);
            continue;
        }
        if !num.is_empty() {
            let parsed = num.parse::<u64>().ok();
            if parsed.is_some() {
                return parsed;
            }
            num.clear();
        }
    }
    if !num.is_empty() {
        return num.parse::<u64>().ok();
    }
    None
}

fn detect_status(code: Option<i32>, lines: &[&str], result: &DnsParseResult) -> DnsStatus {
    let merged = lines.join("\n").to_lowercase();
    if merged.contains("nxdomain") || merged.contains("name does not exist") {
        return DnsStatus::NotFound;
    }
    if merged.contains("timed out") || merged.contains("timeout") {
        return DnsStatus::Timeout;
    }
    if merged.contains("servfail") || merged.contains("server failure") {
        return DnsStatus::ServerFailure;
    }
    if result.total_record_count() > 0 && code == Some(0) {
        return DnsStatus::Success;
    }
    if code == Some(0) {
        return DnsStatus::NoRecords;
    }
    DnsStatus::Failed
}

fn build_flow_lines(nodes: Vec<String>) -> Vec<String> {
    if nodes.len() < 2 {
        return Vec::new();
    }

    let mut lines = Vec::new();
    let width = 8usize;
    for (idx, node) in nodes.iter().enumerate() {
        lines.push(node.clone());
        if idx + 1 < nodes.len() {
            lines.push(format!("{:width$}│", "", width = width));
            lines.push(format!("{:width$}▼", "", width = width));
        }
    }
    lines
}

fn dns_provider(server: &str) -> Option<&'static str> {
    match server {
        "100.100.100.100" => Some("Tailscale MagicDNS"),
        "1.1.1.1" | "1.0.0.1" => Some("Cloudflare"),
        "8.8.8.8" | "8.8.4.4" => Some("Google Public DNS"),
        "9.9.9.9" => Some("Quad9"),
        "208.67.222.222" | "208.67.220.220" => Some("OpenDNS"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dig_output_summary() {
        let output = "\
;; ->>HEADER<<- opcode: QUERY, status: NOERROR, id: 12345
;; ANSWER SECTION:
tameuscc.co.kr. 579 IN A 112.133.7.34
;; Query time: 85 msec
;; SERVER: 100.100.100.100#53(100.100.100.100)
";

        let result = parse_dns_output("tameuscc.co.kr", output, "", Some(0), Some(85));
        assert_eq!(result.status, DnsStatus::Success);
        assert_eq!(result.response_time_ms, Some(85));
        assert_eq!(result.dns_server.as_deref(), Some("100.100.100.100"));
        assert_eq!(result.a_records.len(), 1);
        assert_eq!(result.a_records[0].value, "112.133.7.34");
        assert_eq!(result.a_records[0].ttl, Some(579));
        assert_eq!(result.provider, Some("Tailscale MagicDNS"));
    }

    #[test]
    fn parses_nxdomain_output() {
        let output = ";; ->>HEADER<<- opcode: QUERY, status: NXDOMAIN, id: 1\n";
        let result = parse_dns_output("missing.example", output, "", Some(0), Some(20));
        assert_eq!(result.status, DnsStatus::NotFound);
        assert_eq!(result.a_records.len(), 0);
    }
}
