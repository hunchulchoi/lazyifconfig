use std::collections::{BTreeMap, HashMap};
use std::net::{Ipv4Addr, Ipv6Addr};

use crate::model::{
    CommandOutput, CommandSourceId, NetworkEvent, NetworkInterface, NetworkSnapshot, PublicIpInfo,
    RouteDiagnostic, RouteEntry, RouteInspectorSection, RoutePathResult, RouteSortColumn, Subnet,
};
use crate::update::{AvailableUpdate, UpdateMessage, UpdateStatus};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewMode {
    Interface,
    Network,
    Connections,
    Ports,
    Timeline,
    Routes,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NavigationItem {
    Interface {
        name: String,
        associated_ip: Option<String>,
    },
    SubnetHeader(Subnet),
    Connection {
        proto: String,
        local: String,
        foreign: String,
        state: Option<String>,
        index: usize,
    },
    ListeningPort {
        proto: String,
        port: String,
        command: String,
        pid: String,
        user: String,
        index: usize,
    },
    Event {
        index: usize,
        kind: crate::model::NetworkEventKind,
        timestamp: std::time::SystemTime,
        message: String,
    },
    Route {
        destination: String,
        gateway: String,
        interface: String,
        index: usize,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InterfaceHistory {
    pub rx_rates: Vec<u64>,
    pub tx_rates: Vec<u64>,
}

#[derive(Clone, Debug)]
pub struct App {
    pub current_snapshot: Option<NetworkSnapshot>,
    pub previous_snapshot: Option<NetworkSnapshot>,
    pub selected_index: usize,
    pub recent_events: Vec<NetworkEvent>,
    pub show_all: bool,
    pub view_mode: ViewMode,
    pub navigation_items: Vec<NavigationItem>,
    pub traffic_history: HashMap<String, InterfaceHistory>,
    pub whois_cache: std::sync::Arc<std::sync::Mutex<HashMap<String, String>>>,
    pub details_scroll: u16,
    pub port_filter: String,
    pub port_filter_active: bool,
    pub public_ip_info: std::sync::Arc<std::sync::Mutex<Option<PublicIpInfo>>>,
    pub current_public_ip_info: Option<PublicIpInfo>,
    pub last_public_ip_fetch: Option<std::time::Instant>,
    pub command_outputs: HashMap<CommandSourceId, CommandOutput>,
    pub raw_viewer: RawViewerState,
    pub help_visible: bool,
    pub async_command_outputs:
        std::sync::Arc<std::sync::Mutex<HashMap<CommandSourceId, CommandOutput>>>,
    pub update_status: UpdateStatus,
    pub pending_update: Option<AvailableUpdate>,
    pub last_update_check: Option<std::time::Instant>,
    pub update_messages: std::sync::Arc<std::sync::Mutex<Vec<UpdateMessage>>>,
    pub attempted_update_version: Option<String>,
    pub release_notes_viewer: ReleaseNotesViewerState,
    pub route_inspector: RouteInspectorState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchMatch {
    pub line_index: usize,
    pub start_byte: usize,
    pub end_byte: usize,
}

#[derive(Clone, Debug, Default)]
pub struct RawViewerState {
    pub active: bool,
    pub sources: Vec<CommandSourceId>,
    pub selected_index: usize,
    pub scroll: u16,
    pub search_query: String,
    pub search_active: bool,
    pub search_matches: Vec<SearchMatch>,
    pub current_match_index: usize,
}

#[derive(Clone, Debug)]
pub struct RouteInspectorState {
    pub active_section: RouteInspectorSection,
    pub destination_input: String,
    pub destination_input_active: bool,
    pub latest_path_result: Option<RoutePathResult>,
    pub latest_path_error: Option<String>,
    pub diagnostics: Vec<RouteDiagnostic>,
    pub route_filter: String,
    pub route_filter_active: bool,
    pub sort_column: RouteSortColumn,
}

impl Default for RouteInspectorState {
    fn default() -> Self {
        Self {
            active_section: RouteInspectorSection::Summary,
            destination_input: "8.8.8.8".to_string(),
            destination_input_active: false,
            latest_path_result: None,
            latest_path_error: None,
            diagnostics: Vec::new(),
            route_filter: String::new(),
            route_filter_active: false,
            sort_column: RouteSortColumn::Destination,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ReleaseNotesViewerState {
    pub active: bool,
    pub scroll: u16,
}

impl Default for App {
    fn default() -> Self {
        Self {
            current_snapshot: None,
            previous_snapshot: None,
            selected_index: 0,
            recent_events: Vec::new(),
            show_all: false,
            view_mode: ViewMode::Interface,
            navigation_items: Vec::new(),
            traffic_history: HashMap::new(),
            whois_cache: std::sync::Arc::new(std::sync::Mutex::new(HashMap::new())),
            details_scroll: 0,
            port_filter: String::new(),
            port_filter_active: false,
            public_ip_info: std::sync::Arc::new(std::sync::Mutex::new(None)),
            current_public_ip_info: None,
            last_public_ip_fetch: None,
            command_outputs: HashMap::new(),
            raw_viewer: RawViewerState::default(),
            help_visible: false,
            async_command_outputs: std::sync::Arc::new(std::sync::Mutex::new(HashMap::new())),
            update_status: UpdateStatus::Idle,
            pending_update: None,
            last_update_check: None,
            update_messages: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            attempted_update_version: None,
            release_notes_viewer: ReleaseNotesViewerState::default(),
            route_inspector: RouteInspectorState::default(),
        }
    }
}

impl App {
    pub fn replace_snapshot(&mut self, mut snapshot: NetworkSnapshot) {
        let route_diagnostics = crate::route_inspector::diagnostics::build_route_diagnostics(
            &snapshot.routes,
            &snapshot.interfaces,
        );

        if !self.show_all {
            snapshot
                .interfaces
                .retain(|interface| interface.status == crate::model::InterfaceStatus::Up);
        }

        let selected_name = self.selected_interface_name().map(str::to_owned);

        if let Some(previous) = self.current_snapshot.replace(snapshot) {
            self.previous_snapshot = Some(previous);
        }

        // Update traffic history
        if let (Some(previous), Some(current)) = (&self.previous_snapshot, &self.current_snapshot) {
            let elapsed = current
                .captured_at_secs
                .saturating_sub(previous.captured_at_secs);
            if elapsed > 0 {
                let previous_by_name = interfaces_by_name(&previous.interfaces);
                for interface in &current.interfaces {
                    if let Some(prev_if) = previous_by_name.get(interface.name.as_str()) {
                        if let (Some(curr_stats), Some(prev_stats)) =
                            (&interface.stats, &prev_if.stats)
                        {
                            let rx_rate =
                                curr_stats.rx_bytes.saturating_sub(prev_stats.rx_bytes) / elapsed;
                            let tx_rate =
                                curr_stats.tx_bytes.saturating_sub(prev_stats.tx_bytes) / elapsed;

                            let history = self
                                .traffic_history
                                .entry(interface.name.clone())
                                .or_default();
                            history.rx_rates.push(rx_rate);
                            history.tx_rates.push(tx_rate);

                            if history.rx_rates.len() > 40 {
                                history.rx_rates.remove(0);
                            }
                            if history.tx_rates.len() > 40 {
                                history.tx_rates.remove(0);
                            }
                        }
                    }
                }
            }
        }

        // Clean up history for removed interfaces
        if let Some(current) = &self.current_snapshot {
            self.traffic_history
                .retain(|name, _| current.interfaces.iter().any(|i| i.name == *name));
        }

        self.push_generated_events();
        self.route_inspector.diagnostics = route_diagnostics;
        self.update_navigation_items();
        self.restore_selection(selected_name.as_deref());
    }

    pub fn selected_interface_name(&self) -> Option<&str> {
        match self.navigation_items.get(self.selected_index)? {
            NavigationItem::Interface { name, .. } => Some(name.as_str()),
            NavigationItem::SubnetHeader(_) => None,
            NavigationItem::Connection { .. } => None,
            NavigationItem::ListeningPort { .. } => None,
            NavigationItem::Event { .. } => None,
            NavigationItem::Route { .. } => None,
        }
    }

    pub fn set_view_mode(&mut self, mode: ViewMode) {
        if self.view_mode == mode {
            return;
        }
        let selected_name = self.selected_interface_name().map(str::to_owned);
        self.view_mode = mode;
        self.details_scroll = 0;
        self.port_filter.clear();
        self.port_filter_active = false;
        self.route_inspector.route_filter.clear();
        self.route_inspector.route_filter_active = false;
        self.update_navigation_items();
        self.restore_selection(selected_name.as_deref());
    }

    pub fn update_raw_viewer_search_matches(&mut self) {
        self.raw_viewer.search_matches.clear();
        self.raw_viewer.current_match_index = 0;

        if self.raw_viewer.search_query.is_empty() {
            return;
        }

        let source_id = match self.raw_viewer.sources.get(self.raw_viewer.selected_index) {
            Some(id) => *id,
            None => return,
        };

        if let Some(output) = self.command_outputs.get(&source_id) {
            let text = format!("{}\n{}", output.stdout, output.stderr);
            let query = self.raw_viewer.search_query.to_lowercase();
            for (line_idx, line) in text.lines().enumerate() {
                let line_lower = line.to_lowercase();
                let mut start_pos = 0;
                while let Some(pos) = line_lower[start_pos..].find(&query) {
                    let absolute_start = start_pos + pos;
                    let absolute_end = absolute_start + query.len();
                    self.raw_viewer.search_matches.push(SearchMatch {
                        line_index: line_idx,
                        start_byte: absolute_start,
                        end_byte: absolute_end,
                    });
                    start_pos = absolute_end;
                }
            }
        }
    }

    pub fn selected_rates(&self) -> Option<(u64, u64)> {
        let current = self.current_snapshot.as_ref()?;
        let previous = self.previous_snapshot.as_ref()?;
        let elapsed = current
            .captured_at_secs
            .checked_sub(previous.captured_at_secs)?;

        if elapsed == 0 {
            return None;
        }

        let selected_name = self.selected_interface_name()?;
        let current_interface = current
            .interfaces
            .iter()
            .find(|i| i.name == selected_name)?;
        let previous_interface = previous
            .interfaces
            .iter()
            .find(|i| i.name == selected_name)?;
        let current_stats = current_interface.stats.as_ref()?;
        let previous_stats = previous_interface.stats.as_ref()?;

        Some((
            current_stats
                .rx_bytes
                .saturating_sub(previous_stats.rx_bytes)
                / elapsed,
            current_stats
                .tx_bytes
                .saturating_sub(previous_stats.tx_bytes)
                / elapsed,
        ))
    }

    pub fn refresh_route_diagnostics(&mut self) {
        let Some(snapshot) = &self.current_snapshot else {
            self.route_inspector.diagnostics.clear();
            return;
        };

        self.route_inspector.diagnostics =
            crate::route_inspector::diagnostics::build_route_diagnostics(
                &snapshot.routes,
                &snapshot.interfaces,
            );
    }

    pub fn filtered_sorted_routes(&self) -> Vec<(usize, &RouteEntry)> {
        let Some(snapshot) = &self.current_snapshot else {
            return Vec::new();
        };

        let query = self.route_inspector.route_filter.to_lowercase();
        let mut routes: Vec<(usize, &RouteEntry)> = snapshot
            .routes
            .iter()
            .enumerate()
            .filter(|(_, route)| {
                query.is_empty()
                    || route.destination.to_lowercase().contains(&query)
                    || route.gateway.to_lowercase().contains(&query)
                    || route.interface.to_lowercase().contains(&query)
            })
            .collect();

        match self.route_inspector.sort_column {
            RouteSortColumn::Destination => {
                routes.sort_by(|(_, a), (_, b)| a.destination.cmp(&b.destination))
            }
            RouteSortColumn::Gateway => routes.sort_by(|(_, a), (_, b)| a.gateway.cmp(&b.gateway)),
            RouteSortColumn::Interface => {
                routes.sort_by(|(_, a), (_, b)| a.interface.cmp(&b.interface))
            }
            RouteSortColumn::Metric => routes.sort_by_key(|(_, route)| route.metric),
        }

        routes
    }

    pub fn select_next_route_section(&mut self) {
        self.route_inspector.active_section = match self.route_inspector.active_section {
            RouteInspectorSection::Summary => RouteInspectorSection::PathViewer,
            RouteInspectorSection::PathViewer => RouteInspectorSection::RouteTable,
            RouteInspectorSection::RouteTable => RouteInspectorSection::VpnRoutes,
            RouteInspectorSection::VpnRoutes => RouteInspectorSection::Diagnostics,
            RouteInspectorSection::Diagnostics => RouteInspectorSection::Summary,
        };
        self.details_scroll = 0;
    }

    pub fn select_previous_route_section(&mut self) {
        self.route_inspector.active_section = match self.route_inspector.active_section {
            RouteInspectorSection::Summary => RouteInspectorSection::Diagnostics,
            RouteInspectorSection::PathViewer => RouteInspectorSection::Summary,
            RouteInspectorSection::RouteTable => RouteInspectorSection::PathViewer,
            RouteInspectorSection::VpnRoutes => RouteInspectorSection::RouteTable,
            RouteInspectorSection::Diagnostics => RouteInspectorSection::VpnRoutes,
        };
        self.details_scroll = 0;
    }

    pub fn update_navigation_items(&mut self) {
        if self.view_mode != ViewMode::Timeline && self.current_snapshot.is_none() {
            self.navigation_items = Vec::new();
            return;
        }

        let snapshot = self.current_snapshot.as_ref();

        match self.view_mode {
            ViewMode::Interface => {
                let snapshot = snapshot.unwrap();
                self.navigation_items = snapshot
                    .interfaces
                    .iter()
                    .map(|i| NavigationItem::Interface {
                        name: i.name.clone(),
                        associated_ip: i.ipv4.first().map(|a| a.value.clone()),
                    })
                    .collect();
            }
            ViewMode::Network => {
                let snapshot = snapshot.unwrap();
                let mut groups: BTreeMap<Subnet, Vec<(String, Option<String>)>> = BTreeMap::new();

                for interface in &snapshot.interfaces {
                    let mut assigned = false;

                    // Group IPv4
                    for addr in &interface.ipv4 {
                        if let Some(prefix) = addr.prefix_len {
                            if let Ok(ip) = addr.value.parse::<Ipv4Addr>() {
                                let net_ip = calculate_ipv4_subnet(&ip, prefix);
                                let subnet = Subnet::Ipv4 {
                                    network: net_ip,
                                    prefix_len: prefix,
                                };
                                groups
                                    .entry(subnet)
                                    .or_default()
                                    .push((interface.name.clone(), Some(addr.value.clone())));
                                assigned = true;
                            }
                        }
                    }

                    // Group IPv6
                    for addr in &interface.ipv6 {
                        if let Some(prefix) = addr.prefix_len {
                            if let Ok(ip) = addr.value.parse::<Ipv6Addr>() {
                                let net_ip = calculate_ipv6_subnet(&ip, prefix);
                                let subnet = Subnet::Ipv6 {
                                    network: net_ip,
                                    prefix_len: prefix,
                                };
                                groups
                                    .entry(subnet)
                                    .or_default()
                                    .push((interface.name.clone(), Some(addr.value.clone())));
                                assigned = true;
                            }
                        }
                    }

                    if !assigned {
                        groups
                            .entry(Subnet::Unassigned)
                            .or_default()
                            .push((interface.name.clone(), None));
                    }
                }

                let mut items = Vec::new();
                for (subnet, mut members) in groups {
                    // Sort members by interface name
                    members.sort_by(|a, b| a.0.cmp(&b.0));
                    members.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);

                    items.push(NavigationItem::SubnetHeader(subnet));
                    for (name, ip) in members {
                        items.push(NavigationItem::Interface {
                            name,
                            associated_ip: ip,
                        });
                    }
                }
                self.navigation_items = items;
            }
            ViewMode::Connections => {
                let snapshot = snapshot.unwrap();
                self.navigation_items = snapshot
                    .connections
                    .iter()
                    .enumerate()
                    .map(|(idx, c)| NavigationItem::Connection {
                        proto: c.proto.clone(),
                        local: format!("{}:{}", c.local_ip, c.local_port),
                        foreign: format!("{}:{}", c.foreign_ip, c.foreign_port),
                        state: c.state.clone(),
                        index: idx,
                    })
                    .collect();
            }
            ViewMode::Ports => {
                let snapshot = snapshot.unwrap();
                let query = self.port_filter.to_lowercase();
                self.navigation_items = snapshot
                    .listening_ports
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| {
                        if query.is_empty() {
                            true
                        } else {
                            p.local_port.contains(&query)
                                || p.command.to_lowercase().contains(&query)
                                || p.pid.contains(&query)
                                || p.proto.to_lowercase().contains(&query)
                        }
                    })
                    .map(|(idx, p)| NavigationItem::ListeningPort {
                        proto: p.proto.clone(),
                        port: p.local_port.clone(),
                        command: p.command.clone(),
                        pid: p.pid.clone(),
                        user: p.user.clone(),
                        index: idx,
                    })
                    .collect();
            }
            ViewMode::Timeline => {
                self.navigation_items = self
                    .recent_events
                    .iter()
                    .enumerate()
                    .map(|(idx, e)| NavigationItem::Event {
                        index: idx,
                        kind: e.kind,
                        timestamp: e.timestamp,
                        message: e.message.clone(),
                    })
                    .collect();
            }
            ViewMode::Routes => {
                self.navigation_items = self
                    .filtered_sorted_routes()
                    .into_iter()
                    .map(|(idx, r)| NavigationItem::Route {
                        destination: r.destination.clone(),
                        gateway: r.gateway.clone(),
                        interface: r.interface.clone(),
                        index: idx,
                    })
                    .collect();
            }
        }
    }

    fn push_generated_events(&mut self) {
        use crate::model::{EventSeverity, NetworkEventKind};

        let Some(current) = self.current_snapshot.as_ref() else {
            return;
        };

        let mut new_events = Vec::new();

        if let Some(previous) = self.previous_snapshot.as_ref() {
            let previous_by_name = interfaces_by_name(&previous.interfaces);
            let current_by_name = interfaces_by_name(&current.interfaces);

            for interface in &current.interfaces {
                match previous_by_name.get(interface.name.as_str()) {
                    None => {
                        // Interface Appeared
                        match interface.network_kind {
                            crate::model::NetworkKind::Vpn => {
                                new_events.push(NetworkEvent::new(
                                    NetworkEventKind::VpnConnected,
                                    EventSeverity::Info,
                                    format!("{} connected (VPN appeared)", interface.name),
                                ));
                            }
                            crate::model::NetworkKind::Container => {
                                new_events.push(NetworkEvent::new(
                                    NetworkEventKind::ContainerNetworkAppeared,
                                    EventSeverity::Info,
                                    format!("{} appeared (Docker/Bridge)", interface.name),
                                ));
                            }
                            _ => {
                                new_events.push(NetworkEvent::new(
                                    NetworkEventKind::InterfaceAppeared,
                                    EventSeverity::Info,
                                    format!("{} appeared", interface.name),
                                ));
                            }
                        }
                        // Initial IPs as added
                        for addr in &interface.ipv4 {
                            new_events.push(NetworkEvent::new(
                                NetworkEventKind::Ipv4Added,
                                EventSeverity::Info,
                                format!("{}: added IPv4 {}", interface.name, addr.value),
                            ));
                        }
                        for addr in &interface.ipv6 {
                            new_events.push(NetworkEvent::new(
                                NetworkEventKind::Ipv6Added,
                                EventSeverity::Info,
                                format!("{}: added IPv6 {}", interface.name, addr.value),
                            ));
                        }
                    }
                    Some(previous_interface) => {
                        // Check status change
                        if previous_interface.status != interface.status {
                            let (kind, severity) = match interface.status {
                                crate::model::InterfaceStatus::Up => match interface.network_kind {
                                    crate::model::NetworkKind::Vpn => {
                                        (NetworkEventKind::VpnConnected, EventSeverity::Info)
                                    }
                                    crate::model::NetworkKind::Container => (
                                        NetworkEventKind::ContainerNetworkAppeared,
                                        EventSeverity::Info,
                                    ),
                                    _ => (NetworkEventKind::InterfaceUp, EventSeverity::Info),
                                },
                                crate::model::InterfaceStatus::Down => {
                                    match interface.network_kind {
                                        crate::model::NetworkKind::Vpn => (
                                            NetworkEventKind::VpnDisconnected,
                                            EventSeverity::Warning,
                                        ),
                                        crate::model::NetworkKind::Container => (
                                            NetworkEventKind::ContainerNetworkRemoved,
                                            EventSeverity::Info,
                                        ),
                                        _ => (
                                            NetworkEventKind::InterfaceDown,
                                            EventSeverity::Warning,
                                        ),
                                    }
                                }
                            };
                            let msg = match kind {
                                NetworkEventKind::VpnConnected => {
                                    format!("{} connected", interface.name)
                                }
                                NetworkEventKind::VpnDisconnected => {
                                    format!("{} disconnected", interface.name)
                                }
                                NetworkEventKind::ContainerNetworkAppeared => {
                                    format!("{} appeared", interface.name)
                                }
                                NetworkEventKind::ContainerNetworkRemoved => {
                                    format!("{} removed", interface.name)
                                }
                                _ => format!(
                                    "{} status changed: {} -> {}",
                                    interface.name,
                                    status_label(&previous_interface.status),
                                    status_label(&interface.status)
                                ),
                            };
                            new_events.push(NetworkEvent::new(kind, severity, msg));
                        }

                        // IPv4 Address changes
                        let prev_v4: Vec<String> = previous_interface
                            .ipv4
                            .iter()
                            .map(|a| a.value.clone())
                            .collect();
                        let curr_v4: Vec<String> =
                            interface.ipv4.iter().map(|a| a.value.clone()).collect();
                        if prev_v4.len() == 1 && curr_v4.len() == 1 && prev_v4[0] != curr_v4[0] {
                            new_events.push(NetworkEvent::new(
                                NetworkEventKind::Ipv4Changed,
                                EventSeverity::Info,
                                format!("{}: {} -> {}", interface.name, prev_v4[0], curr_v4[0]),
                            ));
                        } else {
                            for ip in &curr_v4 {
                                if !prev_v4.contains(ip) {
                                    new_events.push(NetworkEvent::new(
                                        NetworkEventKind::Ipv4Added,
                                        EventSeverity::Info,
                                        format!("{}: added IPv4 {}", interface.name, ip),
                                    ));
                                }
                            }
                            for ip in &prev_v4 {
                                if !curr_v4.contains(ip) {
                                    new_events.push(NetworkEvent::new(
                                        NetworkEventKind::Ipv4Removed,
                                        EventSeverity::Info,
                                        format!("{}: removed IPv4 {}", interface.name, ip),
                                    ));
                                }
                            }
                        }

                        // IPv6 Address changes
                        let prev_v6: Vec<String> = previous_interface
                            .ipv6
                            .iter()
                            .map(|a| a.value.clone())
                            .collect();
                        let curr_v6: Vec<String> =
                            interface.ipv6.iter().map(|a| a.value.clone()).collect();
                        if prev_v6.len() == 1 && curr_v6.len() == 1 && prev_v6[0] != curr_v6[0] {
                            new_events.push(NetworkEvent::new(
                                NetworkEventKind::Ipv6Changed,
                                EventSeverity::Info,
                                format!("{}: {} -> {}", interface.name, prev_v6[0], curr_v6[0]),
                            ));
                        } else {
                            for ip in &curr_v6 {
                                if !prev_v6.contains(ip) {
                                    new_events.push(NetworkEvent::new(
                                        NetworkEventKind::Ipv6Added,
                                        EventSeverity::Info,
                                        format!("{}: added IPv6 {}", interface.name, ip),
                                    ));
                                }
                            }
                            for ip in &prev_v6 {
                                if !curr_v6.contains(ip) {
                                    new_events.push(NetworkEvent::new(
                                        NetworkEventKind::Ipv6Removed,
                                        EventSeverity::Info,
                                        format!("{}: removed IPv6 {}", interface.name, ip),
                                    ));
                                }
                            }
                        }
                    }
                }
            }

            for interface in &previous.interfaces {
                if !current_by_name.contains_key(interface.name.as_str()) {
                    // Interface Removed
                    match interface.network_kind {
                        crate::model::NetworkKind::Vpn => {
                            new_events.push(NetworkEvent::new(
                                NetworkEventKind::VpnDisconnected,
                                EventSeverity::Warning,
                                format!("{} disconnected (VPN disappeared)", interface.name),
                            ));
                        }
                        crate::model::NetworkKind::Container => {
                            new_events.push(NetworkEvent::new(
                                NetworkEventKind::ContainerNetworkRemoved,
                                EventSeverity::Info,
                                format!("{} removed (Docker/Bridge)", interface.name),
                            ));
                        }
                        _ => {
                            new_events.push(NetworkEvent::new(
                                NetworkEventKind::InterfaceRemoved,
                                EventSeverity::Warning,
                                format!("{} disappeared", interface.name),
                            ));
                        }
                    }
                }
            }
        }

        self.recent_events.extend(new_events);

        if self.recent_events.len() > 100 {
            let overflow = self.recent_events.len() - 100;
            self.recent_events.drain(0..overflow);
        }
    }

    fn restore_selection(&mut self, selected_name: Option<&str>) {
        let len = self.navigation_items.len();
        if len == 0 {
            self.selected_index = 0;
            return;
        }

        if let Some(name) = selected_name {
            if let Some(index) = self.navigation_items.iter().position(|item| match item {
                NavigationItem::Interface {
                    name: item_name, ..
                } => item_name == name,
                _ => false,
            }) {
                self.selected_index = index;
                return;
            }
        }

        if self.selected_index >= len {
            self.selected_index = len - 1;
        }
    }

    pub fn select_next(&mut self) {
        let len = self.navigation_items.len();
        if len > 0 {
            self.selected_index = (self.selected_index + 1) % len;
            self.details_scroll = 0;
        }
    }

    pub fn select_previous(&mut self) {
        let len = self.navigation_items.len();
        if len > 0 {
            if self.selected_index == 0 {
                self.selected_index = len - 1;
            } else {
                self.selected_index -= 1;
            }
            self.details_scroll = 0;
        }
    }

    pub fn scroll_details_down(&mut self) {
        self.details_scroll = self.details_scroll.saturating_add(1);
    }

    pub fn scroll_details_up(&mut self) {
        self.details_scroll = self.details_scroll.saturating_sub(1);
    }

    pub fn get_whois_result(&self, ip: &str) -> Option<String> {
        let lock = self.whois_cache.lock().ok()?;
        lock.get(ip).cloned()
    }

    pub fn push_event(&mut self, event: NetworkEvent) {
        self.recent_events.push(event);
        if self.recent_events.len() > 100 {
            let overflow = self.recent_events.len() - 100;
            self.recent_events.drain(0..overflow);
        }
    }
}

fn calculate_ipv4_subnet(ip: &Ipv4Addr, prefix_len: u8) -> Ipv4Addr {
    let ip_u32 = u32::from(*ip);
    let mask = if prefix_len == 0 {
        0
    } else if prefix_len >= 32 {
        u32::MAX
    } else {
        u32::MAX << (32 - prefix_len)
    };
    Ipv4Addr::from(ip_u32 & mask)
}

fn calculate_ipv6_subnet(ip: &Ipv6Addr, prefix_len: u8) -> Ipv6Addr {
    let octets = ip.octets();
    let mut mask_octets = [0u8; 16];
    for i in 0..16 {
        let bit_index = (i as u8) * 8;
        if prefix_len >= bit_index + 8 {
            mask_octets[i] = 0xff;
        } else if prefix_len <= bit_index {
            mask_octets[i] = 0x00;
        } else {
            let remaining = prefix_len - bit_index;
            mask_octets[i] = 0xff_u8.checked_shl((8 - remaining) as u32).unwrap_or(0);
        }
    }
    let mut subnet_octets = [0u8; 16];
    for i in 0..16 {
        subnet_octets[i] = octets[i] & mask_octets[i];
    }
    Ipv6Addr::from(subnet_octets)
}

fn interfaces_by_name<'a>(
    interfaces: &'a [NetworkInterface],
) -> HashMap<&'a str, &'a NetworkInterface> {
    interfaces
        .iter()
        .map(|interface| (interface.name.as_str(), interface))
        .collect()
}

fn status_label(status: &crate::model::InterfaceStatus) -> &'static str {
    match status {
        crate::model::InterfaceStatus::Up => "up",
        crate::model::InterfaceStatus::Down => "down",
    }
}
