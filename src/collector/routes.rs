use crate::model::RouteEntry;

pub fn parse_routes(netstat_output: &str) -> Vec<RouteEntry> {
    if netstat_output.lines().any(is_linux_ip_route_line) {
        return parse_linux_ip_routes(netstat_output);
    }

    let mut routes = Vec::new();
    let mut parsing_ipv4 = false;

    for line in netstat_output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Routing tables") {
            continue;
        }
        if trimmed.starts_with("Internet:") {
            parsing_ipv4 = true;
            continue;
        } else if trimmed.starts_with("Internet6:") {
            parsing_ipv4 = false;
            continue;
        }

        if parsing_ipv4 {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                let destination = parts[0];
                let gateway = parts[1];
                let _flags = parts[2];
                let interface = parts[3];

                // Skip headers
                if destination == "Destination" {
                    continue;
                }

                routes.push(RouteEntry {
                    destination: destination.to_string(),
                    gateway: gateway.to_string(),
                    interface: interface.to_string(),
                });
            }
        }
    }
    routes
}

fn parse_linux_ip_routes(input: &str) -> Vec<RouteEntry> {
    let mut routes = Vec::new();

    for line in input.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        let destination = parts[0];
        let Some(interface) = value_after(&parts, "dev") else {
            continue;
        };
        let gateway = value_after(&parts, "via").unwrap_or("link");

        routes.push(RouteEntry {
            destination: destination.to_string(),
            gateway: gateway.to_string(),
            interface: interface.to_string(),
        });
    }

    routes
}

fn is_linux_ip_route_line(line: &str) -> bool {
    let parts: Vec<&str> = line.split_whitespace().collect();
    value_after(&parts, "dev").is_some()
}

fn value_after<'a>(parts: &'a [&str], key: &str) -> Option<&'a str> {
    parts
        .iter()
        .position(|part| *part == key)
        .and_then(|index| parts.get(index + 1).copied())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_routes() {
        let sample = "\
Routing tables

Internet:
Destination        Gateway            Flags               Netif Expire
default            192.168.0.1        UGScg                 en0
127.0.0.1          127.0.0.1          UH                    lo0
192.168.0.0/24     link#18            UCS                   en0

Internet6:
Destination        Gateway            Flags         Netif Expire
::1                ::1                UHL            lo0
";
        let routes = parse_routes(sample);
        assert_eq!(routes.len(), 3);
        assert_eq!(routes[0].destination, "default");
        assert_eq!(routes[0].gateway, "192.168.0.1");
        assert_eq!(routes[0].interface, "en0");

        assert_eq!(routes[1].destination, "127.0.0.1");
        assert_eq!(routes[1].gateway, "127.0.0.1");
        assert_eq!(routes[1].interface, "lo0");

        assert_eq!(routes[2].destination, "192.168.0.0/24");
        assert_eq!(routes[2].gateway, "link#18");
        assert_eq!(routes[2].interface, "en0");
    }

    #[test]
    fn test_parse_linux_ip_routes() {
        let sample = "\
default via 172.17.0.1 dev eth0 proto static
172.17.0.0/16 dev eth0 proto kernel scope link src 172.17.0.2
10.8.0.0/24 via 10.8.0.1 dev tun0
";
        let routes = parse_routes(sample);

        assert_eq!(routes.len(), 3);
        assert_eq!(routes[0].destination, "default");
        assert_eq!(routes[0].gateway, "172.17.0.1");
        assert_eq!(routes[0].interface, "eth0");

        assert_eq!(routes[1].destination, "172.17.0.0/16");
        assert_eq!(routes[1].gateway, "link");
        assert_eq!(routes[1].interface, "eth0");

        assert_eq!(routes[2].destination, "10.8.0.0/24");
        assert_eq!(routes[2].gateway, "10.8.0.1");
        assert_eq!(routes[2].interface, "tun0");
    }
}
