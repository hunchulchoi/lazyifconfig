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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveConnection {
    pub proto: String,
    pub local_ip: String,
    pub local_port: String,
    pub foreign_ip: String,
    pub foreign_port: String,
    pub state: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListeningPort {
    pub proto: String,
    pub local_ip: String,
    pub local_port: String,
    pub pid: String,
    pub command: String,
    pub user: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteEntry {
    pub destination: String,
    pub gateway: String,
    pub interface: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicIpInfo {
    pub ip: String,
    pub provider: Option<String>,
    pub country: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NetworkSnapshot {
    pub interfaces: Vec<NetworkInterface>,
    pub connections: Vec<ActiveConnection>,
    pub listening_ports: Vec<ListeningPort>,
    pub routes: Vec<RouteEntry>,
    pub captured_at_secs: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventSeverity {
    Info,
    Warning,
    Error,
}

impl EventSeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            EventSeverity::Info => "INFO",
            EventSeverity::Warning => "WARNING",
            EventSeverity::Error => "ERROR",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NetworkEventKind {
    InterfaceAppeared,
    InterfaceRemoved,
    InterfaceUp,
    InterfaceDown,
    Ipv4Added,
    Ipv4Removed,
    Ipv4Changed,
    Ipv6Added,
    Ipv6Removed,
    Ipv6Changed,
    VpnConnected,
    VpnDisconnected,
    ContainerNetworkAppeared,
    ContainerNetworkRemoved,
    ProcessKilled,
    ActionCopied,
    ActionWhois,
    SystemError,
    PublicIpChanged,
    ProviderChanged,
    UpdateAvailable,
    UpdateInstalled,
    UpdateCheckFailed,
}

impl NetworkEventKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            NetworkEventKind::InterfaceAppeared => "Interface Appeared",
            NetworkEventKind::InterfaceRemoved => "Interface Removed",
            NetworkEventKind::InterfaceUp => "Interface Up",
            NetworkEventKind::InterfaceDown => "Interface Down",
            NetworkEventKind::Ipv4Added => "IPv4 Added",
            NetworkEventKind::Ipv4Removed => "IPv4 Removed",
            NetworkEventKind::Ipv4Changed => "IPv4 Changed",
            NetworkEventKind::Ipv6Added => "IPv6 Added",
            NetworkEventKind::Ipv6Removed => "IPv6 Removed",
            NetworkEventKind::Ipv6Changed => "IPv6 Changed",
            NetworkEventKind::VpnConnected => "VPN Connected",
            NetworkEventKind::VpnDisconnected => "VPN Disconnected",
            NetworkEventKind::ContainerNetworkAppeared => "Container Network Appeared",
            NetworkEventKind::ContainerNetworkRemoved => "Container Network Removed",
            NetworkEventKind::ProcessKilled => "Process Killed",
            NetworkEventKind::ActionCopied => "Copied to Clipboard",
            NetworkEventKind::ActionWhois => "WHOIS Lookup",
            NetworkEventKind::SystemError => "System Error",
            NetworkEventKind::PublicIpChanged => "Public IP Changed",
            NetworkEventKind::ProviderChanged => "Provider Changed",
            NetworkEventKind::UpdateAvailable => "Update Available",
            NetworkEventKind::UpdateInstalled => "Update Installed",
            NetworkEventKind::UpdateCheckFailed => "Update Check Failed",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkEvent {
    pub timestamp: std::time::SystemTime,
    pub severity: EventSeverity,
    pub kind: NetworkEventKind,
    pub message: String,
}

impl NetworkEvent {
    pub fn new(kind: NetworkEventKind, severity: EventSeverity, message: String) -> Self {
        Self {
            timestamp: std::time::SystemTime::now(),
            severity,
            kind,
            message,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CommandSourceId {
    Ifconfig,
    NetstatRoutes,
    DefaultRoute,
    NetstatConnections,
    LsofPorts,
    PublicIp,
    GitHubRelease,
    Arp,
}

impl CommandSourceId {
    pub fn as_str(&self) -> &'static str {
        match self {
            CommandSourceId::Ifconfig => {
                if cfg!(target_os = "linux") {
                    "ip -details -statistics address show"
                } else {
                    "ifconfig"
                }
            }
            CommandSourceId::NetstatRoutes => {
                if cfg!(target_os = "linux") {
                    "ip route show"
                } else {
                    "netstat -rn"
                }
            }
            CommandSourceId::DefaultRoute => {
                if cfg!(target_os = "linux") {
                    "ip route show default"
                } else {
                    "route -n get default"
                }
            }
            CommandSourceId::NetstatConnections => "netstat -an",
            CommandSourceId::LsofPorts => "lsof -iTCP -sTCP:LISTEN -P -n",
            CommandSourceId::PublicIp => "curl -s -m 5 https://ipinfo.io/json",
            CommandSourceId::GitHubRelease => "curl -s -L https://api.github.com/repos/<owner>/<repo>/releases/latest",
            CommandSourceId::Arp => "arp -a",
        }
    }
}

#[derive(Clone, Debug)]
pub struct CommandOutput {
    pub command: String,
    pub stdout: String,
    pub stderr: String,
    pub executed_at: std::time::SystemTime,
    pub exit_code: Option<i32>,
}
