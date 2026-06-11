use std::collections::BTreeMap;
use std::net::IpAddr;
use std::time::Duration;

pub mod dns;
pub mod ip_info;
pub mod ping;
pub mod port_check;
pub mod tls;
pub mod traceroute;
pub mod whois;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ToolId {
    DnsLookup,
    WhoisLookup,
    IpInformation,
    PortCheck,
    TlsInspector,
    Ping,
    Traceroute,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolAvailability {
    Runnable,
    Planned,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToolField {
    pub key: &'static str,
    pub label: &'static str,
    pub placeholder: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolDefinition {
    pub id: ToolId,
    pub name: &'static str,
    pub description: &'static str,
    pub availability: ToolAvailability,
    pub fields: &'static [ToolField],
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ToolInput {
    pub values: BTreeMap<String, String>,
}

impl ToolInput {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolResultSection {
    pub label: String,
    pub lines: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolResult {
    pub title: String,
    pub sections: Vec<ToolResultSection>,
    pub raw_output: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolExecutionState {
    Idle,
    Running,
    Succeeded,
    Failed,
}

#[derive(Clone, Debug)]
pub struct ToolRegistry {
    definitions: Vec<ToolDefinition>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self {
            definitions: vec![
                ToolDefinition {
                    id: ToolId::DnsLookup,
                    name: "DNS Lookup",
                    description: "Resolve DNS records for a domain or IP.",
                    availability: ToolAvailability::Runnable,
                    fields: &[ToolField {
                        key: "target",
                        label: "Target",
                        placeholder: "example.com",
                    }],
                },
                ToolDefinition {
                    id: ToolId::WhoisLookup,
                    name: "Whois Lookup",
                    description: "Look up domain or IP ownership metadata.",
                    availability: ToolAvailability::Runnable,
                    fields: &[ToolField {
                        key: "target",
                        label: "Target",
                        placeholder: "github.com",
                    }],
                },
                ToolDefinition {
                    id: ToolId::IpInformation,
                    name: "IP Information",
                    description: "Summarize ASN, organization, country, and reverse DNS.",
                    availability: ToolAvailability::Runnable,
                    fields: &[ToolField {
                        key: "ip",
                        label: "IP",
                        placeholder: "8.8.8.8",
                    }],
                },
                ToolDefinition {
                    id: ToolId::PortCheck,
                    name: "Port Check",
                    description: "Check TCP connectivity to a host and port.",
                    availability: ToolAvailability::Runnable,
                    fields: &[
                        ToolField {
                            key: "host",
                            label: "Host",
                            placeholder: "github.com",
                        },
                        ToolField {
                            key: "port",
                            label: "Port",
                            placeholder: "443",
                        },
                    ],
                },
                ToolDefinition {
                    id: ToolId::TlsInspector,
                    name: "TLS Inspector",
                    description: "Inspect certificate and TLS details.",
                    availability: ToolAvailability::Runnable,
                    fields: &[ToolField {
                        key: "target",
                        label: "Target",
                        placeholder: "github.com:443",
                    }],
                },
                ToolDefinition {
                    id: ToolId::Ping,
                    name: "Ping",
                    description: "Measure reachability and latency with ping.",
                    availability: ToolAvailability::Runnable,
                    fields: &[ToolField {
                        key: "target",
                        label: "Target",
                        placeholder: "8.8.8.8",
                    }],
                },
                ToolDefinition {
                    id: ToolId::Traceroute,
                    name: "Traceroute",
                    description: "Visualize the packet path to a target.",
                    availability: ToolAvailability::Runnable,
                    fields: &[ToolField {
                        key: "target",
                        label: "Target",
                        placeholder: "8.8.8.8",
                    }],
                },
            ],
        }
    }
}

impl ToolRegistry {
    pub fn definitions(&self) -> &[ToolDefinition] {
        &self.definitions
    }

    pub fn definition(&self, id: ToolId) -> Option<&ToolDefinition> {
        self.definitions
            .iter()
            .find(|definition| definition.id == id)
    }
}

pub fn tool_id_from_cli_name(name: &str) -> Option<ToolId> {
    match name {
        "dns" | "dns-lookup" => Some(ToolId::DnsLookup),
        "whois" | "whois-lookup" => Some(ToolId::WhoisLookup),
        "ip-info" | "ip-information" => Some(ToolId::IpInformation),
        "port-check" => Some(ToolId::PortCheck),
        "tls" | "tls-inspector" => Some(ToolId::TlsInspector),
        "ping" => Some(ToolId::Ping),
        "traceroute" => Some(ToolId::Traceroute),
        _ => None,
    }
}

pub fn tool_cli_name(id: ToolId) -> &'static str {
    match id {
        ToolId::DnsLookup => "dns",
        ToolId::WhoisLookup => "whois",
        ToolId::IpInformation => "ip-info",
        ToolId::PortCheck => "port-check",
        ToolId::TlsInspector => "tls",
        ToolId::Ping => "ping",
        ToolId::Traceroute => "traceroute",
    }
}

pub fn tool_input_from_cli_args(id: ToolId, args: &[&str]) -> Result<ToolInput, String> {
    let registry = ToolRegistry::default();
    let definition = registry
        .definition(id)
        .ok_or_else(|| "Unknown tool id.".to_string())?;

    if args.len() != definition.fields.len() {
        return Err(format!(
            "Usage: lazyifconfig tools {} {}",
            tool_cli_name(id),
            definition
                .fields
                .iter()
                .map(|field| format!("<{}>", field.key))
                .collect::<Vec<_>>()
                .join(" ")
        ));
    }

    let mut values = BTreeMap::new();
    for (field, value) in definition.fields.iter().zip(args.iter()) {
        values.insert(field.key.to_string(), (*value).to_string());
    }

    let input = ToolInput { values };
    let errors = validate_tool_input(id, &input);
    if errors.is_empty() {
        Ok(input)
    } else {
        Err(errors.join(" "))
    }
}

pub fn format_tool_result_plaintext(result: &ToolResult) -> String {
    let mut lines = vec![result.title.clone()];

    for section in &result.sections {
        lines.push(String::new());
        lines.push(format!("[{}]", section.label));
        lines.extend(section.lines.iter().cloned());
    }

    if !result.raw_output.trim().is_empty() {
        lines.push(String::new());
        lines.push("[Raw Output]".to_string());
        lines.push(result.raw_output.clone());
    }

    lines.join("\n")
}

pub fn tools_cli_usage() -> String {
    let registry = ToolRegistry::default();
    let mut lines = vec![
        "Usage: lazyifconfig tools <tool> [args]".to_string(),
        String::new(),
        "Available tools:".to_string(),
    ];

    for definition in registry.definitions() {
        let args = definition
            .fields
            .iter()
            .map(|field| format!("<{}>", field.key))
            .collect::<Vec<_>>()
            .join(" ");
        lines.push(format!(
            "  {:<12} {} {}",
            tool_cli_name(definition.id),
            args,
            definition.description
        ));
    }

    lines.join("\n")
}

pub fn validate_tool_input(id: ToolId, input: &ToolInput) -> Vec<String> {
    match id {
        ToolId::DnsLookup
        | ToolId::WhoisLookup
        | ToolId::Ping
        | ToolId::Traceroute => validate_required_fields(input, &["target"]),
        ToolId::IpInformation => {
            let mut errors = validate_required_fields(input, &["ip"]);
            let ip = input.get("ip").unwrap_or("").trim();
            if !ip.is_empty() && ip.parse::<IpAddr>().is_err() {
                errors.push("IP must be a valid IPv4 or IPv6 address.".to_string());
            }
            errors
        }
        ToolId::PortCheck => {
            let mut errors = validate_required_fields(input, &["host", "port"]);
            let port = input.get("port").unwrap_or("").trim();
            if !port.is_empty() && !is_valid_port(port) {
                errors.push("Port must be a number from 1 to 65535.".to_string());
            }
            errors
        }
        ToolId::TlsInspector => {
            let mut errors = validate_required_fields(input, &["target"]);
            let target = input.get("target").unwrap_or("").trim();
            if !target.is_empty() && !is_valid_tls_target(target) {
                errors.push("Target must look like host:port or host.".to_string());
            }
            errors
        }
    }
}

fn validate_required_fields(input: &ToolInput, fields: &[&str]) -> Vec<String> {
    fields
        .iter()
        .filter_map(|field| {
            input.get(field)
                .map(str::trim)
                .filter(|value| value.is_empty())
                .or_else(|| input.get(field).is_none().then_some(""))
                .map(|_| format!("{} is required.", field_label(field)))
        })
        .collect()
}

fn field_label(field: &str) -> &'static str {
    match field {
        "target" => "Target",
        "ip" => "IP",
        "host" => "Host",
        "port" => "Port",
        _ => "Field",
    }
}

fn is_valid_port(port: &str) -> bool {
    port.parse::<u16>().is_ok_and(|value| value != 0)
}

fn is_valid_tls_target(target: &str) -> bool {
    if target.is_empty() {
        return false;
    }
    if let Some((host, port)) = target.rsplit_once(':') {
        !host.trim().is_empty() && is_valid_port(port.trim())
    } else {
        !target.trim().is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolCommandSpec {
    pub display: String,
    pub program: String,
    pub args: Vec<String>,
}

pub async fn run_tool(id: ToolId, input: ToolInput) -> Result<ToolResult, String> {
    match id {
        ToolId::DnsLookup => dns::run(input).await,
        ToolId::WhoisLookup => whois::run(input).await,
        ToolId::IpInformation => ip_info::run(input).await,
        ToolId::PortCheck => port_check::run(input, Duration::from_secs(3)).await,
        ToolId::TlsInspector => tls::run(input).await,
        ToolId::Ping => ping::run(input).await,
        ToolId::Traceroute => traceroute::run(input).await,
    }
}

pub async fn run_command(spec: &ToolCommandSpec) -> Result<(String, String, Option<i32>), String> {
    let output = tokio::process::Command::new(&spec.program)
        .args(&spec.args)
        .output()
        .await
        .map_err(|e| e.to_string())?;

    Ok((
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code(),
    ))
}
