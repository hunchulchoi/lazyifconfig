use crate::model::ListeningPort;

pub fn parse_listening_ports(input: &str) -> Vec<ListeningPort> {
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

fn parse_ss_listening_ports(input: &str) -> Vec<ListeningPort> {
    let mut ports = Vec::new();

    for line in input.lines() {
        if !is_ss_socket_row(line) {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 5 {
            continue;
        }

        let proto = parts[0].to_lowercase();
        let (local_ip, local_port) = split_node_name(parts[4]);
        let process = parts.get(6).copied().unwrap_or_default();
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
    let mut parts = line.split_whitespace();
    let Some(proto) = parts.next() else {
        return false;
    };
    matches!(proto, "tcp" | "tcp6" | "udp" | "udp6") && parts.next().is_some()
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
}
