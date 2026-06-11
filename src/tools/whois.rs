use std::net::IpAddr;

use serde_json::Value;

use super::{run_command, ToolCommandSpec, ToolInput, ToolResult, ToolResultSection};

pub fn command_spec(target: &str) -> ToolCommandSpec {
    ToolCommandSpec {
        display: format!("whois {target}"),
        program: "whois".to_string(),
        args: vec![target.to_string()],
    }
}

pub async fn run(input: ToolInput) -> Result<ToolResult, String> {
    let target = input.get("target").unwrap_or("").trim();
    if target.is_empty() {
        return Err("Target is required.".to_string());
    }

    let spec = command_spec(target);
    let (raw_output, parsed) = match run_command(&spec).await {
        Ok((stdout, stderr, code)) => (
            format!("$ {}\n{}{}", spec.display, stdout, stderr),
            parse_whois_output(target, &stdout, &stderr, code),
        ),
        Err(command_error) => run_rdap_lookup(target, &command_error).await?,
    };

    Ok(ToolResult {
        title: "Whois Lookup".to_string(),
        sections: parsed,
        raw_output,
    })
}

fn rdap_command_spec(target: &str) -> ToolCommandSpec {
    let kind = if target.parse::<IpAddr>().is_ok() {
        "ip"
    } else {
        "domain"
    };
    let url = format!("https://rdap.org/{kind}/{target}");
    ToolCommandSpec {
        display: format!("curl -sS -L -m 10 {url}"),
        program: "curl".to_string(),
        args: vec![
            "-sS".to_string(),
            "-L".to_string(),
            "-m".to_string(),
            "10".to_string(),
            url,
        ],
    }
}

async fn run_rdap_lookup(
    target: &str,
    whois_error: &str,
) -> Result<(String, Vec<ToolResultSection>), String> {
    let spec = rdap_command_spec(target);
    let (stdout, stderr, code) = run_command(&spec).await?;
    let raw_output = format!("$ {}\n{}{}", spec.display, stdout, stderr);
    let json: Value = serde_json::from_str(&stdout)
        .map_err(|e| format!("RDAP response was not valid JSON: {e}"))?;
    let mut sections = parse_rdap_output(target, &json, code);
    if let Some(diagnostics) = sections
        .iter_mut()
        .find(|section| section.label == "Diagnostics")
    {
        diagnostics
            .lines
            .push(format!("Local whois unavailable: {whois_error}"));
        if !stderr.trim().is_empty() {
            diagnostics.lines.push(stderr.trim().to_string());
        }
    }
    Ok((raw_output, sections))
}

pub(crate) fn parse_rdap_output(
    target: &str,
    value: &Value,
    code: Option<i32>,
) -> Vec<ToolResultSection> {
    let mut summary = vec![format!("Target: {target}")];
    if code == Some(0) {
        summary.push("Status: RDAP lookup completed".to_string());
    } else {
        summary.push(format!(
            "Status: RDAP command exited with status {:?}",
            code
        ));
    }
    if let Some(handle) = text_field(value, "handle") {
        summary.push(format!("Handle: {handle}"));
    }
    if let Some(name) = text_field(value, "name").or_else(|| text_field(value, "ldhName")) {
        summary.push(format!("Name: {name}"));
    }
    if let Some(country) = text_field(value, "country") {
        summary.push(format!("Country: {country}"));
    }
    if let Some(range) = rdap_range(value) {
        summary.push(format!("Range: {range}"));
    }

    let mut sections = vec![ToolResultSection {
        label: "Summary".to_string(),
        lines: summary,
    }];

    let events = rdap_events(value);
    if !events.is_empty() {
        sections.push(ToolResultSection {
            label: "Events".to_string(),
            lines: events,
        });
    }

    let entities = rdap_entities(value);
    if !entities.is_empty() {
        sections.push(ToolResultSection {
            label: "Entities".to_string(),
            lines: entities,
        });
    }

    sections.push(ToolResultSection {
        label: "Diagnostics".to_string(),
        lines: vec!["Used RDAP fallback over HTTPS.".to_string()],
    });

    sections
}

pub(crate) fn parse_whois_output(
    target: &str,
    stdout: &str,
    stderr: &str,
    code: Option<i32>,
) -> Vec<ToolResultSection> {
    let combined = if stdout.is_empty() { stderr } else { stdout };
    let registrar = first_field(combined, &["Registrar", "registrar"]);
    let organization = first_field(
        combined,
        &[
            "OrgName",
            "org-name",
            "org",
            "Organization",
            "Registrant Organization",
            "owner",
        ],
    );
    let country = first_field(combined, &["Country", "Registrant Country", "country"]);
    let created = first_field(
        combined,
        &[
            "Creation Date",
            "Created On",
            "created",
            "Registration Time",
        ],
    );
    let updated = first_field(combined, &["Updated Date", "Last Updated On", "updated"]);
    let expiry = first_field(
        combined,
        &[
            "Registry Expiry Date",
            "Registrar Registration Expiration Date",
            "Expiry Date",
            "paid-till",
            "expires",
        ],
    );
    let name_servers = collect_fields(combined, &["Name Server", "nserver", "Name Servers"]);

    let mut summary = vec![format!("Target: {target}")];
    if code == Some(0) {
        summary.push("Status: Lookup completed".to_string());
    } else {
        summary.push(format!("Status: Command exited with status {:?}", code));
    }
    if let Some(value) = registrar.clone() {
        summary.push(format!("Registrar: {value}"));
    }
    if let Some(value) = organization.clone() {
        summary.push(format!("Organization: {value}"));
    }
    if let Some(value) = country.clone() {
        summary.push(format!("Country: {value}"));
    }

    let mut sections = vec![ToolResultSection {
        label: "Summary".to_string(),
        lines: summary,
    }];

    let mut dates = Vec::new();
    if let Some(value) = created {
        dates.push(format!("Created: {value}"));
    }
    if let Some(value) = updated {
        dates.push(format!("Updated: {value}"));
    }
    if let Some(value) = expiry {
        dates.push(format!("Expires: {value}"));
    }
    if !dates.is_empty() {
        sections.push(ToolResultSection {
            label: "Dates".to_string(),
            lines: dates,
        });
    }

    if !name_servers.is_empty() {
        sections.push(ToolResultSection {
            label: "Name Servers".to_string(),
            lines: name_servers,
        });
    }

    let mut diagnostics = Vec::new();
    if registrar.is_none() && organization.is_none() && country.is_none() {
        diagnostics
            .push("Parsed metadata was limited; inspect raw output for full details.".to_string());
    }
    if !stderr.trim().is_empty() {
        diagnostics.push(stderr.trim().to_string());
    }
    if diagnostics.is_empty() {
        diagnostics.push("Whois lookup returned structured ownership details.".to_string());
    }
    sections.push(ToolResultSection {
        label: "Diagnostics".to_string(),
        lines: diagnostics,
    });

    sections
}

pub(crate) fn first_field(text: &str, names: &[&str]) -> Option<String> {
    text.lines().find_map(|line| {
        names
            .iter()
            .find_map(|name| extract_named_field(line, name))
            .filter(|value| !value.is_empty())
    })
}

pub(crate) fn collect_fields(text: &str, names: &[&str]) -> Vec<String> {
    let mut values = Vec::new();
    for line in text.lines() {
        for name in names {
            if let Some(value) = extract_named_field(line, name) {
                if !value.is_empty() && !values.contains(&value) {
                    values.push(value);
                }
            }
        }
    }
    values
}

fn extract_named_field(line: &str, name: &str) -> Option<String> {
    let trimmed = line.trim();
    let lower = trimmed.to_lowercase();
    let name_lower = name.to_lowercase();

    for separator in [':', '='] {
        let pattern = format!("{name_lower}{separator}");
        if lower.starts_with(&pattern) {
            return Some(trimmed[name.len() + 1..].trim().to_string());
        }
    }

    None
}

fn text_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn rdap_range(value: &Value) -> Option<String> {
    match (
        text_field(value, "startAddress"),
        text_field(value, "endAddress"),
    ) {
        (Some(start), Some(end)) => Some(format!("{start} - {end}")),
        _ => None,
    }
}

fn rdap_events(value: &Value) -> Vec<String> {
    value
        .get("events")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|event| {
            let action = text_field(event, "eventAction")?;
            let date = text_field(event, "eventDate")?;
            Some(format!("{action}: {date}"))
        })
        .collect()
}

fn rdap_entities(value: &Value) -> Vec<String> {
    value
        .get("entities")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(entity_name)
        .collect()
}

fn entity_name(entity: &Value) -> Option<String> {
    let roles = entity
        .get("roles")
        .and_then(Value::as_array)
        .map(|roles| {
            roles
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|roles| !roles.is_empty());

    let name = entity
        .get("vcardArray")
        .and_then(Value::as_array)
        .and_then(|vcard| vcard.get(1))
        .and_then(Value::as_array)
        .and_then(|fields| {
            fields.iter().find_map(|field| {
                let field = field.as_array()?;
                if field.first()?.as_str()? == "fn" {
                    field.get(3)?.as_str().map(str::to_string)
                } else {
                    None
                }
            })
        })?;

    Some(match roles {
        Some(roles) => format!("{name} ({roles})"),
        None => name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rdap_domain_output() {
        let value: Value = serde_json::json!({
            "handle": "2336799_DOMAIN_COM-VRSN",
            "ldhName": "EXAMPLE.COM",
            "events": [
                {"eventAction": "registration", "eventDate": "1995-08-14T04:00:00Z"}
            ],
            "entities": [
                {
                    "roles": ["registrar"],
                    "vcardArray": ["vcard", [["fn", {}, "text", "Example Registrar"]]]
                }
            ]
        });

        let sections = parse_rdap_output("example.com", &value, Some(0));

        assert!(sections[0]
            .lines
            .contains(&"Status: RDAP lookup completed".to_string()));
        assert!(sections[0].lines.contains(&"Name: EXAMPLE.COM".to_string()));
        assert!(sections.iter().any(|section| section.label == "Entities"
            && section
                .lines
                .contains(&"Example Registrar (registrar)".to_string())));
    }
}
