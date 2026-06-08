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
}

impl InterfaceAddress {
    pub fn new(value: &str) -> Self {
        Self {
            value: value.to_string(),
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
