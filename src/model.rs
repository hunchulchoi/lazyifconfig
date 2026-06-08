#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkKind {
    Loopback,
    Lan,
    Vpn,
    Container,
    LinkLocal,
    Public,
    Unknown,
}

impl NetworkKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            NetworkKind::Loopback => "LOOPBACK",
            NetworkKind::Lan => "LAN",
            NetworkKind::Vpn => "VPN",
            NetworkKind::Container => "CONTAINER",
            NetworkKind::LinkLocal => "LINK LOCAL",
            NetworkKind::Public => "PUBLIC",
            NetworkKind::Unknown => "UNKNOWN",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InterfaceType {
    Vpn,
    Loopback,
    Bridge,
    AirDrop,
    WifiOrEthernet,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InterfaceStatus {
    Up,
    Down,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InterfaceAddress {
    pub value: String,
    pub prefix_len: Option<u8>,
    pub gateway: Option<String>,
}

impl InterfaceAddress {
    pub fn new(value: &str) -> Self {
        Self {
            value: value.to_string(),
            prefix_len: None,
            gateway: None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InterfaceStats {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_packets: u64,
    pub tx_packets: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkInterface {
    pub name: String,
    pub network_kind: NetworkKind,
    pub interface_type: InterfaceType,
    pub status: InterfaceStatus,
    pub ipv4: Vec<InterfaceAddress>,
    pub ipv6: Vec<InterfaceAddress>,
    pub mac_address: Option<String>,
    pub mtu: Option<u32>,
    pub stats: Option<InterfaceStats>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NetworkSnapshot {
    pub interfaces: Vec<NetworkInterface>,
    pub captured_at_secs: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkEvent {
    pub message: String,
    pub captured_at_secs: u64,
}

impl NetworkEvent {
    pub fn new(message: String, captured_at_secs: u64) -> Self {
        Self {
            message,
            captured_at_secs,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Subnet {
    Ipv4 {
        network: std::net::Ipv4Addr,
        prefix_len: u8,
    },
    Ipv6 {
        network: std::net::Ipv6Addr,
        prefix_len: u8,
    },
    Unassigned,
}

impl Ord for Subnet {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        match (self, other) {
            (Subnet::Ipv4 { network: n1, prefix_len: p1 }, Subnet::Ipv4 { network: n2, prefix_len: p2 }) => {
                n1.cmp(n2).then(p1.cmp(p2))
            }
            (Subnet::Ipv6 { network: n1, prefix_len: p1 }, Subnet::Ipv6 { network: n2, prefix_len: p2 }) => {
                n1.cmp(n2).then(p1.cmp(p2))
            }
            (Subnet::Unassigned, Subnet::Unassigned) => Ordering::Equal,
            (Subnet::Ipv4 { .. }, _) => Ordering::Less,
            (_, Subnet::Ipv4 { .. }) => Ordering::Greater,
            (Subnet::Ipv6 { .. }, _) => Ordering::Less,
            (_, Subnet::Ipv6 { .. }) => Ordering::Greater,
        }
    }
}

impl PartialOrd for Subnet {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
