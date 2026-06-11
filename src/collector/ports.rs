use crate::model::ListeningPort;

pub fn parse_listening_ports(input: &str) -> Vec<ListeningPort> {
    if looks_like_windows_netstat(input) {
        return parse_windows_netstat_ports(input);
    }

    if input.lines().any(is_ss_socket_row) {
        return parse_ss_listening_ports(input);
    }

    let mut ports = Vec::new();
    let lines: Vec<&str> = input.lines().collect();
    if lines.is_empty() {
        return ports;
    }

    // Skip header line
    for line in lines.iter().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 9 {
            continue;
        }

        let command = parts[0].to_string();
        let pid = parts[1].to_string();
        let user = parts[2].to_string();
        let proto = parts[7].to_lowercase();

        // The Node Name (parts[8]) usually looks like `*:56642` or `127.0.0.1:80`
        // It might end with ` (LISTEN)` or `(LISTEN)`
        let name_part = parts[8];
        let name_clean = name_part
            .trim_end_matches(" (LISTEN)")
            .trim_end_matches("(LISTEN)")
            .trim();

        let (local_ip, local_port) = split_node_name(name_clean);

        ports.push(ListeningPort {
            proto,
            local_ip,
            local_port,
            pid,
            command,
            user,
        });
    }

    ports
}

fn looks_like_windows_netstat(input: &str) -> bool {
    input.contains("Proto") && input.contains("PID")
}

fn parse_windows_netstat_ports(input: &str) -> Vec<ListeningPort> {
    let mut ports = Vec::new();

    for line in input.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 5 || !parts[0].eq_ignore_ascii_case("TCP") {
            continue;
        }
        if !parts[3].eq_ignore_ascii_case("LISTENING") {
            continue;
        }

        let (local_ip, local_port) = split_node_name(parts[1]);
        let pid = parts[4].to_string();
        ports.push(ListeningPort {
            proto: "tcp".to_string(),
            local_ip,
            local_port,
            pid: pid.clone(),
            command: format!("pid:{pid}"),
            user: "-".to_string(),
        });
    }

    ports
}

fn parse_ss_listening_ports(input: &str) -> Vec<ListeningPort> {
    let mut ports = Vec::new();

    for line in input.lines() {
        let Some((proto, local_address, process)) = parse_ss_row_parts(line) else {
            continue;
        };

        let (local_ip, local_port) = split_node_name(local_address);
        let command = parse_ss_command(process).unwrap_or_else(|| "-".to_string());
        let pid = parse_ss_pid(process).unwrap_or_else(|| "-".to_string());

        ports.push(ListeningPort {
            proto,
            local_ip,
            local_port,
            pid,
            command,
            user: "-".to_string(),
        });
    }

    ports
}

fn is_ss_socket_row(line: &str) -> bool {
    parse_ss_row_parts(line).is_some()
}

fn parse_ss_row_parts(line: &str) -> Option<(String, &str, &str)> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    let first = parts.first().copied()?;

    if matches!(first, "tcp" | "tcp6" | "udp" | "udp6") {
        if parts.len() < 5 {
            return None;
        }
        return Some((
            first.to_lowercase(),
            parts[4],
            parts.get(6).copied().unwrap_or_default(),
        ));
    }

    if first == "LISTEN" {
        if parts.len() < 4 {
            return None;
        }
        return Some((
            "tcp".to_string(),
            parts[3],
            parts.get(5).copied().unwrap_or_default(),
        ));
    }

    None
}

fn parse_ss_command(process: &str) -> Option<String> {
    let start = process.find('"')?;
    let rest = &process[start + 1..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn parse_ss_pid(process: &str) -> Option<String> {
    let start = process.find("pid=")? + "pid=".len();
    let pid: String = process[start..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if pid.is_empty() {
        None
    } else {
        Some(pid)
    }
}

fn split_node_name(node: &str) -> (String, String) {
    if let Some(pos) = node.rfind(':') {
        let ip = node[..pos]
            .trim_start_matches('[')
            .trim_end_matches(']')
            .to_string();
        let port = node[pos + 1..].to_string();
        (ip, port)
    } else {
        (node.to_string(), "*".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lsof_listening_tcp_rows() {
        let input = "\
COMMAND   PID USER   FD   TYPE             DEVICE SIZE/OFF NODE NAME
node    12345 user   21u  IPv6 0x123456789abcdef0      0t0  TCP *:3000 (LISTEN)
Python  23456 user    5u  IPv4 0xabcdef0123456789      0t0  TCP 127.0.0.1:8000 (LISTEN)
";

        let ports = parse_listening_ports(input);

        assert_eq!(ports.len(), 2);
        assert_eq!(ports[0].proto, "tcp");
        assert_eq!(ports[0].local_ip, "*");
        assert_eq!(ports[0].local_port, "3000");
        assert_eq!(ports[0].pid, "12345");
        assert_eq!(ports[0].command, "node");
        assert_eq!(ports[0].user, "user");

        assert_eq!(ports[1].proto, "tcp");
        assert_eq!(ports[1].local_ip, "127.0.0.1");
        assert_eq!(ports[1].local_port, "8000");
    }

    #[test]
    fn parses_ss_listening_tcp_rows() {
        let input = "\
tcp LISTEN 0 4096 127.0.0.53%lo:53 0.0.0.0:* users:((\"systemd-resolve\",pid=579,fd=14))
tcp LISTEN 0 128 0.0.0.0:22 0.0.0.0:* users:((\"sshd\",pid=123,fd=3))
tcp LISTEN 0 511 [::]:8080 [::]:* users:((\"nginx\",pid=987,fd=6))
";

        let ports = parse_listening_ports(input);

        assert_eq!(ports.len(), 3);
        assert_eq!(ports[0].proto, "tcp");
        assert_eq!(ports[0].local_ip, "127.0.0.53%lo");
        assert_eq!(ports[0].local_port, "53");
        assert_eq!(ports[0].command, "systemd-resolve");
        assert_eq!(ports[0].pid, "579");
        assert_eq!(ports[0].user, "-");

        assert_eq!(ports[1].local_ip, "0.0.0.0");
        assert_eq!(ports[1].local_port, "22");
        assert_eq!(ports[1].command, "sshd");
        assert_eq!(ports[1].pid, "123");

        assert_eq!(ports[2].local_ip, "::");
        assert_eq!(ports[2].local_port, "8080");
        assert_eq!(ports[2].command, "nginx");
        assert_eq!(ports[2].pid, "987");
    }

    #[test]
    fn parses_headerless_ss_ltnp_rows() {
        let input = "\
LISTEN 0 4096 127.0.0.53%lo:53 0.0.0.0:* users:((\"systemd-resolve\",pid=579,fd=14))
LISTEN 0 128 0.0.0.0:22 0.0.0.0:* users:((\"sshd\",pid=123,fd=3))
LISTEN 0 511 [::]:8080 [::]:* users:((\"nginx\",pid=987,fd=6))
";

        let ports = parse_listening_ports(input);

        assert_eq!(ports.len(), 3);
        assert_eq!(ports[0].proto, "tcp");
        assert_eq!(ports[0].local_ip, "127.0.0.53%lo");
        assert_eq!(ports[0].local_port, "53");
        assert_eq!(ports[0].command, "systemd-resolve");
        assert_eq!(ports[0].pid, "579");

        assert_eq!(ports[1].local_ip, "0.0.0.0");
        assert_eq!(ports[1].local_port, "22");
        assert_eq!(ports[1].command, "sshd");
        assert_eq!(ports[1].pid, "123");

        assert_eq!(ports[2].local_ip, "::");
        assert_eq!(ports[2].local_port, "8080");
        assert_eq!(ports[2].command, "nginx");
        assert_eq!(ports[2].pid, "987");
    }

    #[test]
    fn parses_windows_netstat_listening_tcp_rows() {
        let input = "\
Active Connections

  Proto  Local Address          Foreign Address        State           PID
  TCP    0.0.0.0:135            0.0.0.0:0              LISTENING       980
  TCP    127.0.0.1:3000         0.0.0.0:0              LISTENING       12345
  TCP    192.168.1.42:50000     93.184.216.34:443      ESTABLISHED     2222
  TCP    [::]:8080              [::]:0                 LISTENING       777
";

        let ports = parse_listening_ports(input);

        assert_eq!(ports.len(), 3);
        assert_eq!(ports[0].proto, "tcp");
        assert_eq!(ports[0].local_ip, "0.0.0.0");
        assert_eq!(ports[0].local_port, "135");
        assert_eq!(ports[0].pid, "980");
        assert_eq!(ports[0].command, "pid:980");
        assert_eq!(ports[0].user, "-");

        assert_eq!(ports[1].local_ip, "127.0.0.1");
        assert_eq!(ports[1].local_port, "3000");
        assert_eq!(ports[2].local_ip, "::");
        assert_eq!(ports[2].local_port, "8080");
    }
}
