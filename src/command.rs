pub fn run_ifconfig(_show_all: bool) -> Result<String, String> {
    use std::process::Command;
    let mut cmd = Command::new("ifconfig");
    let output = cmd.output().map_err(|e| e.to_string())?;

    if output.status.success() {
        String::from_utf8(output.stdout).map_err(|e| e.to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_ifconfig_success() {
        let result = run_ifconfig(false);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("lo0") || output.contains("en0"));

        let result_all = run_ifconfig(true);
        assert!(result_all.is_ok());
        let output_all = result_all.unwrap();
        assert!(output_all.contains("lo0") || output_all.contains("en0"));
    }
}
