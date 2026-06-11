use std::collections::{BTreeMap, HashMap};
use std::net::{Ipv4Addr, Ipv6Addr};

use crate::model::{
    CommandOutput, CommandSourceId, NetworkEvent, NetworkInterface, NetworkSnapshot,
    ProcessMetrics, PublicIpInfo, RouteDiagnostic, RouteEntry, RouteInspectorSection,
    RoutePathResult, RouteSortColumn, Subnet,
};
use crate::tools::{
    validate_tool_input, ToolAvailability, ToolExecutionState, ToolId, ToolInput, ToolRegistry,
    ToolResult,
};
use crate::update::{AvailableUpdate, UpdateMessage, UpdateStatus};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortDetailsSection {
    Summary,
    Process,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionDetailsSection {
    Summary,
    Whois,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewMode {
    Interface,
    Network,
    Connections,
    Ports,
    Timeline,
    Routes,
    Tools,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortSortColumn {
    Port,
    Command,
    Pid,
    User,
    Proto,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionSortColumn {
    Local,
    Foreign,
    State,
    Proto,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

const VIEW_MODE_TABS: [ViewMode; 7] = [
    ViewMode::Interface,
    ViewMode::Network,
    ViewMode::Ports,
    ViewMode::Connections,
    ViewMode::Routes,
    ViewMode::Tools,
    ViewMode::Timeline,
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NavigationItem {
    Interface {
        name: String,
        associated_ip: Option<String>,
    },
    SubnetHeader(Subnet),
    Connection {
        proto: String,
        local_ip: String,
        local_port: String,
        foreign_ip: String,
        foreign_port: String,
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
pub struct ToolsState {
    pub registry: ToolRegistry,
    pub selected_index: usize,
    pub selected_field_index: usize,
    pub input_modal_open: bool,
    pub editing_input: bool,
    pub inputs: HashMap<ToolId, ToolInput>,
    pub results: HashMap<ToolId, ToolResult>,
    pub errors: HashMap<ToolId, String>,
    pub states: HashMap<ToolId, ToolExecutionState>,
    pub raw_scroll: u16,
    pub dns_raw_output_expanded: bool,
}

impl Default for ToolsState {
    fn default() -> Self {
        Self {
            registry: ToolRegistry::default(),
            selected_index: 0,
            selected_field_index: 0,
            input_modal_open: false,
            editing_input: false,
            inputs: HashMap::new(),
            results: HashMap::new(),
            errors: HashMap::new(),
            states: HashMap::new(),
            raw_scroll: 0,
            dns_raw_output_expanded: true,
        }
    }
}

impl ToolsState {
    pub fn selected_tool_id(&self) -> ToolId {
        self.registry.definitions()[self.selected_index].id
    }

    pub fn selected_definition(&self) -> &crate::tools::ToolDefinition {
        &self.registry.definitions()[self.selected_index]
    }

    pub fn selected_tool_is_runnable(&self) -> bool {
        self.selected_definition().availability == ToolAvailability::Runnable
    }

    pub fn select_next_tool(&mut self) {
        let len = self.registry.definitions().len();
        if len > 0 {
            self.selected_index = (self.selected_index + 1) % len;
            self.selected_field_index = 0;
            self.raw_scroll = 0;
            self.dns_raw_output_expanded = true;
        }
    }

    pub fn select_previous_tool(&mut self) {
        let len = self.registry.definitions().len();
        if len > 0 {
            self.selected_index = if self.selected_index == 0 {
                len - 1
            } else {
                self.selected_index - 1
            };
            self.selected_field_index = 0;
            self.raw_scroll = 0;
            self.dns_raw_output_expanded = true;
        }
    }

    pub fn input_for_selected_tool(&mut self) -> &mut ToolInput {
        let id = self.selected_tool_id();
        self.inputs.entry(id).or_insert_with(|| {
            let mut input = ToolInput::default();
            if let Some(definition) = self.registry.definition(id) {
                for field in definition.fields {
                    input.values.insert(field.key.to_string(), String::new());
                }
            }
            input
        })
    }

    pub fn start_input_editing(&mut self) {
        self.open_input_modal();
    }

    pub fn selected_tool_is_dns_lookup(&self) -> bool {
        self.selected_tool_id() == ToolId::DnsLookup
    }

    pub fn toggle_dns_raw_output(&mut self) {
        if self.selected_tool_is_dns_lookup() {
            self.dns_raw_output_expanded = !self.dns_raw_output_expanded;
        }
    }

    pub fn expand_dns_raw_output(&mut self) {
        if self.selected_tool_is_dns_lookup() {
            self.dns_raw_output_expanded = true;
        }
    }

    pub fn stop_input_editing(&mut self) {
        self.close_input_modal();
    }

    pub fn open_input_modal(&mut self) {
        if self.selected_tool_is_runnable() && !self.selected_definition().fields.is_empty() {
            self.input_modal_open = true;
            self.editing_input = true;
            self.selected_field_index = 0;
        }
    }

    pub fn close_input_modal(&mut self) {
        self.input_modal_open = false;
        self.editing_input = false;
    }

    pub fn select_next_field(&mut self) {
        let field_count = self.selected_definition().fields.len();
        if field_count > 0 {
            self.selected_field_index = (self.selected_field_index + 1) % field_count;
        }
    }

    pub fn push_input_char(&mut self, c: char) {
        if matches!(c, '\n' | '\r') {
            return;
        }
        if self.selected_definition().fields.is_empty() {
            return;
        }
        let field_key = self.selected_definition().fields[self.selected_field_index]
            .key
            .to_string();
        self.input_for_selected_tool()
            .values
            .entry(field_key)
            .or_default()
            .push(c);
    }

    pub fn push_input_text(&mut self, text: &str) {
        let first_line = text.lines().next().unwrap_or("").trim_end_matches('\r');
        for c in first_line.chars() {
            self.push_input_char(c);
        }
    }

    pub fn pop_input_char(&mut self) {
        if self.selected_definition().fields.is_empty() {
            return;
        }
        let field_key = self.selected_definition().fields[self.selected_field_index]
            .key
            .to_string();
        if let Some(value) = self.input_for_selected_tool().values.get_mut(&field_key) {
            value.pop();
        }
    }

    pub fn selected_input_validation_errors(&self) -> Vec<String> {
        let tool_id = self.selected_tool_id();
        let input = self.inputs.get(&tool_id).cloned().unwrap_or_default();
        validate_tool_input(tool_id, &input)
    }
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
    pub port_sort_column: PortSortColumn,
    pub port_sort_direction: SortDirection,
    pub connection_filter: String,
    pub connection_filter_active: bool,
    pub connection_sort_column: ConnectionSortColumn,
    pub connection_sort_direction: SortDirection,
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
    pub latest_release_date: Option<String>,
    pub release_notes_viewer: ReleaseNotesViewerState,
    pub process_metrics: Option<ProcessMetrics>,
    pub tools: ToolsState,
    pub pending_tool_results:
        std::sync::Arc<std::sync::Mutex<Vec<(ToolId, Result<ToolResult, String>)>>>,
    pub route_inspector: RouteInspectorState,
    pub port_details_section: PortDetailsSection,
    pub connection_details_section: ConnectionDetailsSection,
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
    pub route_sort_direction: SortDirection,
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
            route_sort_direction: SortDirection::Ascending,
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
            port_sort_column: PortSortColumn::Port,
            port_sort_direction: SortDirection::Ascending,
            connection_filter: String::new(),
            connection_filter_active: false,
            connection_sort_column: ConnectionSortColumn::Local,
            connection_sort_direction: SortDirection::Ascending,
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
            latest_release_date: None,
            release_notes_viewer: ReleaseNotesViewerState::default(),
            process_metrics: None,
            tools: ToolsState::default(),
            pending_tool_results: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            route_inspector: RouteInspectorState::default(),
            port_details_section: PortDetailsSection::Summary,
            connection_details_section: ConnectionDetailsSection::Summary,
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

    fn selected_listening_port_index(&self) -> Option<usize> {
        match self.navigation_items.get(self.selected_index)? {
            NavigationItem::ListeningPort { index, .. } => Some(*index),
            _ => None,
        }
    }

    fn selected_connection_index(&self) -> Option<usize> {
        match self.navigation_items.get(self.selected_index)? {
            NavigationItem::Connection { index, .. } => Some(*index),
            _ => None,
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
        self.connection_filter.clear();
        self.connection_filter_active = false;
        self.route_inspector.route_filter.clear();
        self.route_inspector.route_filter_active = false;
        match self.view_mode {
            ViewMode::Routes => self.route_inspector.active_section = RouteInspectorSection::Summary,
            ViewMode::Ports => self.port_details_section = PortDetailsSection::Summary,
            ViewMode::Connections => self.connection_details_section = ConnectionDetailsSection::Summary,
            _ => {}
        }
        self.update_navigation_items();
        self.restore_selection(selected_name.as_deref());
    }

    pub fn select_next_view_mode(&mut self) {
        let current_index = VIEW_MODE_TABS
            .iter()
            .position(|mode| *mode == self.view_mode)
            .unwrap_or(0);
        let next_index = (current_index + 1) % VIEW_MODE_TABS.len();
        self.set_view_mode(VIEW_MODE_TABS[next_index]);
    }

    pub fn select_previous_view_mode(&mut self) {
        let current_index = VIEW_MODE_TABS
            .iter()
            .position(|mode| *mode == self.view_mode)
            .unwrap_or(0);
        let previous_index = if current_index == 0 {
            VIEW_MODE_TABS.len() - 1
        } else {
            current_index - 1
        };
        self.set_view_mode(VIEW_MODE_TABS[previous_index]);
    }

    pub fn cycle_port_sort_column(&mut self) {
        let selected_port_index = self.selected_listening_port_index();
        self.port_sort_column = match self.port_sort_column {
            PortSortColumn::Port => PortSortColumn::Command,
            PortSortColumn::Command => PortSortColumn::Pid,
            PortSortColumn::Pid => PortSortColumn::User,
            PortSortColumn::User => PortSortColumn::Proto,
            PortSortColumn::Proto => PortSortColumn::Port,
        };
        self.port_sort_direction = SortDirection::Ascending;
        self.update_navigation_items();
        self.restore_selected_listening_port(selected_port_index);
    }

    pub fn toggle_port_sort_direction(&mut self) {
        let selected_port_index = self.selected_listening_port_index();
        self.port_sort_direction = match self.port_sort_direction {
            SortDirection::Ascending => SortDirection::Descending,
            SortDirection::Descending => SortDirection::Ascending,
        };
        self.update_navigation_items();
        self.restore_selected_listening_port(selected_port_index);
    }

    pub fn cycle_connection_sort_column(&mut self) {
        let selected_connection_index = self.selected_connection_index();
        self.connection_sort_column = match self.connection_sort_column {
            ConnectionSortColumn::Local => ConnectionSortColumn::Foreign,
            ConnectionSortColumn::Foreign => ConnectionSortColumn::State,
            ConnectionSortColumn::State => ConnectionSortColumn::Proto,
            ConnectionSortColumn::Proto => ConnectionSortColumn::Local,
        };
        self.connection_sort_direction = SortDirection::Ascending;
        self.update_navigation_items();
        self.restore_selected_connection(selected_connection_index);
    }

    pub fn toggle_connection_sort_direction(&mut self) {
        let selected_connection_index = self.selected_connection_index();
        self.connection_sort_direction = match self.connection_sort_direction {
            SortDirection::Ascending => SortDirection::Descending,
            SortDirection::Descending => SortDirection::Ascending,
        };
        self.update_navigation_items();
        self.restore_selected_connection(selected_connection_index);
    }

    pub fn cycle_route_sort_column(&mut self) {
        let selected_route_index = self.selected_route_index();
        self.route_inspector.sort_column = match self.route_inspector.sort_column {
            RouteSortColumn::Destination => RouteSortColumn::Gateway,
            RouteSortColumn::Gateway => RouteSortColumn::Interface,
            RouteSortColumn::Interface => RouteSortColumn::Metric,
            RouteSortColumn::Metric => RouteSortColumn::Destination,
        };
        self.route_inspector.route_sort_direction = SortDirection::Ascending;
        self.update_navigation_items();
        self.restore_selected_route(selected_route_index);
    }

    pub fn toggle_route_sort_direction(&mut self) {
        self.route_inspector.route_sort_direction = match self.route_inspector.route_sort_direction
        {
            SortDirection::Ascending => SortDirection::Descending,
            SortDirection::Descending => SortDirection::Ascending,
        };
        let selected_route_index = self.selected_route_index();
        self.update_navigation_items();
        self.restore_selected_route(selected_route_index);
    }

    fn restore_selected_listening_port(&mut self, selected_port_index: Option<usize>) {
        if let Some(selected_port_index) = selected_port_index {
            if let Some(index) = self.navigation_items.iter().position(|item| {
                matches!(item, NavigationItem::ListeningPort { index, .. } if *index == selected_port_index)
            }) {
                self.selected_index = index;
                return;
            }
        }

        if self.selected_index >= self.navigation_items.len() {
            self.selected_index = self.navigation_items.len().saturating_sub(1);
        }
    }

    fn selected_route_index(&self) -> Option<usize> {
        match self.navigation_items.get(self.selected_index)? {
            NavigationItem::Route { index, .. } => Some(*index),
            _ => None,
        }
    }

    fn restore_selected_route(&mut self, selected_route_index: Option<usize>) {
        if let Some(selected_route_index) = selected_route_index {
            if let Some(index) = self.navigation_items.iter().position(|item| {
                matches!(item, NavigationItem::Route { index, .. } if *index == selected_route_index)
            }) {
                self.selected_index = index;
                return;
            }
        }

        if self.selected_index >= self.navigation_items.len() {
            self.selected_index = self.navigation_items.len().saturating_sub(1);
        }
    }

    fn restore_selected_connection(&mut self, selected_connection_index: Option<usize>) {
        if let Some(selected_connection_index) = selected_connection_index {
            if let Some(index) = self.navigation_items.iter().position(|item| {
                matches!(item, NavigationItem::Connection { index, .. } if *index == selected_connection_index)
            }) {
                self.selected_index = index;
                return;
            }
        }

        if self.selected_index >= self.navigation_items.len() {
            self.selected_index = self.navigation_items.len().saturating_sub(1);
        }
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

        routes.sort_by(|(_, a), (_, b)| {
            let ordering = match self.route_inspector.sort_column {
                RouteSortColumn::Destination => a.destination.cmp(&b.destination),
                RouteSortColumn::Gateway => a.gateway.cmp(&b.gateway),
                RouteSortColumn::Interface => a.interface.cmp(&b.interface),
                RouteSortColumn::Metric => a
                    .metric
                    .unwrap_or(u32::MAX)
                    .cmp(&b.metric.unwrap_or(u32::MAX)),
            };

            match self.route_inspector.route_sort_direction {
                SortDirection::Ascending => ordering,
                SortDirection::Descending => ordering.reverse(),
            }
        });

        routes
    }

    pub fn select_next_route_section(&mut self) {
        self.route_inspector.active_section = match self.route_inspector.active_section {
            RouteInspectorSection::Summary => RouteInspectorSection::PathViewer,
            RouteInspectorSection::PathViewer => RouteInspectorSection::VpnRoutes,
            RouteInspectorSection::VpnRoutes => RouteInspectorSection::Diagnostics,
            RouteInspectorSection::Diagnostics => RouteInspectorSection::Summary,
        };
        self.details_scroll = 0;
    }

    pub fn select_previous_route_section(&mut self) {
        self.route_inspector.active_section = match self.route_inspector.active_section {
            RouteInspectorSection::Summary => RouteInspectorSection::Diagnostics,
            RouteInspectorSection::PathViewer => RouteInspectorSection::Summary,
            RouteInspectorSection::VpnRoutes => RouteInspectorSection::PathViewer,
            RouteInspectorSection::Diagnostics => RouteInspectorSection::VpnRoutes,
        };
        self.details_scroll = 0;
    }

    pub fn select_next_port_details_section(&mut self) {
        self.port_details_section = match self.port_details_section {
            PortDetailsSection::Summary => PortDetailsSection::Process,
            PortDetailsSection::Process => PortDetailsSection::Summary,
        };
        self.details_scroll = 0;
    }

    pub fn select_previous_port_details_section(&mut self) {
        self.port_details_section = match self.port_details_section {
            PortDetailsSection::Summary => PortDetailsSection::Process,
            PortDetailsSection::Process => PortDetailsSection::Summary,
        };
        self.details_scroll = 0;
    }

    pub fn select_port_details_section_by_index(&mut self, index: usize) {
        self.port_details_section = if index == 0 {
            PortDetailsSection::Summary
        } else {
            PortDetailsSection::Process
        };
        self.details_scroll = 0;
    }

    pub fn select_next_connection_details_section(&mut self) {
        self.connection_details_section = match self.connection_details_section {
            ConnectionDetailsSection::Summary => ConnectionDetailsSection::Whois,
            ConnectionDetailsSection::Whois => ConnectionDetailsSection::Summary,
        };
        self.details_scroll = 0;
    }

    pub fn select_previous_connection_details_section(&mut self) {
        self.connection_details_section = match self.connection_details_section {
            ConnectionDetailsSection::Summary => ConnectionDetailsSection::Whois,
            ConnectionDetailsSection::Whois => ConnectionDetailsSection::Summary,
        };
        self.details_scroll = 0;
    }

    pub fn select_connection_details_section_by_index(&mut self, index: usize) {
        self.connection_details_section = if index == 0 {
            ConnectionDetailsSection::Summary
        } else {
            ConnectionDetailsSection::Whois
        };
        self.details_scroll = 0;
    }

    pub fn select_route_section_by_index(&mut self, index: usize) {
        self.route_inspector.active_section = match index {
            0 => RouteInspectorSection::Summary,
            1 => RouteInspectorSection::PathViewer,
            2 => RouteInspectorSection::VpnRoutes,
            _ => RouteInspectorSection::Diagnostics,
        };
        self.details_scroll = 0;
    }

    pub fn update_navigation_items(&mut self) {
        if !matches!(self.view_mode, ViewMode::Timeline | ViewMode::Tools)
            && self.current_snapshot.is_none()
        {
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
                let query = self.connection_filter.to_lowercase();
                let mut items: Vec<_> = snapshot
                    .connections
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, c)| {
                        let matches_filter = query.is_empty()
                            || c.proto.to_lowercase().contains(&query)
                            || c.local_ip.to_lowercase().contains(&query)
                            || c.local_port.to_lowercase().contains(&query)
                            || c.foreign_ip.to_lowercase().contains(&query)
                            || c.foreign_port.to_lowercase().contains(&query)
                            || c.state
                                .as_deref()
                                .unwrap_or("")
                                .to_lowercase()
                                .contains(&query);

                        if matches_filter {
                            Some((idx, c))
                        } else {
                            None
                        }
                    })
                    .map(|(idx, c)| NavigationItem::Connection {
                        proto: c.proto.clone(),
                        local_ip: c.local_ip.clone(),
                        local_port: c.local_port.clone(),
                        foreign_ip: c.foreign_ip.clone(),
                        foreign_port: c.foreign_port.clone(),
                        state: c.state.clone(),
                        index: idx,
                    })
                    .collect();
                sort_connection_items(
                    &mut items,
                    self.connection_sort_column,
                    self.connection_sort_direction,
                );
                self.navigation_items = items;
            }
            ViewMode::Ports => {
                let snapshot = snapshot.unwrap();
                let query = self.port_filter.to_lowercase();
                let mut items: Vec<_> = snapshot
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
                sort_listening_port_items(
                    &mut items,
                    self.port_sort_column,
                    self.port_sort_direction,
                );
                self.navigation_items = items;
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
            ViewMode::Tools => {
                self.navigation_items = Vec::new();
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

    pub fn drain_pending_tool_results(&mut self) {
        let drained = if let Ok(mut lock) = self.pending_tool_results.lock() {
            lock.drain(..).collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        for (id, result) in drained {
            match result {
                Ok(result) => {
                    self.tools.states.insert(id, ToolExecutionState::Succeeded);
                    self.tools.errors.remove(&id);
                    self.tools.results.insert(id, result);
                }
                Err(error) => {
                    self.tools.states.insert(id, ToolExecutionState::Failed);
                    self.tools.errors.insert(id, error);
                }
            }
        }
    }
}

fn sort_listening_port_items(
    items: &mut [NavigationItem],
    column: PortSortColumn,
    direction: SortDirection,
) {
    items.sort_by(|a, b| {
        let ordering = compare_listening_port_items(a, b, column);
        match direction {
            SortDirection::Ascending => ordering,
            SortDirection::Descending => ordering.reverse(),
        }
    });
}

fn compare_listening_port_items(
    a: &NavigationItem,
    b: &NavigationItem,
    column: PortSortColumn,
) -> std::cmp::Ordering {
    let (
        NavigationItem::ListeningPort {
            proto: proto_a,
            port: port_a,
            command: command_a,
            pid: pid_a,
            user: user_a,
            ..
        },
        NavigationItem::ListeningPort {
            proto: proto_b,
            port: port_b,
            command: command_b,
            pid: pid_b,
            user: user_b,
            ..
        },
    ) = (a, b)
    else {
        return std::cmp::Ordering::Equal;
    };

    let primary = match column {
        PortSortColumn::Port => compare_numeric_text(port_a, port_b),
        PortSortColumn::Command => compare_text(command_a, command_b),
        PortSortColumn::Pid => compare_numeric_text(pid_a, pid_b),
        PortSortColumn::User => compare_text(user_a, user_b),
        PortSortColumn::Proto => compare_text(proto_a, proto_b),
    };

    primary
        .then_with(|| compare_numeric_text(port_a, port_b))
        .then_with(|| compare_text(command_a, command_b))
        .then_with(|| compare_numeric_text(pid_a, pid_b))
}

fn sort_connection_items(
    items: &mut [NavigationItem],
    column: ConnectionSortColumn,
    direction: SortDirection,
) {
    items.sort_by(|a, b| {
        let ordering = compare_connection_items(a, b, column);
        match direction {
            SortDirection::Ascending => ordering,
            SortDirection::Descending => ordering.reverse(),
        }
    });
}

fn compare_connection_items(
    a: &NavigationItem,
    b: &NavigationItem,
    column: ConnectionSortColumn,
) -> std::cmp::Ordering {
    let (
        NavigationItem::Connection {
            proto: proto_a,
            local_ip: local_ip_a,
            local_port: local_port_a,
            foreign_ip: foreign_ip_a,
            foreign_port: foreign_port_a,
            state: state_a,
            ..
        },
        NavigationItem::Connection {
            proto: proto_b,
            local_ip: local_ip_b,
            local_port: local_port_b,
            foreign_ip: foreign_ip_b,
            foreign_port: foreign_port_b,
            state: state_b,
            ..
        },
    ) = (a, b)
    else {
        return std::cmp::Ordering::Equal;
    };

    let state_a = state_a.as_deref().unwrap_or("");
    let state_b = state_b.as_deref().unwrap_or("");
    let primary = match column {
        ConnectionSortColumn::Local => compare_text(local_ip_a, local_ip_b)
            .then_with(|| compare_numeric_text(local_port_a, local_port_b)),
        ConnectionSortColumn::Foreign => compare_text(foreign_ip_a, foreign_ip_b)
            .then_with(|| compare_numeric_text(foreign_port_a, foreign_port_b)),
        ConnectionSortColumn::State => compare_text(state_a, state_b),
        ConnectionSortColumn::Proto => compare_text(proto_a, proto_b),
    };

    primary
        .then_with(|| compare_text(local_ip_a, local_ip_b))
        .then_with(|| compare_numeric_text(local_port_a, local_port_b))
        .then_with(|| compare_text(foreign_ip_a, foreign_ip_b))
        .then_with(|| compare_numeric_text(foreign_port_a, foreign_port_b))
        .then_with(|| compare_text(state_a, state_b))
        .then_with(|| compare_text(proto_a, proto_b))
}

fn compare_numeric_text(a: &str, b: &str) -> std::cmp::Ordering {
    match (a.parse::<u64>(), b.parse::<u64>()) {
        (Ok(a), Ok(b)) => a.cmp(&b),
        _ => compare_text(a, b),
    }
}

fn compare_text(a: &str, b: &str) -> std::cmp::Ordering {
    a.to_lowercase().cmp(&b.to_lowercase())
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
