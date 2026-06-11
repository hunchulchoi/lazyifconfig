use super::{run_command, ToolCommandSpec, ToolInput, ToolResult, ToolResultSection};

pub fn command_spec_for_os(os: &str, target: &str) -> ToolCommandSpec {
    if os == "linux" {
        ToolCommandSpec {
            display: format!("traceroute -m 8 -w 1 {target}"),
            program: "traceroute".to_string(),
            args: vec![
                "-m".to_string(),
                "8".to_string(),
                "-w".to_string(),
                "1".to_string(),
                target.to_string(),
            ],
        }
    } else if os == "windows" {
        ToolCommandSpec {
            display: format!("tracert -h 8 {target}"),
            program: "tracert".to_string(),
            args: vec!["-h".to_string(), "8".to_string(), target.to_string()],
        }
    } else {
        ToolCommandSpec {
            display: format!("traceroute -m 8 {target}"),
            program: "traceroute".to_string(),
            args: vec!["-m".to_string(), "8".to_string(), target.to_string()],
        }
    }
}

pub async fn run(input: ToolInput) -> Result<ToolResult, String> {
    let target = input.get("target").unwrap_or("").trim();
    if target.is_empty() {
        return Err("Target is required.".to_string());
    }

    let spec = command_spec_for_os(std::env::consts::OS, target);
    let (stdout, stderr, code) = run_command(&spec).await?;
    let raw_output = format!("$ {}\n{}{}", spec.display, stdout, stderr);
    let hops = parse_hops(&stdout);
    let mut sections = vec![ToolResultSection {
        label: "Summary".to_string(),
        lines: vec![
            format!("Target: {target}"),
            format!("Status: traceroute exited with {:?}", code),
            format!("Hops Parsed: {}", hops.len()),
        ],
    }];

    if !hops.is_empty() {
        sections.push(ToolResultSection {
            label: "Hops".to_string(),
            lines: hops,
        });
    }

    let mut diagnostics = Vec::new();
    if !stderr.trim().is_empty() {
        diagnostics.push(stderr.trim().to_string());
    }
    if diagnostics.is_empty() && sections.len() == 1 {
        diagnostics
            .push("No hop lines were parsed; raw output may contain timeout details.".to_string());
    }
    if diagnostics.is_empty() {
        diagnostics.push("Traceroute completed with at least one parsed hop.".to_string());
    }
    sections.push(ToolResultSection {
        label: "Diagnostics".to_string(),
        lines: diagnostics,
    });

    Ok(ToolResult {
        title: "Traceroute".to_string(),
        sections,
        raw_output,
    })
}

fn parse_hops(stdout: &str) -> Vec<String> {
    let mut hops = Vec::new();
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed
            .split_whitespace()
            .next()
            .and_then(|token| token.parse::<usize>().ok())
            .is_some()
        {
            hops.push(trimmed.to_string());
        }
    }
    hops
}
