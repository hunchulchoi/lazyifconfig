use crate::model::{RouteEntry, RouteFamily, RoutePathResult};

pub fn parse_routes(netstat_output: &str) -> Vec<RouteEntry> {
    if looks_like_linux_route_get_output(netstat_output) {
        return Vec::new();
    }

    if netstat_output.lines().any(is_linux_ip_route_line) {
        return parse_linux_ip_routes(netstat_output);
    }
    if looks_like_windows_route_print(netstat_output) {
        return parse_windows_route_print(netstat_output);
    }

    let mut routes = Vec::new();
    let mut family = RouteFamily::Unknown;

    for line in netstat_output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Routing tables") {
            continue;
        }
        if trimmed.starts_with("Internet:") {
            family = RouteFamily::Ipv4;
            continue;
        } else if trimmed.starts_with("Internet6:") {
            family = RouteFamily::Ipv6;
            continue;
        }

        if family != RouteFamily::Unknown {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                let destination = parts[0];
                let gateway = parts[1];
                let flags = parts[2];
                let interface = parts[3];

                // Skip headers
                if destination == "Destination" {
                    continue;
                }

                let mut route = RouteEntry::new(destination, gateway, interface);
                route.flags = Some(flags.to_string());
                route.family = family;
                routes.push(route);
            }
        }
    }
    routes
}

fn looks_like_windows_route_print(input: &str) -> bool {
    input.contains("IPv4 Route Table") || input.contains("Active Routes:")
}

fn parse_windows_route_print(input: &str) -> Vec<RouteEntry> {
    let mut routes = Vec::new();
    let mut family = RouteFamily::Unknown;
    let mut in_active_routes = false;
    let mut pending_ipv6: Option<(String, Option<u32>, String)> = None;

    for line in input.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("IPv4 Route Table") {
            family = RouteFamily::Ipv4;
            in_active_routes = false;
            pending_ipv6 = None;
            continue;
        }
        if trimmed.starts_with("IPv6 Route Table") {
            family = RouteFamily::Ipv6;
            in_active_routes = false;
            pending_ipv6 = None;
            continue;
        }
        if trimmed.starts_with("Active Routes:") {
            in_active_routes = true;
            continue;
        }
        if trimmed.starts_with("Persistent Routes:") {
            in_active_routes = false;
            continue;
        }
        if !in_active_routes
            || trimmed.is_empty()
            || trimmed.starts_with("Network")
            || trimmed.starts_with("If ")
            || trimmed.starts_with('=')
        {
            continue;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if let (RouteFamily::Ipv6, Some((interface, metric, destination))) =
            (&family, pending_ipv6.take())
        {
            if parts.len() == 1 {
                let mut route = RouteEntry::new(destination, parts[0].to_string(), interface);
                route.metric = metric;
                route.family = RouteFamily::Ipv6;
                routes.push(route);
                continue;
            }
        }

        match family {
            RouteFamily::Ipv4 if parts.len() >= 5 => {
                let destination = if parts[0] == "0.0.0.0" && parts[1] == "0.0.0.0" {
                    "default".to_string()
                } else {
                    format!(
                        "{}/{}",
                        parts[0],
                        ipv4_mask_to_prefix(parts[1]).unwrap_or(32)
                    )
                };
                let gateway = parts[2].to_string();
                let interface = parts[3].to_string();
                let mut route = RouteEntry::new(destination, gateway, interface);
                route.metric = parts[4].parse().ok();
                route.family = RouteFamily::Ipv4;
                routes.push(route);
            }
            RouteFamily::Ipv6 if parts.len() >= 4 => {
                let destination = if parts[2] == "::/0" {
                    "default".to_string()
                } else {
                    parts[2].to_string()
                };
                let gateway = parts.get(3).copied().unwrap_or("link").to_string();
                let interface = parts[0].to_string();
                let mut route = RouteEntry::new(destination, gateway, interface);
                route.metric = parts[1].parse().ok();
                route.family = RouteFamily::Ipv6;
                routes.push(route);
            }
            RouteFamily::Ipv6 if parts.len() == 3 => {
                let destination = if parts[2] == "::/0" {
                    "default".to_string()
                } else {
                    parts[2].to_string()
                };
                pending_ipv6 = Some((parts[0].to_string(), parts[1].parse().ok(), destination));
            }
            _ => {}
        }
    }

    routes
}

fn ipv4_mask_to_prefix(mask: &str) -> Option<u8> {
    let mask = mask.parse::<std::net::Ipv4Addr>().ok()?;
    Some(u32::from(mask).count_ones() as u8)
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

        let mut route = RouteEntry::new(destination, gateway, interface);
        route.protocol = value_after(&parts, "proto").map(str::to_string);
        route.metric = value_after(&parts, "metric").and_then(|value| value.parse().ok());
        route.family = infer_linux_route_family(destination, gateway);
        routes.push(route);
    }

    routes
}

pub fn parse_linux_route_path(destination: &str, output: &str) -> Result<RoutePathResult, String> {
    let first_line = output.lines().next().unwrap_or("").trim();
    if first_line.is_empty() {
        return Err("route path output is empty".to_string());
    }
    let parts: Vec<&str> = first_line.split_whitespace().collect();
    let resolved_destination = parts.first().map(|value| (*value).to_string());
    let gateway = value_after(&parts, "via").map(str::to_string);
    let interface = value_after(&parts, "dev").map(str::to_string);
    if resolved_destination.is_none() || interface.is_none() {
        return Err("route path output is missing required fields".to_string());
    }
    let source_ip = value_after(&parts, "src").map(str::to_string);
    Ok(RoutePathResult {
        destination: destination.to_string(),
        resolved_destination,
        source_ip,
        interface,
        gateway,
        is_vpn: false,
        raw_output: output.to_string(),
    })
}

pub fn parse_macos_route_path(destination: &str, output: &str) -> Result<RoutePathResult, String> {
    if output.trim().is_empty() {
        return Err("route path output is empty".to_string());
    }
    let mut resolved_destination = None;
    let mut gateway = None;
    let mut interface = None;
    for line in output.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("route to:") {
            resolved_destination = Some(value.trim().to_string());
        } else if let Some(value) = trimmed.strip_prefix("gateway:") {
            gateway = Some(value.trim().to_string());
        } else if let Some(value) = trimmed.strip_prefix("interface:") {
            interface = Some(value.trim().to_string());
        }
    }
    if resolved_destination.is_none() || interface.is_none() {
        return Err("route path output is missing required fields".to_string());
    }
    Ok(RoutePathResult {
        destination: destination.to_string(),
        resolved_destination,
        source_ip: None,
        interface,
        gateway,
        is_vpn: false,
        raw_output: output.to_string(),
    })
}

fn is_linux_ip_route_line(line: &str) -> bool {
    let parts: Vec<&str> = line.split_whitespace().collect();
    value_after(&parts, "dev").is_some()
}

fn looks_like_linux_route_get_output(input: &str) -> bool {
    input.lines().any(|line| {
        let trimmed = line.trim();
        trimmed == "cache" || trimmed.split_whitespace().any(|part| part == "uid")
    })
}

fn infer_linux_route_family(destination: &str, gateway: &str) -> RouteFamily {
    if destination.contains(':') || gateway.contains(':') {
        RouteFamily::Ipv6
    } else {
        RouteFamily::Ipv4
    }
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
        assert_eq!(routes.len(), 4);
        assert_eq!(routes[0].destination, "default");
        assert_eq!(routes[0].gateway, "192.168.0.1");
        assert_eq!(routes[0].interface, "en0");
        assert_eq!(routes[0].flags.as_deref(), Some("UGScg"));
        assert_eq!(routes[0].family, RouteFamily::Ipv4);

        assert_eq!(routes[1].destination, "127.0.0.1");
        assert_eq!(routes[1].gateway, "127.0.0.1");
        assert_eq!(routes[1].interface, "lo0");

        assert_eq!(routes[2].destination, "192.168.0.0/24");
        assert_eq!(routes[2].gateway, "link#18");
        assert_eq!(routes[2].interface, "en0");

        assert_eq!(routes[3].destination, "::1");
        assert_eq!(routes[3].gateway, "::1");
        assert_eq!(routes[3].interface, "lo0");
        assert_eq!(routes[3].flags.as_deref(), Some("UHL"));
        assert_eq!(routes[3].family, RouteFamily::Ipv6);
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
        assert_eq!(routes[0].protocol.as_deref(), Some("static"));
        assert_eq!(routes[0].family, RouteFamily::Ipv4);

        assert_eq!(routes[1].destination, "172.17.0.0/16");
        assert_eq!(routes[1].gateway, "link");
        assert_eq!(routes[1].interface, "eth0");
        assert_eq!(routes[1].protocol.as_deref(), Some("kernel"));

        assert_eq!(routes[2].destination, "10.8.0.0/24");
        assert_eq!(routes[2].gateway, "10.8.0.1");
        assert_eq!(routes[2].interface, "tun0");
    }

    #[test]
    fn test_parse_windows_route_print() {
        let sample = "\
===========================================================================
Interface List
 12...aa bb cc dd ee ff ......Intel(R) Ethernet
===========================================================================

IPv4 Route Table
===========================================================================
Active Routes:
Network Destination        Netmask          Gateway       Interface  Metric
          0.0.0.0          0.0.0.0      192.168.1.1    192.168.1.42     25
        127.0.0.0        255.0.0.0         On-link       127.0.0.1    331
      192.168.1.0    255.255.255.0         On-link     192.168.1.42    281
===========================================================================
IPv6 Route Table
===========================================================================
Active Routes:
 If Metric Network Destination      Gateway
 12    281 ::/0                     fe80::1
 12    281 2001:4860::8888/128
                                    On-link
  1    331 ::1/128                  On-link
";

        let routes = parse_routes(sample);

        assert_eq!(routes.len(), 6);
        assert_eq!(routes[0].destination, "default");
        assert_eq!(routes[0].gateway, "192.168.1.1");
        assert_eq!(routes[0].interface, "192.168.1.42");
        assert_eq!(routes[0].metric, Some(25));
        assert_eq!(routes[0].family, RouteFamily::Ipv4);

        assert_eq!(routes[2].destination, "192.168.1.0/24");
        assert_eq!(routes[2].gateway, "On-link");

        assert_eq!(routes[3].destination, "default");
        assert_eq!(routes[3].gateway, "fe80::1");
        assert_eq!(routes[3].interface, "12");
        assert_eq!(routes[3].metric, Some(281));
        assert_eq!(routes[3].family, RouteFamily::Ipv6);

        assert_eq!(routes[4].destination, "2001:4860::8888/128");
        assert_eq!(routes[4].gateway, "On-link");
        assert_eq!(routes[4].interface, "12");
    }
}
