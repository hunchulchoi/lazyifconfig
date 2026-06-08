use std::collections::HashMap;

use crate::model::{
    InterfaceAddress, InterfaceStatus, InterfaceType, NetworkInterface, NetworkKind,
};

pub fn parse_interfaces(input: &str) -> Vec<NetworkInterface> {
    let mut interfaces = Vec::new();
    let mut current: Option<NetworkInterface> = None;

    for line in input.lines() {
        if is_interface_header(line) {
            if let Some(mut interface) = current.take() {
                interface.network_kind = classify_interface(&interface.name, &interface.ipv4, &interface.ipv6);
                interfaces.push(interface);
            }

            let name = line.split(':').next().unwrap_or_default().to_string();
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
        } else if let Some(value) = trimmed.strip_prefix("inet6 ") {
            let parts: Vec<&str> = value.split_whitespace().collect();
            if !parts.is_empty() {
                let address = parts[0].to_string();
                let mut prefix_len = None;
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
                let address = parts[0].to_string();
                let mut prefix_len = None;
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
        interface.network_kind = classify_interface(&interface.name, &interface.ipv4, &interface.ipv6);
        interfaces.push(interface);
    }

    interfaces
}

fn parse_hex_netmask(hex_str: &str) -> Option<u8> {
    let hex_val = hex_str.strip_prefix("0x")?;
    let val = u32::from_str_radix(hex_val, 16).ok()?;
    Some(val.count_ones() as u8)
}

fn classify_interface(name: &str, ipv4: &[InterfaceAddress], ipv6: &[InterfaceAddress]) -> NetworkKind {
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
    if name.starts_with("utun") || name.starts_with("tun") || name.starts_with("tap") || name.starts_with("wg") {
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

fn is_interface_header(line: &str) -> bool {
    !line.starts_with(' ') && !line.starts_with('\t') && line.contains(':')
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
    } else if name == "lo0" {
        InterfaceType::Loopback
    } else if name.starts_with("bridge") {
        InterfaceType::Bridge
    } else if name.starts_with("awdl") {
        InterfaceType::AirDrop
    } else if name.starts_with("en") {
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
