use crate::model::{
    InterfaceAddress, InterfaceStatus, InterfaceType, NetworkInterface,
};

pub fn parse_interfaces(input: &str) -> Vec<NetworkInterface> {
    let mut interfaces = Vec::new();
    let mut current: Option<NetworkInterface> = None;

    for line in input.lines() {
        if is_interface_header(line) {
            if let Some(interface) = current.take() {
                interfaces.push(interface);
            }

            let name = line.split(':').next().unwrap_or_default().to_string();
            current = Some(NetworkInterface {
                interface_type: infer_interface_type(&name),
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
            let address = value.split_whitespace().next().unwrap_or_default();
            interface.ipv6.push(InterfaceAddress::new(address));
        } else if let Some(value) = trimmed.strip_prefix("inet ") {
            let address = value.split_whitespace().next().unwrap_or_default();
            interface.ipv4.push(InterfaceAddress::new(address));
        } else if trimmed == "status: active" {
            interface.status = InterfaceStatus::Up;
        } else if trimmed == "status: inactive" {
            interface.status = InterfaceStatus::Down;
        }
    }

    if let Some(interface) = current {
        interfaces.push(interface);
    }

    interfaces
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
