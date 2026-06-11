use super::{run_command, ToolCommandSpec, ToolInput, ToolResult, ToolResultSection};

pub fn command_spec_for_os(os: &str, target: &str) -> ToolCommandSpec {
    if os == "windows" {
        return ToolCommandSpec {
            display: format!("ping -n 4 {target}"),
            program: "ping".to_string(),
            args: vec!["-n".to_string(), "4".to_string(), target.to_string()],
        };
    }

    ToolCommandSpec {
        display: format!("ping -c 4 {target}"),
        program: "ping".to_string(),
        args: vec!["-c".to_string(), "4".to_string(), target.to_string()],
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
    let mut lines = Vec::new();

    if code == Some(0) {
        lines.push("Ping completed successfully.".to_string());
    } else {
        lines.push(format!("Ping exited with status {:?}.", code));
    }

    let summary_lines = stdout
        .lines()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev();
    for line in summary_lines {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            lines.push(trimmed.to_string());
        }
    }

    Ok(ToolResult {
        title: "Ping".to_string(),
        sections: vec![ToolResultSection {
            label: "Summary".to_string(),
            lines,
        }],
        raw_output,
    })
}
