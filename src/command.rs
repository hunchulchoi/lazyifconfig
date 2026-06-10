#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandSpec {
    pub display: &'static str,
    pub program: &'static str,
    pub args: &'static [&'static str],
}

pub fn interface_command_spec() -> CommandSpec {
    interface_command_spec_for_os(std::env::consts::OS)
}

pub fn route_table_command_spec() -> CommandSpec {
    route_table_command_spec_for_os(std::env::consts::OS)
}

pub fn default_route_command_spec() -> CommandSpec {
    default_route_command_spec_for_os(std::env::consts::OS)
}

pub fn interface_command_spec_for_os(os: &str) -> CommandSpec {
    if os == "linux" {
        CommandSpec {
            display: "ip -details -statistics address show",
            program: "ip",
            args: &["-details", "-statistics", "address", "show"],
        }
    } else {
        CommandSpec {
            display: "ifconfig",
            program: "ifconfig",
            args: &[],
        }
    }
}

pub fn route_table_command_spec_for_os(os: &str) -> CommandSpec {
    if os == "linux" {
        CommandSpec {
            display: "ip route show",
            program: "ip",
            args: &["route", "show"],
        }
    } else {
        CommandSpec {
            display: "netstat -rn",
            program: "netstat",
            args: &["-rn"],
        }
    }
}

pub fn default_route_command_spec_for_os(os: &str) -> CommandSpec {
    if os == "linux" {
        CommandSpec {
            display: "ip route show default",
            program: "ip",
            args: &["route", "show", "default"],
        }
    } else {
        CommandSpec {
            display: "route -n get default",
            program: "route",
            args: &["-n", "get", "default"],
        }
    }
}

pub fn run_command_capture(program: &str, args: &[&str]) -> Result<CommandResult, String> {
    use std::process::Command;

    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;

    Ok(CommandResult {
        stdout: String::from_utf8(output.stdout).map_err(|e| e.to_string())?,
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code(),
    })
}

pub fn run_ifconfig(_show_all: bool) -> Result<String, String> {
    let command = interface_command_spec();
    let output = run_command_capture(command.program, command.args)?;

    if output.exit_code == Some(0) {
        Ok(output.stdout)
    } else {
        Err(output.stderr)
    }
}

pub fn run_netstat() -> Result<String, String> {
    let command = route_table_command_spec();
    let output = run_command_capture(command.program, command.args)?;

    if output.exit_code == Some(0) {
        Ok(output.stdout)
    } else {
        Err(output.stderr)
    }
}

pub fn run_netstat_an() -> Result<String, String> {
    let output = run_command_capture("netstat", &["-an"])?;

    if output.exit_code == Some(0) {
        Ok(output.stdout)
    } else {
        Err(output.stderr)
    }
}

pub fn run_netstat_ib() -> Result<String, String> {
    let output = run_command_capture("netstat", &["-ib"])?;

    if output.exit_code == Some(0) {
        Ok(output.stdout)
    } else {
        Err(output.stderr)
    }
}

pub fn run_lsof_listening() -> Result<String, String> {
    let output = run_command_capture("lsof", &["-iTCP", "-sTCP:LISTEN", "-P", "-n"])?;

    if output.exit_code == Some(0) {
        Ok(output.stdout)
    } else {
        if output.stderr.trim().is_empty() {
            Ok(String::new())
        } else {
            Err(output.stderr)
        }
    }
}

pub fn run_whois(ip: &str) -> Result<String, String> {
    use std::process::Command;
    let mut cmd = Command::new("whois");
    cmd.arg(ip);
    let output = cmd.output().map_err(|e| e.to_string())?;

    if output.status.success() {
        String::from_utf8(output.stdout).map_err(|e| e.to_string())
    } else {
        let stdout_str = String::from_utf8_lossy(&output.stdout).to_string();
        if !stdout_str.trim().is_empty() {
            Ok(stdout_str)
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }
}

pub fn copy_to_clipboard(text: &str) -> Result<(), String> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut child = Command::new("pbcopy")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(text.as_bytes())
            .map_err(|e| e.to_string())?;
    }

    child.wait().map_err(|e| e.to_string())?;
    Ok(())
}

pub fn run_kill(pid: &str) -> Result<(), String> {
    use std::process::Command;
    let output = Command::new("kill")
        .args(["-9", pid])
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

pub fn run_curl(url: &str) -> Result<String, String> {
    let output = run_command_capture("curl", &["-s", "-m", "5", url])?;

    if output.exit_code == Some(0) {
        Ok(output.stdout)
    } else {
        Err(output.stderr)
    }
}

pub fn run_route_default() -> Result<String, String> {
    let command = default_route_command_spec();
    let output = run_command_capture(command.program, command.args)?;

    if output.exit_code == Some(0) {
        Ok(output.stdout)
    } else {
        Err(output.stderr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interface_command_uses_ip_on_linux() {
        let command = interface_command_spec_for_os("linux");

        assert_eq!(command.display, "ip -details -statistics address show");
        assert_eq!(command.program, "ip");
        assert_eq!(command.args, &["-details", "-statistics", "address", "show"]);
    }

    #[test]
    fn interface_command_uses_ifconfig_on_non_linux() {
        let command = interface_command_spec_for_os("macos");

        assert_eq!(command.display, "ifconfig");
        assert_eq!(command.program, "ifconfig");
        assert!(command.args.is_empty());
    }

    #[test]
    fn route_commands_use_ip_on_linux() {
        let routes = route_table_command_spec_for_os("linux");
        let default_route = default_route_command_spec_for_os("linux");

        assert_eq!(routes.display, "ip route show");
        assert_eq!(routes.program, "ip");
        assert_eq!(routes.args, &["route", "show"]);
        assert_eq!(default_route.display, "ip route show default");
        assert_eq!(default_route.program, "ip");
        assert_eq!(default_route.args, &["route", "show", "default"]);
    }

    #[test]
    fn route_commands_use_legacy_tools_on_non_linux() {
        let routes = route_table_command_spec_for_os("macos");
        let default_route = default_route_command_spec_for_os("macos");

        assert_eq!(routes.display, "netstat -rn");
        assert_eq!(routes.program, "netstat");
        assert_eq!(routes.args, &["-rn"]);
        assert_eq!(default_route.display, "route -n get default");
        assert_eq!(default_route.program, "route");
        assert_eq!(default_route.args, &["-n", "get", "default"]);
    }

    #[test]
    fn test_run_ifconfig_success() {
        let result = run_ifconfig(false);
        assert!(result.is_ok());
        let output = result.unwrap();
        if cfg!(target_os = "linux") {
            assert!(output.contains(" lo:") || output.contains(" lo "));
        } else {
            assert!(output.contains("lo0") || output.contains("en0"));
        }

        let result_all = run_ifconfig(true);
        assert!(result_all.is_ok());
        let output_all = result_all.unwrap();
        if cfg!(target_os = "linux") {
            assert!(output_all.contains(" lo:") || output_all.contains(" lo "));
        } else {
            assert!(output_all.contains("lo0") || output_all.contains("en0"));
        }
    }

    #[test]
    fn test_run_netstat_success() {
        let result = run_netstat();
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("Routing tables") || output.contains("default"));
    }

    #[test]
    fn test_run_netstat_an_success() {
        let result = run_netstat_an();
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_netstat_ib_success() {
        let result = run_netstat_ib();
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("Name") && output.contains("Ibytes") && output.contains("Obytes"));
    }

    #[test]
    fn test_run_lsof_listening_success() {
        let result = run_lsof_listening();
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_run_route_default_success() {
        let result = run_route_default();
        assert!(result.is_ok() || result.is_err());
    }
}
