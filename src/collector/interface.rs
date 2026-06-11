use std::collections::HashMap;

use crate::model::{
    InterfaceAddress, InterfaceStatus, InterfaceType, NetworkInterface, NetworkKind,
};

pub fn parse_interfaces(input: &str) -> Vec<NetworkInterface> {
    if input.contains("Windows IP Configuration") {
        return parse_windows_ipconfig(input);
    }

    let mut interfaces = Vec::new();
    let mut current: Option<NetworkInterface> = None;

    for line in input.lines() {
        if let Some(name) = parse_interface_header_name(line) {
            if let Some(mut interface) = current.take() {
                interface.network_kind =
                    classify_interface(&interface.name, &interface.ipv4, &interface.ipv6);
                interfaces.push(interface);
            }

            current = Some(NetworkInterface {
                interface_type: infer_interface_type(&name),
                network_kind: NetworkKind::Unknown,
                status: parse_header_status(line),
                mtu: parse_mtu(line),
                name,
                ipv4: Vec::new(),
                ipv6: Vec::new(),
                mac_address: None,
                stats: None,
            });
            continue;
        }

        let Some(interface) = current.as_mut() else {
            continue;
        };

        let trimmed = line.trim();

        if let Some(value) = trimmed.strip_prefix("ether ") {
            interface.mac_address = Some(value.to_string());
        } else if let Some(value) = trimmed.strip_prefix("link/ether ") {
            if let Some(mac) = value.split_whitespace().next() {
                interface.mac_address = Some(mac.to_string());
            }
        } else if let Some(value) = trimmed.strip_prefix("inet6 ") {
            let parts: Vec<&str> = value.split_whitespace().collect();
            if !parts.is_empty() {
                let (address, mut prefix_len) = split_cidr_address(parts[0]);
                let mut gateway = None;

                if let Some(pos) = parts.iter().position(|&p| p == "-->") {
                    if pos + 1 < parts.len() {
                        gateway = Some(parts[pos + 1].to_string());
                    }
                }

                if let Some(pos) = parts.iter().position(|&p| p == "prefixlen") {
                    if pos + 1 < parts.len() {
                        prefix_len = parts[pos + 1].parse::<u8>().ok();
                    }
                }

                interface.ipv6.push(InterfaceAddress {
                    value: address,
                    prefix_len,
                    gateway,
                });
            }
        } else if let Some(value) = trimmed.strip_prefix("inet ") {
            let parts: Vec<&str> = value.split_whitespace().collect();
            if !parts.is_empty() {
                let (address, mut prefix_len) = split_cidr_address(parts[0]);
                let mut gateway = None;

                if let Some(pos) = parts.iter().position(|&p| p == "-->") {
                    if pos + 1 < parts.len() {
                        gateway = Some(parts[pos + 1].to_string());
                    }
                }

                if let Some(pos) = parts.iter().position(|&p| p == "netmask") {
                    if pos + 1 < parts.len() {
                        let hex_mask = parts[pos + 1];
                        prefix_len = parse_hex_netmask(hex_mask);
                    }
                }

                interface.ipv4.push(InterfaceAddress {
                    value: address,
                    prefix_len,
                    gateway,
                });
            }
        } else if trimmed == "status: active" {
            interface.status = InterfaceStatus::Up;
        } else if trimmed == "status: inactive" {
            interface.status = InterfaceStatus::Down;
        }
    }

    if let Some(mut interface) = current {
        interface.network_kind =
            classify_interface(&interface.name, &interface.ipv4, &interface.ipv6);
        interfaces.push(interface);
    }

    interfaces
}

fn parse_windows_ipconfig(input: &str) -> Vec<NetworkInterface> {
    let mut interfaces = Vec::new();
    let mut current: Option<NetworkInterface> = None;
    let mut pending_v4_prefix: Option<usize> = None;

    for line in input.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if !line.starts_with(' ') && trimmed.ends_with(':') && trimmed.contains("adapter") {
            if let Some(interface) = current.take() {
                interfaces.push(finalize_windows_interface(interface));
            }

            let name = trimmed
                .trim_end_matches(':')
                .split_once("adapter")
                .map(|(_, value)| value.trim())
                .filter(|value| !value.is_empty())
                .unwrap_or(trimmed.trim_end_matches(':'))
                .to_string();

            current = Some(NetworkInterface {
                interface_type: infer_interface_type(&name),
                network_kind: NetworkKind::Unknown,
                status: InterfaceStatus::Up,
                mtu: None,
                name,
                ipv4: Vec::new(),
                ipv6: Vec::new(),
                mac_address: None,
                stats: None,
            });
            pending_v4_prefix = None;
            continue;
        }

        let Some(interface) = current.as_mut() else {
            continue;
        };

        if let Some(value) = windows_field_value(trimmed, "Media State") {
            if value.contains("disconnected") {
                interface.status = InterfaceStatus::Down;
            }
        } else if let Some(value) = windows_field_value(trimmed, "Physical Address") {
            interface.mac_address = Some(value.replace('-', ":").to_lowercase());
        } else if let Some(value) = windows_field_value(trimmed, "IPv4 Address") {
            let clean = clean_windows_address(value);
            interface.ipv4.push(InterfaceAddress {
                value: clean,
                prefix_len: None,
                gateway: None,
            });
            pending_v4_prefix = interface.ipv4.len().checked_sub(1);
        } else if let Some(value) = windows_field_value(trimmed, "Subnet Mask") {
            if let (Some(index), Some(prefix)) = (pending_v4_prefix, dotted_mask_prefix(value)) {
                if let Some(addr) = interface.ipv4.get_mut(index) {
                    addr.prefix_len = Some(prefix);
                }
            }
        } else if let Some(value) = windows_field_value(trimmed, "IPv6 Address") {
            interface.ipv6.push(InterfaceAddress {
                value: clean_windows_address(value),
                prefix_len: None,
                gateway: None,
            });
        } else if let Some(value) = windows_field_value(trimmed, "Link-local IPv6 Address") {
            interface.ipv6.push(InterfaceAddress {
                value: clean_windows_address(value),
                prefix_len: Some(64),
                gateway: None,
            });
        } else if let Some(value) = windows_field_value(trimmed, "Default Gateway") {
            let gateway = clean_windows_address(value);
            if gateway.is_empty() {
                continue;
            }
            if gateway.contains(':') {
                for addr in &mut interface.ipv6 {
                    if addr.gateway.is_none() {
                        addr.gateway = Some(gateway.clone());
                    }
                }
            } else {
                for addr in &mut interface.ipv4 {
                    if addr.gateway.is_none() {
                        addr.gateway = Some(gateway.clone());
                    }
                }
            }
        }
    }

    if let Some(interface) = current {
        interfaces.push(finalize_windows_interface(interface));
    }

    interfaces
}

fn finalize_windows_interface(mut interface: NetworkInterface) -> NetworkInterface {
    interface.network_kind = classify_interface(&interface.name, &interface.ipv4, &interface.ipv6);
    interface
}

fn windows_field_value<'a>(line: &'a str, field: &str) -> Option<&'a str> {
    let (left, right) = line.split_once(':')?;
    left.trim()
        .starts_with(field)
        .then_some(right.trim())
        .filter(|value| !value.is_empty())
}

fn clean_windows_address(value: &str) -> String {
    value
        .split('(')
        .next()
        .unwrap_or(value)
        .split('%')
        .next()
        .unwrap_or(value)
        .trim()
        .to_string()
}

fn dotted_mask_prefix(value: &str) -> Option<u8> {
    let mask = value.trim().parse::<std::net::Ipv4Addr>().ok()?;
    Some(u32::from(mask).count_ones() as u8)
}

fn parse_hex_netmask(hex_str: &str) -> Option<u8> {
    let hex_val = hex_str.strip_prefix("0x")?;
    let val = u32::from_str_radix(hex_val, 16).ok()?;
    Some(val.count_ones() as u8)
}

fn split_cidr_address(value: &str) -> (String, Option<u8>) {
    if let Some((address, prefix)) = value.split_once('/') {
        (address.to_string(), prefix.parse::<u8>().ok())
    } else {
        (value.to_string(), None)
    }
}

fn classify_interface(
    name: &str,
    ipv4: &[InterfaceAddress],
    ipv6: &[InterfaceAddress],
) -> NetworkKind {
    // 1. Loopback
    if name == "lo0" || name.starts_with("lo") {
        return NetworkKind::Loopback;
    }
    for addr in ipv4 {
        if let Ok(ip) = addr.value.parse::<std::net::Ipv4Addr>() {
            if ip.is_loopback() {
                return NetworkKind::Loopback;
            }
        }
    }
    for addr in ipv6 {
        if let Ok(ip) = addr.value.parse::<std::net::Ipv6Addr>() {
            if ip.is_loopback() {
                return NetworkKind::Loopback;
            }
        }
    }

    // 2. VPN (utun, tun, tap, wg)
    if name.starts_with("utun")
        || name.starts_with("tun")
        || name.starts_with("tap")
        || name.starts_with("wg")
    {
        return NetworkKind::Vpn;
    }

    // 3. Container (docker, bridge, br-)
    if name.starts_with("docker") || name.starts_with("bridge") || name.starts_with("br-") {
        return NetworkKind::Container;
    }

    // 4. Link Local
    for addr in ipv4 {
        if let Ok(ip) = addr.value.parse::<std::net::Ipv4Addr>() {
            if ip.is_link_local() {
                return NetworkKind::LinkLocal;
            }
        }
    }
    for addr in ipv6 {
        if let Ok(ip) = addr.value.parse::<std::net::Ipv6Addr>() {
            let octets = ip.octets();
            if octets[0] == 0xfe && (octets[1] & 0xc0) == 0x80 {
                return NetworkKind::LinkLocal;
            }
        }
    }

    // 5. Public / LAN
    let mut has_lan = false;
    let mut has_public = false;

    for addr in ipv4 {
        if let Ok(ip) = addr.value.parse::<std::net::Ipv4Addr>() {
            if ip.is_private() {
                has_lan = true;
            } else if !ip.is_loopback() && !ip.is_link_local() {
                has_public = true;
            }
        }
    }

    for addr in ipv6 {
        if let Ok(ip) = addr.value.parse::<std::net::Ipv6Addr>() {
            let octets = ip.octets();
            let is_unique_local = (octets[0] & 0xfe) == 0xfc;
            let is_loopback = ip.is_loopback();
            let is_link_local = octets[0] == 0xfe && (octets[1] & 0xc0) == 0x80;
            let is_multicast = octets[0] == 0xff;
            let is_unspecified = ip.is_unspecified();

            if is_unique_local {
                has_lan = true;
            } else if !is_loopback && !is_link_local && !is_multicast && !is_unspecified {
                has_public = true;
            }
        }
    }

    if has_public {
        return NetworkKind::Public;
    }
    if has_lan {
        return NetworkKind::Lan;
    }

    NetworkKind::Unknown
}

fn parse_interface_header_name(line: &str) -> Option<String> {
    if line.starts_with(' ') || line.starts_with('\t') {
        return None;
    }

    let (first, rest) = line.split_once(':')?;
    if first.chars().all(|c| c.is_ascii_digit()) {
        let (name, _) = rest.trim_start().split_once(':')?;
        return Some(clean_interface_name(name));
    }

    Some(clean_interface_name(first))
}

fn clean_interface_name(name: &str) -> String {
    name.trim()
        .split('@')
        .next()
        .unwrap_or_default()
        .to_string()
}

fn parse_header_status(line: &str) -> InterfaceStatus {
    if line.contains("<UP") || line.contains(",UP,") || line.contains(",UP>") {
        InterfaceStatus::Up
    } else {
        InterfaceStatus::Down
    }
}

fn infer_interface_type(name: &str) -> InterfaceType {
    if name.starts_with("utun") {
        InterfaceType::Vpn
    } else if name == "lo0" || name == "lo" {
        InterfaceType::Loopback
    } else if name.starts_with("bridge") {
        InterfaceType::Bridge
    } else if name.starts_with("awdl") {
        InterfaceType::AirDrop
    } else if name.starts_with("en") || name.starts_with("eth") || name.starts_with("wlan") {
        InterfaceType::WifiOrEthernet
    } else {
        InterfaceType::Unknown
    }
}

fn parse_mtu(line: &str) -> Option<u32> {
    let parts: Vec<&str> = line.split_whitespace().collect();

    parts
        .windows(2)
        .find(|window| window[0] == "mtu")
        .and_then(|window| window[1].parse::<u32>().ok())
}

pub fn parse_default_gateways(netstat_output: &str) -> HashMap<String, String> {
    let mut gateways = HashMap::new();
    let mut parsing_ipv4 = false;
    let mut parsing_ipv6 = false;

    for line in netstat_output.lines() {
        let trimmed = line.trim();
        if let Some((netif, gateway, is_ipv6)) = parse_linux_default_route(trimmed) {
            let family = if is_ipv6 { "v6" } else { "v4" };
            gateways.insert(format!("{}_{}", netif, family), gateway);
            continue;
        }

        if trimmed.starts_with("Internet:") {
            parsing_ipv4 = true;
            parsing_ipv6 = false;
            continue;
        } else if trimmed.starts_with("Internet6:") {
            parsing_ipv4 = false;
            parsing_ipv6 = true;
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 4 {
            let dest = parts[0];
            let gateway = parts[1];
            let flags = parts[2];
            let netif = parts[3];

            if dest == "default" && flags.contains('G') {
                if parsing_ipv4 {
                    if !gateway.contains(':') && !gateway.starts_with("link#") {
                        gateways.insert(format!("{}_v4", netif), gateway.to_string());
                    }
                } else if parsing_ipv6 {
                    if !gateway.starts_with("link#") {
                        let clean_gateway = gateway.split('%').next().unwrap_or(gateway);
                        gateways.insert(format!("{}_v6", netif), clean_gateway.to_string());
                    }
                }
            }
        }
    }
    gateways
}

fn parse_linux_default_route(line: &str) -> Option<(String, String, bool)> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.first().copied() != Some("default") {
        return None;
    }

    let gateway = value_after(&parts, "via")?;
    let interface = value_after(&parts, "dev")?;
    let clean_gateway = gateway.split('%').next().unwrap_or(gateway).to_string();
    let is_ipv6 = clean_gateway.contains(':');

    Some((interface.to_string(), clean_gateway, is_ipv6))
}

fn value_after<'a>(parts: &'a [&str], key: &str) -> Option<&'a str> {
    parts
        .iter()
        .position(|part| *part == key)
        .and_then(|index| parts.get(index + 1).copied())
}

pub fn merge_gateways(interfaces: &mut [NetworkInterface], netstat_output: &str) {
    let default_gateways = parse_default_gateways(netstat_output);
    for interface in interfaces {
        let v4_key = format!("{}_v4", interface.name);
        let v6_key = format!("{}_v6", interface.name);

        for addr in &mut interface.ipv4 {
            if let Some(gw) = default_gateways.get(&v4_key) {
                if addr.gateway.is_none() {
                    addr.gateway = Some(gw.clone());
                }
            }
        }

        for addr in &mut interface.ipv6 {
            if let Some(gw) = default_gateways.get(&v6_key) {
                if addr.gateway.is_none() {
                    addr.gateway = Some(gw.clone());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_windows_ipconfig_output() {
        let input = "\
Windows IP Configuration

Ethernet adapter Ethernet:

   Connection-specific DNS Suffix  . : lan
   Physical Address. . . . . . . . . : AA-BB-CC-DD-EE-FF
   DHCP Enabled. . . . . . . . . . . : Yes
   IPv6 Address. . . . . . . . . . . : 2001:4860::8888(Preferred)
   Link-local IPv6 Address . . . . . : fe80::abcd:1%12(Preferred)
   IPv4 Address. . . . . . . . . . . : 192.168.1.42(Preferred)
   Subnet Mask . . . . . . . . . . . : 255.255.255.0
   Default Gateway . . . . . . . . . : 192.168.1.1

Wireless LAN adapter Wi-Fi:

   Media State . . . . . . . . . . . : Media disconnected
   Physical Address. . . . . . . . . : 11-22-33-44-55-66
";

        let interfaces = parse_interfaces(input);

        assert_eq!(interfaces.len(), 2);
        assert_eq!(interfaces[0].name, "Ethernet");
        assert_eq!(interfaces[0].status, InterfaceStatus::Up);
        assert_eq!(
            interfaces[0].mac_address.as_deref(),
            Some("aa:bb:cc:dd:ee:ff")
        );
        assert_eq!(interfaces[0].ipv4[0].value, "192.168.1.42");
        assert_eq!(interfaces[0].ipv4[0].prefix_len, Some(24));
        assert_eq!(
            interfaces[0].ipv4[0].gateway.as_deref(),
            Some("192.168.1.1")
        );
        assert_eq!(interfaces[0].ipv6[0].value, "2001:4860::8888");
        assert_eq!(interfaces[0].ipv6[1].value, "fe80::abcd:1");
        assert_eq!(interfaces[0].network_kind, NetworkKind::LinkLocal);

        assert_eq!(interfaces[1].name, "Wi-Fi");
        assert_eq!(interfaces[1].status, InterfaceStatus::Down);
        assert_eq!(
            interfaces[1].mac_address.as_deref(),
            Some("11:22:33:44:55:66")
        );
    }
}
