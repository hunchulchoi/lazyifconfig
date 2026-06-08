pub fn run_ifconfig() -> Result<String, String> {
    use std::process::Command;
    let output = Command::new("ifconfig")
        .output()
        .map_err(|e| e.to_string())?;

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
        let result = run_ifconfig();
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("lo0") || output.contains("en0"));
    }
}
