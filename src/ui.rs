use std::{fs, process::Command, sync::OnceLock};

use crate::app::{
    App, ConnectionSortColumn, NavigationItem, PortSortColumn, SortDirection, ViewMode,
};
use crate::model::{
    InterfaceStatus, NetworkKind, ProcessMetrics, RouteDiagnosticSeverity, RouteFamily,
    RouteInspectorSection, RouteSortColumn, Subnet,
};
use crate::route_inspector::diagnostics::is_default_route;
use crate::route_inspector::graph::{build_route_graph, render_route_graph_lines};
use crate::route_inspector::vpn::is_vpn_interface_name;
use chrono::{DateTime, Local};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Cell, Clear, List, ListItem, Paragraph, Row, Sparkline, Table, Wrap,
    },
    Frame,
};

pub fn render_title() -> &'static str {
    "lazyifconfig"
}

fn header_line(app: &App) -> Line<'static> {
    let mut spans = vec![
        Span::styled(
            "🦥 Lazyifconfig",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" - ", Style::default().fg(Color::DarkGray)),
        Span::styled(os_display_label(), Style::default().fg(Color::White)),
    ];

    if let Some(metrics) = app.process_metrics.as_ref() {
        if let Some(summary) = format_process_metrics(metrics) {
            spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
            spans.push(Span::styled(summary, Style::default().fg(Color::LightCyan)));
        }
    }

    Line::from(spans)
}

fn format_process_metrics(metrics: &ProcessMetrics) -> Option<String> {
    let mut parts = Vec::new();

    if let Some(cpu_tenths) = metrics.cpu_usage_tenths {
        parts.push(format!("CPU {}%", format_tenths(cpu_tenths)));
    }

    if let Some(rss) = metrics.memory_rss_bytes {
        parts.push(format!("MEM {}", format_bytes(rss)));
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" | "))
    }
}

fn format_tenths(value: u16) -> String {
    format!("{}.{:01}", value / 10, value % 10)
}

fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;

    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.1}GB", bytes as f64 / GIB)
    } else if bytes >= 1024 * 1024 {
        format!("{:.1}MB", bytes as f64 / MIB)
    } else if bytes >= 1024 {
        format!("{:.1}KB", bytes as f64 / KIB)
    } else {
        format!("{bytes}B")
    }
}

fn os_display_label() -> &'static str {
    static OS_LABEL: OnceLock<String> = OnceLock::new();
    OS_LABEL.get_or_init(detect_os_label).as_str()
}

fn detect_os_label() -> String {
    if cfg!(target_os = "macos") {
        let version = Command::new("sw_vers")
            .arg("-productVersion")
            .output()
            .ok()
            .and_then(|output| {
                if output.status.success() {
                    String::from_utf8(output.stdout).ok()
                } else {
                    None
                }
            })
            .map(|version| version.trim().to_string())
            .filter(|version| !version.is_empty());

        if let Some(version) = version {
            format!("macOS {version}")
        } else {
            "macOS".to_string()
        }
    } else if cfg!(target_os = "linux") {
        linux_os_label().unwrap_or_else(|| "Linux".to_string())
    } else {
        std::env::consts::OS.to_string()
    }
}

fn linux_os_label() -> Option<String> {
    let os_release = fs::read_to_string("/etc/os-release").ok()?;
    let pretty_name = os_release_value(&os_release, "PRETTY_NAME");
    let version = os_release_value(&os_release, "VERSION_ID");

    pretty_name
        .or_else(|| version.map(|version| format!("Linux {version}")))
        .filter(|label| !label.is_empty())
}

fn os_release_value(contents: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    contents.lines().find_map(|line| {
        let value = line.strip_prefix(&prefix)?;
        Some(value.trim_matches('"').to_string())
    })
}

fn get_active_command(view_mode: ViewMode) -> &'static str {
    match view_mode {
        ViewMode::Interface | ViewMode::Network => {
            if cfg!(target_os = "linux") {
                "ip -details -statistics address show"
            } else {
                "ifconfig"
            }
        }
        ViewMode::Connections => "netstat -an",
        ViewMode::Ports => {
            if cfg!(target_os = "linux") {
                "ss -H -ltnp"
            } else {
                "lsof -iTCP -sTCP:LISTEN -P -n"
            }
        }
        ViewMode::Routes => {
            if cfg!(target_os = "linux") {
                "ip route show"
            } else {
                "netstat -rn"
            }
        }
        ViewMode::Tools => "tool-runner",
        ViewMode::Timeline => "event-logger",
    }
}

fn get_status_text(app: &App) -> String {
    match app.view_mode {
        ViewMode::Connections => {
            if app.connection_filter_active {
                " filter: type | Enter apply | Esc clear | Backspace delete ".to_string()
            } else {
                format!(
                    " q | / filter | s sort | S dir | c copy | w whois | sort:{} | [/] ",
                    connection_sort_label(app)
                )
            }
        }
        ViewMode::Ports => {
            if app.port_filter_active {
                " filter: type | Enter apply | Esc clear | Backspace delete ".to_string()
            } else {
                format!(
                    " q | r | / filter | s sort | S dir | K kill | sort:{} | [/] | i/n/c/e/g ",
                    port_sort_label(app)
                )
            }
        }
        ViewMode::Timeline => {
            " q | u check | U update | R notes | S save | [/] | i/n/c/p/g | j/k ".to_string()
        }
        ViewMode::Routes => {
            if app.route_inspector.route_filter_active {
                " filter routes: type | Enter apply | Esc clear | Backspace delete ".to_string()
            } else if app.route_inspector.destination_input_active {
                " destination: type | Enter lookup | Esc cancel | Backspace delete ".to_string()
            } else {
                format!(
                    " q | u check | U update | R notes | Enter path | Tab section | / filter | s sort | S dir | o raw | sort:{} | i/n/c/p/e ",
                    route_sort_label(app)
                )
            }
        }
        ViewMode::Tools => {
            if app.tools.input_modal_open {
                " input modal | type | Backspace | Tab field | Enter run | Esc cancel ".to_string()
            } else if app.tools.selected_tool_is_dns_lookup() {
                " q | Enter input | / input | r rerun | o raw | [/] scroll | i/n/p/c/g/e "
                    .to_string()
            } else {
                " q | Enter input | / input | r rerun | [/] scroll | i/n/p/c/g/e ".to_string()
            }
        }
        _ => {
            format!(
                " q | r | u check | U update | R notes | a:{} | i/n/c/p/e/g ",
                if app.show_all { "on" } else { "off" }
            )
        }
    }
}

fn port_sort_label(app: &App) -> String {
    format!(
        "{} {}",
        match app.port_sort_column {
            PortSortColumn::Port => "Port",
            PortSortColumn::Command => "Command",
            PortSortColumn::Pid => "PID",
            PortSortColumn::User => "User",
            PortSortColumn::Proto => "Proto",
        },
        match app.port_sort_direction {
            SortDirection::Ascending => "asc",
            SortDirection::Descending => "desc",
        }
    )
}

fn connection_sort_label(app: &App) -> String {
    format!(
        "{} {}",
        match app.connection_sort_column {
            ConnectionSortColumn::Local => "Local",
            ConnectionSortColumn::Foreign => "Foreign",
            ConnectionSortColumn::State => "State",
            ConnectionSortColumn::Proto => "Proto",
        },
        match app.connection_sort_direction {
            SortDirection::Ascending => "asc",
            SortDirection::Descending => "desc",
        }
    )
}

fn route_sort_label(app: &App) -> String {
    format!(
        "{} {}",
        match app.route_inspector.sort_column {
            RouteSortColumn::Destination => "Destination",
            RouteSortColumn::Gateway => "Gateway",
            RouteSortColumn::Interface => "Interface",
            RouteSortColumn::Metric => "Metric",
        },
        match app.route_inspector.route_sort_direction {
            SortDirection::Ascending => "asc",
            SortDirection::Descending => "desc",
        }
    )
}

fn view_tabs(view_mode: ViewMode) -> Line<'static> {
    let tabs = [
        (ViewMode::Interface, "Interface(i)"),
        (ViewMode::Network, "Network(n)"),
        (ViewMode::Ports, "Port(p)"),
        (ViewMode::Connections, "Connection(c)"),
        (ViewMode::Routes, "Route(g)"),
        (ViewMode::Tools, "Tools(t)"),
        (ViewMode::Timeline, "Timeline(e)"),
    ];

    let mut spans = Vec::new();
    for (idx, (mode, label)) in tabs.iter().enumerate() {
        if idx > 0 {
            spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
        }

        let style = if *mode == view_mode {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        spans.push(Span::styled(format!(" {label} "), style));
    }

    Line::from(spans)
}

pub fn draw(frame: &mut Frame, app: &App) {
    let filter_bar_height: u16 = if app.port_filter_active
        || app.connection_filter_active
        || (app.view_mode == ViewMode::Ports && !app.port_filter.is_empty())
        || (app.view_mode == ViewMode::Connections && !app.connection_filter.is_empty())
    {
        1
    } else {
        0
    };
    let command_panel_height = command_panel_height(app);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),                    // 0: App Header
            Constraint::Length(1),                    // 1: View Tabs
            Constraint::Min(3),                       // 2: Top pane
            Constraint::Length(command_panel_height), // 3: Active Command Panel
            Constraint::Length(5),                    // 4: Recent Events Panel
            Constraint::Length(filter_bar_height),    // 5: Filter Bar
            Constraint::Length(1),                    // 6: Status Bar
        ])
        .split(frame.size());

    let header =
        Paragraph::new(header_line(app)).style(Style::default().bg(Color::Rgb(24, 24, 24)));
    frame.render_widget(header, chunks[0]);

    let tabs =
        Paragraph::new(view_tabs(app.view_mode)).style(Style::default().bg(Color::Rgb(32, 32, 32)));
    frame.render_widget(tabs, chunks[1]);

    let top_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(top_chunks_area(chunks[2]));

    // Helper to extract size safely (compatible with older/newer ratatui area/size)
    fn top_chunks_area(area: ratatui::layout::Rect) -> ratatui::layout::Rect {
        area
    }

    // 1. Left Pane: Interfaces or Subnets list
    let title = match app.view_mode {
        ViewMode::Interface => " Interfaces ",
        ViewMode::Network => " Networks (Subnet View) ",
        ViewMode::Connections => " Active Connections ",
        ViewMode::Ports => " Listening Ports ",
        ViewMode::Timeline => " Event Timeline ",
        ViewMode::Routes => " Routes ",
        ViewMode::Tools => " Tools ",
    };
    let list_block = Block::default().borders(Borders::ALL).title(title);

    let mut list_items = Vec::new();
    for (idx, item) in app.navigation_items.iter().enumerate() {
        let style = if idx == app.selected_index {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        match item {
            NavigationItem::SubnetHeader(subnet) => {
                let text = match subnet {
                    Subnet::Ipv4 {
                        network,
                        prefix_len,
                    } => format!("▼ {}/{}", network, prefix_len),
                    Subnet::Ipv6 {
                        network,
                        prefix_len,
                    } => format!("▼ {}/{}", network, prefix_len),
                    Subnet::Unassigned => "▼ Unassigned / No IP".to_string(),
                };
                let header_style = if idx == app.selected_index {
                    style
                } else {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                };
                list_items.push(ListItem::new(text).style(header_style));
            }
            NavigationItem::Interface {
                name,
                associated_ip,
            } => {
                let mut status_indicator = "○";
                let mut is_up = false;
                let mut kind = NetworkKind::Unknown;

                if let Some(snapshot) = &app.current_snapshot {
                    if let Some(interface) = snapshot.interfaces.iter().find(|i| i.name == *name) {
                        is_up = interface.status == InterfaceStatus::Up;
                        status_indicator = if is_up { "●" } else { "○" };
                        kind = interface.network_kind;
                    }
                }

                let mut display_text = if app.view_mode == ViewMode::Network {
                    format!(
                        "  {} {} ({})",
                        status_indicator,
                        name,
                        associated_ip.as_deref().unwrap_or("no IP")
                    )
                } else {
                    format!(
                        "{} {} ({})",
                        status_indicator,
                        name,
                        associated_ip.as_deref().unwrap_or("no IP")
                    )
                };

                // Add padding to display classification right-aligned nicely
                let padding = 35_usize.saturating_sub(display_text.chars().count());
                display_text.push_str(&" ".repeat(padding));
                display_text.push_str(kind.as_str());

                let mut final_style = style;
                if !is_up {
                    if idx == app.selected_index {
                        final_style = final_style.add_modifier(Modifier::DIM);
                    } else {
                        final_style = final_style.fg(Color::DarkGray);
                    }
                }
                list_items.push(ListItem::new(display_text).style(final_style));
            }
            NavigationItem::Connection {
                proto,
                local_ip,
                local_port,
                foreign_ip,
                foreign_port,
                state,
                ..
            } => {
                let state_str = state
                    .as_ref()
                    .map(|s| format!(" ({})", s))
                    .unwrap_or_default();
                let text = format!(
                    "[{}] {} -> {}{}",
                    proto.to_uppercase(),
                    format_endpoint(local_ip, local_port),
                    format_endpoint(foreign_ip, foreign_port),
                    state_str
                );
                list_items.push(ListItem::new(text).style(style));
            }
            NavigationItem::ListeningPort {
                proto,
                port,
                command,
                pid,
                ..
            } => {
                let text = format!(
                    "[{}] :{:<6} {} (PID: {})",
                    proto.to_uppercase(),
                    port,
                    command,
                    pid
                );
                list_items.push(ListItem::new(text).style(style));
            }
            NavigationItem::Event {
                index,
                kind,
                timestamp,
                message,
            } => {
                let datetime: DateTime<Local> = (*timestamp).into();
                let time_str = datetime.format("%H:%M:%S").to_string();
                let text = format!("{} [{}] {}", time_str, kind.as_str(), message);

                // Color code based on severity
                let mut item_style = style;
                if idx != app.selected_index {
                    if let Some(event) = app.recent_events.get(*index) {
                        match event.severity {
                            crate::model::EventSeverity::Warning => {
                                item_style = item_style.fg(Color::Yellow)
                            }
                            crate::model::EventSeverity::Error => {
                                item_style = item_style.fg(Color::Red)
                            }
                            crate::model::EventSeverity::Info => {}
                        }
                    }
                }
                list_items.push(ListItem::new(text).style(item_style));
            }
            NavigationItem::Route {
                destination,
                gateway,
                interface,
                index,
            } => {
                let text = format!("{:<18} {:<16} {}", destination, gateway, interface);
                let route_style = app
                    .current_snapshot
                    .as_ref()
                    .and_then(|snapshot| snapshot.routes.get(*index))
                    .map(|route| {
                        if is_default_route(route) {
                            Style::default()
                                .fg(Color::Green)
                                .add_modifier(Modifier::BOLD)
                        } else if is_vpn_interface_name(&route.interface) {
                            Style::default().fg(Color::Yellow)
                        } else {
                            Style::default()
                        }
                    })
                    .unwrap_or_default();
                let final_style = if idx == app.selected_index {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    route_style
                };
                list_items.push(ListItem::new(text).style(final_style));
            }
        }
    }
    if app.view_mode == ViewMode::Tools {
        render_tools_view(frame, app, top_chunks[0], top_chunks[1]);
    } else if app.view_mode == ViewMode::Ports {
        render_ports_table(frame, app, list_block, top_chunks[0]);
    } else if app.view_mode == ViewMode::Connections {
        render_connections_table(frame, app, list_block, top_chunks[0]);
    } else {
        let list_widget = List::new(list_items).block(list_block);
        frame.render_widget(list_widget, top_chunks[0]);
    }

    // 2. Right Pane: Details Panel
    if app.view_mode != ViewMode::Tools {
        let details_block = Block::default().borders(Borders::ALL).title(" Details ");

        let details_inner = details_block.inner(top_chunks[1]);
        frame.render_widget(details_block, top_chunks[1]);

        if app.view_mode == ViewMode::Routes {
            render_route_inspector_details(frame, app, details_inner);
        } else if let Some(selected_item) = app.navigation_items.get(app.selected_index) {
            match selected_item {
                NavigationItem::SubnetHeader(subnet) => {
                    let mut details_text = String::new();
                    details_text.push_str("=== Subnet Information ===\n\n");
                    match subnet {
                        Subnet::Ipv4 {
                            network,
                            prefix_len,
                        } => {
                            details_text.push_str(&format!("Protocol:       IPv4\n"));
                            details_text.push_str(&format!("Network Addr:   {}\n", network));
                            details_text.push_str(&format!("Prefix Length:  {}\n", prefix_len));
                            details_text.push_str(&format!(
                                "Subnet Mask:    {}\n",
                                prefix_len_to_ipv4_mask(*prefix_len)
                            ));
                        }
                        Subnet::Ipv6 {
                            network,
                            prefix_len,
                        } => {
                            details_text.push_str(&format!("Protocol:       IPv6\n"));
                            details_text.push_str(&format!("Network Addr:   {}\n", network));
                            details_text.push_str(&format!("Prefix Length:  {}\n", prefix_len));
                        }
                        Subnet::Unassigned => {
                            details_text.push_str("Protocol:       N/A\n");
                            details_text.push_str(
                                "Description:    Interfaces without an IP Address assigned.\n",
                            );
                        }
                    }

                    details_text.push_str("\nMember Interfaces:\n");
                    if let Some(snapshot) = &app.current_snapshot {
                        for interface in &snapshot.interfaces {
                            let mut matches_subnet = false;
                            let mut ip_val = "no IP".to_string();

                            match subnet {
                                Subnet::Ipv4 {
                                    network,
                                    prefix_len,
                                } => {
                                    for addr in &interface.ipv4 {
                                        if let Some(p) = addr.prefix_len {
                                            if p == *prefix_len {
                                                if let Ok(ip) =
                                                    addr.value.parse::<std::net::Ipv4Addr>()
                                                {
                                                    let net_ip =
                                                        calculate_ipv4_subnet_u32(u32::from(ip), p);
                                                    if net_ip == *network {
                                                        matches_subnet = true;
                                                        ip_val = addr.value.clone();
                                                        break;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                Subnet::Ipv6 {
                                    network,
                                    prefix_len,
                                } => {
                                    for addr in &interface.ipv6 {
                                        if let Some(p) = addr.prefix_len {
                                            if p == *prefix_len {
                                                if let Ok(ip) =
                                                    addr.value.parse::<std::net::Ipv6Addr>()
                                                {
                                                    let net_ip = calculate_ipv6_subnet_arr(&ip, p);
                                                    if net_ip == *network {
                                                        matches_subnet = true;
                                                        ip_val = addr.value.clone();
                                                        break;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                Subnet::Unassigned => {
                                    let has_ipv4 =
                                        interface.ipv4.iter().any(|a| a.prefix_len.is_some());
                                    let has_ipv6 =
                                        interface.ipv6.iter().any(|a| a.prefix_len.is_some());
                                    if !has_ipv4 && !has_ipv6 {
                                        matches_subnet = true;
                                    }
                                }
                            }

                            if matches_subnet {
                                details_text
                                    .push_str(&format!("  - {} ({})\n", interface.name, ip_val));
                            }
                        }
                    }

                    let details_p = Paragraph::new(details_text)
                        .wrap(Wrap { trim: true })
                        .scroll((app.details_scroll, 0));
                    frame.render_widget(details_p, details_inner);
                }
                NavigationItem::Interface { name, .. } => {
                    if let Some(snapshot) = &app.current_snapshot {
                        if let Some(interface) =
                            snapshot.interfaces.iter().find(|i| i.name == *name)
                        {
                            let sub_chunks = Layout::default()
                                .direction(Direction::Vertical)
                                .constraints([Constraint::Min(5), Constraint::Length(6)])
                                .split(details_inner);

                            let mut details_text = String::new();
                            details_text.push_str(&format!("Name:           {}\n", interface.name));
                            details_text.push_str(&format!(
                                "Classification: {}\n",
                                interface.network_kind.as_str()
                            ));
                            details_text.push_str(&format!(
                                "Status:         {}\n",
                                match interface.status {
                                    InterfaceStatus::Up => "Active / Up",
                                    InterfaceStatus::Down => "Inactive / Down",
                                }
                            ));
                            details_text.push_str(&format!(
                                "MAC Address:    {}\n",
                                interface.mac_address.as_deref().unwrap_or("N/A")
                            ));
                            details_text.push_str(&format!(
                                "MTU:            {}\n",
                                interface
                                    .mtu
                                    .map(|m| m.to_string())
                                    .unwrap_or_else(|| "N/A".to_string())
                            ));

                            details_text.push_str("\nIPv4 Addresses:\n");
                            for addr in &interface.ipv4 {
                                let gw_str = addr
                                    .gateway
                                    .as_ref()
                                    .map(|g| format!(" (Gateway: {})", g))
                                    .unwrap_or_default();
                                details_text.push_str(&format!(
                                    "  - {} / {}{}\n",
                                    addr.value,
                                    addr.prefix_len
                                        .map(|p| p.to_string())
                                        .unwrap_or_else(|| "?".to_string()),
                                    gw_str
                                ));
                            }
                            details_text.push_str("IPv6 Addresses:\n");
                            for addr in &interface.ipv6 {
                                let gw_str = addr
                                    .gateway
                                    .as_ref()
                                    .map(|g| format!(" (Gateway: {})", g))
                                    .unwrap_or_default();
                                details_text.push_str(&format!(
                                    "  - {} / {}{}\n",
                                    addr.value,
                                    addr.prefix_len
                                        .map(|p| p.to_string())
                                        .unwrap_or_else(|| "?".to_string()),
                                    gw_str
                                ));
                            }

                            details_text.push_str("\nTraffic Cumulative Stats:\n");
                            if let Some(stats) = &interface.stats {
                                details_text.push_str(&format!(
                                    "  Packets: RX {} / TX {}\n",
                                    stats.rx_packets, stats.tx_packets
                                ));
                                details_text.push_str(&format!(
                                    "  Bytes:   RX {} / TX {}\n",
                                    stats.rx_bytes, stats.tx_bytes
                                ));
                            } else {
                                details_text.push_str("  No stats available\n");
                            }

                            let details_p = Paragraph::new(details_text)
                                .wrap(Wrap { trim: true })
                                .scroll((app.details_scroll, 0));
                            frame.render_widget(details_p, sub_chunks[0]);

                            // Render Charts
                            let chart_chunks = Layout::default()
                                .direction(Direction::Horizontal)
                                .constraints([
                                    Constraint::Percentage(50),
                                    Constraint::Percentage(50),
                                ])
                                .split(sub_chunks[1]);

                            let (rx_rate, tx_rate) = app.selected_rates().unwrap_or((0, 0));

                            let mut rx_data = vec![0u64; 40];
                            let mut tx_data = vec![0u64; 40];
                            if let Some(history) = app.traffic_history.get(&interface.name) {
                                let rx_len = history.rx_rates.len();
                                let rx_start = 40_usize.saturating_sub(rx_len);
                                for (i, &val) in history.rx_rates.iter().enumerate() {
                                    if rx_start + i < 40 {
                                        rx_data[rx_start + i] = val;
                                    }
                                }
                                let tx_len = history.tx_rates.len();
                                let tx_start = 40_usize.saturating_sub(tx_len);
                                for (i, &val) in history.tx_rates.iter().enumerate() {
                                    if tx_start + i < 40 {
                                        tx_data[tx_start + i] = val;
                                    }
                                }
                            }

                            let rx_title = format!(" RX Rate: {} ", format_bps(rx_rate));
                            let tx_title = format!(" TX Rate: {} ", format_bps(tx_rate));

                            let rx_sparkline = Sparkline::default()
                                .block(Block::default().borders(Borders::ALL).title(rx_title))
                                .style(Style::default().fg(Color::Green))
                                .data(&rx_data);

                            let tx_sparkline = Sparkline::default()
                                .block(Block::default().borders(Borders::ALL).title(tx_title))
                                .style(Style::default().fg(Color::Yellow))
                                .data(&tx_data);

                            frame.render_widget(rx_sparkline, chart_chunks[0]);
                            frame.render_widget(tx_sparkline, chart_chunks[1]);
                        }
                    }
                }
                NavigationItem::Connection {
                    proto,
                    local_ip,
                    local_port,
                    foreign_ip,
                    foreign_port,
                    state,
                    index: _,
                } => {
                    let mut details_text = String::new();
                    details_text.push_str("=== Active Connection Details ===\n\n");
                    details_text
                        .push_str(&format!("Protocol:             {}\n", proto.to_uppercase()));

                    details_text.push_str(&format!("Local IP Address:     {}\n", local_ip));
                    details_text.push_str(&format!("Local Port:           {}\n", local_port));

                    details_text.push_str(&format!("Foreign IP Address:   {}\n", foreign_ip));
                    details_text.push_str(&format!("Foreign Port:         {}\n", foreign_port));

                    if let Some(s) = state {
                        details_text.push_str(&format!("TCP State:            {}\n", s));
                    }

                    // Map local IP to local interfaces
                    let mut mapped_interface = "N/A (External/Wildcard)".to_string();
                    if let Some(snapshot) = &app.current_snapshot {
                        for interface in &snapshot.interfaces {
                            let matches_ipv4 =
                                interface.ipv4.iter().any(|addr| addr.value == *local_ip);
                            let matches_ipv6 =
                                interface.ipv6.iter().any(|addr| addr.value == *local_ip);
                            if matches_ipv4 || matches_ipv6 {
                                mapped_interface = format!(
                                    "{} ({})",
                                    interface.name,
                                    interface.network_kind.as_str()
                                );
                                break;
                            }
                        }
                    }
                    if local_ip == "127.0.0.1" || local_ip == "::1" || local_ip == "fe80::1%lo0" {
                        mapped_interface = "lo0 (LOOPBACK)".to_string();
                    } else if local_ip == "*" || local_ip == "::" || local_ip == "0.0.0.0" {
                        mapped_interface = "All Interfaces (Wildcard)".to_string();
                    }

                    details_text.push_str(&format!("Associated Interface: {}\n", mapped_interface));

                    if foreign_ip != "*"
                        && foreign_ip != "::"
                        && foreign_ip != "0.0.0.0"
                        && foreign_ip != "*.*"
                    {
                        details_text.push_str("\n[c: Copy IP | w: WHOIS Query]\n");
                        if let Some(whois) = app.get_whois_result(foreign_ip) {
                            details_text.push_str("\n=== Whois Information ===\n");
                            details_text.push_str(&whois);
                        } else {
                            details_text.push_str("\nPress 'w' to fetch WHOIS information.\n");
                        }
                    }

                    let mut ui_lines = Vec::new();
                    let mut in_whois_section = false;
                    for line in details_text.lines() {
                        if line.contains("=== Whois Information ===") {
                            in_whois_section = true;
                        }

                        let is_highlight = in_whois_section && {
                            let lower = line.to_lowercase();
                            lower.contains("origin") || lower.contains("org")
                        };

                        if is_highlight {
                            ui_lines.push(Line::from(Span::styled(
                                line.to_string(),
                                Style::default()
                                    .fg(Color::Yellow)
                                    .add_modifier(Modifier::BOLD),
                            )));
                        } else {
                            ui_lines.push(Line::from(line.to_string()));
                        }
                    }

                    let details_p = Paragraph::new(ui_lines)
                        .wrap(Wrap { trim: true })
                        .scroll((app.details_scroll, 0));
                    frame.render_widget(details_p, details_inner);
                }
                NavigationItem::ListeningPort {
                    proto,
                    port,
                    command,
                    pid,
                    user,
                    ..
                } => {
                    let mut lines = Vec::new();
                    lines.push(Line::from(Span::styled(
                        "=== Listening Port Details ===",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    )));
                    lines.push(Line::from(""));
                    lines.push(Line::from(vec![
                        Span::styled(
                            "Protocol:   ",
                            Style::default().add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(proto.to_uppercase()),
                    ]));
                    lines.push(Line::from(vec![
                        Span::styled(
                            "Port:       ",
                            Style::default().add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            port.as_str(),
                            Style::default()
                                .fg(Color::Yellow)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]));
                    lines.push(Line::from(""));
                    lines.push(Line::from(Span::styled(
                        "=== Process Information ===",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    )));
                    lines.push(Line::from(""));
                    lines.push(Line::from(vec![
                        Span::styled(
                            "Command:    ",
                            Style::default().add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(command.as_str(), Style::default().fg(Color::Green)),
                    ]));
                    lines.push(Line::from(vec![
                        Span::styled(
                            "PID:        ",
                            Style::default().add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(pid.as_str()),
                    ]));
                    lines.push(Line::from(vec![
                        Span::styled(
                            "User:       ",
                            Style::default().add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(user.as_str()),
                    ]));

                    let details_p = Paragraph::new(lines)
                        .wrap(Wrap { trim: true })
                        .scroll((app.details_scroll, 0));
                    frame.render_widget(details_p, details_inner);
                }
                NavigationItem::Event {
                    index,
                    kind,
                    timestamp,
                    message,
                } => {
                    let mut lines = Vec::new();
                    lines.push(Line::from(Span::styled(
                        "=== Event Details ===",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    )));
                    lines.push(Line::from(""));
                    lines.push(Line::from(vec![
                        Span::styled(
                            "Type:        ",
                            Style::default().add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            kind.as_str(),
                            Style::default()
                                .fg(Color::Yellow)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]));

                    let datetime: DateTime<Local> = (*timestamp).into();
                    let time_str = datetime.format("%Y-%m-%d %H:%M:%S").to_string();
                    lines.push(Line::from(vec![
                        Span::styled(
                            "Time:        ",
                            Style::default().add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(time_str),
                    ]));

                    let severity_str = if let Some(event) = app.recent_events.get(*index) {
                        event.severity.as_str()
                    } else {
                        "INFO"
                    };
                    let severity_color = match severity_str {
                        "WARNING" => Color::Yellow,
                        "ERROR" => Color::Red,
                        _ => Color::Green,
                    };
                    lines.push(Line::from(vec![
                        Span::styled(
                            "Severity:    ",
                            Style::default().add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            severity_str,
                            Style::default()
                                .fg(severity_color)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]));

                    lines.push(Line::from(""));
                    lines.push(Line::from(vec![
                        Span::styled(
                            "Description: ",
                            Style::default().add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(message.as_str()),
                    ]));

                    let impact = match kind {
                    crate::model::NetworkEventKind::VpnConnected => "Traffic may be routed through VPN. Default routes might change.",
                    crate::model::NetworkEventKind::VpnDisconnected => "VPN connection lost. Traffic will not be routed through VPN.",
                    crate::model::NetworkEventKind::ContainerNetworkAppeared => "Local container networking active. Container services might be reachable.",
                    crate::model::NetworkEventKind::ContainerNetworkRemoved => "Local container networking inactive.",
                    crate::model::NetworkEventKind::InterfaceAppeared => "New network interface is discovered and registered.",
                    crate::model::NetworkEventKind::InterfaceRemoved => "Interface has been removed or disabled.",
                    crate::model::NetworkEventKind::InterfaceUp => "Network interface is now active and up.",
                    crate::model::NetworkEventKind::InterfaceDown => "Network interface is inactive. No traffic can flow.",
                    crate::model::NetworkEventKind::Ipv4Added | crate::model::NetworkEventKind::Ipv6Added => "IP address assigned. Communications on this subnet are now enabled.",
                    crate::model::NetworkEventKind::Ipv4Removed | crate::model::NetworkEventKind::Ipv6Removed => "IP address unassigned. Host loses addressability on this subnet.",
                    crate::model::NetworkEventKind::Ipv4Changed | crate::model::NetworkEventKind::Ipv6Changed => "IP address has changed. Active sockets on this interface might drop.",
                    crate::model::NetworkEventKind::ProcessKilled => "The process holding the listening port has been terminated. Port is now free.",
                    crate::model::NetworkEventKind::ActionCopied => "An IP address has been successfully copied to your system clipboard.",
                    crate::model::NetworkEventKind::ActionWhois => "WHOIS query initiated to fetch metadata for the foreign IP address.",
                    crate::model::NetworkEventKind::SystemError => "A command or system level call returned an error status.",
                    crate::model::NetworkEventKind::PublicIpChanged => "Your public IP address has changed. Network route or VPN activation might have occurred.",
                    crate::model::NetworkEventKind::ProviderChanged => "Your ISP or network provider has changed. Active routing paths updated.",
                    crate::model::NetworkEventKind::UpdateAvailable => "A newer GitHub release was found and is ready to install.",
                    crate::model::NetworkEventKind::UpdateInstalled => "A new binary has been installed. Restart the app to run the updated version.",
                    crate::model::NetworkEventKind::UpdateCheckFailed => "The GitHub release check or install step failed.",
                    crate::model::NetworkEventKind::TimelineExported => "Timeline was saved to disk for offline review.",
                };
                    lines.push(Line::from(""));
                    lines.push(Line::from(Span::styled(
                        "=== Expected Impact ===",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    )));
                    lines.push(Line::from(""));
                    lines.push(Line::from(Span::raw(impact)));

                    let details_p = Paragraph::new(lines)
                        .wrap(Wrap { trim: true })
                        .scroll((app.details_scroll, 0));
                    frame.render_widget(details_p, details_inner);
                }
                NavigationItem::Route { .. } => {}
            }
        } else {
            let details_p = Paragraph::new("No data collected yet. Press 'r' to refresh.")
                .wrap(Wrap { trim: true })
                .scroll((app.details_scroll, 0));
            frame.render_widget(details_p, details_inner);
        }
    }

    // 3. Active Command Panel
    let (command_lines, command_style) = build_command_panel(app);
    let command_p = Paragraph::new(command_lines)
        .style(command_style)
        .wrap(Wrap { trim: true });
    frame.render_widget(command_p, chunks[3]);

    // 4. Event Panel
    let event_block = Block::default()
        .borders(Borders::ALL)
        .title(" Recent Events ");
    let mut event_items = Vec::new();
    for event in app.recent_events.iter().rev().take(10) {
        let datetime: DateTime<Local> = event.timestamp.into();
        let time_str = datetime.format("%H:%M:%S").to_string();

        let mut item_style = Style::default();
        match event.severity {
            crate::model::EventSeverity::Warning => item_style = item_style.fg(Color::Yellow),
            crate::model::EventSeverity::Error => item_style = item_style.fg(Color::Red),
            _ => {}
        }

        event_items
            .push(ListItem::new(format!("[{}] {}", time_str, event.message)).style(item_style));
    }
    let event_list = List::new(event_items).block(event_block);
    frame.render_widget(event_list, chunks[4]);

    // 5. Filter Bar
    if filter_bar_height > 0 {
        let (filter_value, filter_active) = if app.view_mode == ViewMode::Connections {
            (app.connection_filter.as_str(), app.connection_filter_active)
        } else {
            (app.port_filter.as_str(), app.port_filter_active)
        };
        let filter_text = if filter_active {
            format!(" 🔍 Filter: {}▌", filter_value)
        } else {
            format!(" 🔍 Filter: {}  (/: edit, Esc: clear)", filter_value)
        };
        let filter_style = if filter_active {
            Style::default().bg(Color::DarkGray).fg(Color::Yellow)
        } else {
            Style::default().bg(Color::DarkGray).fg(Color::White)
        };
        let filter_p = Paragraph::new(filter_text).style(filter_style);
        frame.render_widget(filter_p, chunks[5]);
    }

    // 6. Status Bar
    let status_idx = 6;
    let status_text = get_status_text(app);
    let status_p = Paragraph::new(status_text).style(
        Style::default()
            .bg(Color::Black)
            .fg(Color::LightYellow)
            .add_modifier(Modifier::BOLD),
    );
    frame.render_widget(status_p, chunks[status_idx]);

    if app.help_visible {
        draw_help(frame);
    }

    if app.release_notes_viewer.active {
        draw_release_notes_viewer(frame, app);
    }

    if app.raw_viewer.active {
        draw_raw_viewer(frame, app);
    }

    if app.view_mode == ViewMode::Tools && app.tools.input_modal_open {
        render_tools_input_modal(frame, app);
    }
}

fn render_tools_view(frame: &mut Frame, app: &App, list_area: Rect, details_area: Rect) {
    let mut tool_items = Vec::new();
    for (idx, definition) in app.tools.registry.definitions().iter().enumerate() {
        let selected = idx == app.tools.selected_index;
        let mut style = if selected {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let suffix = match definition.availability {
            crate::tools::ToolAvailability::Runnable => "",
            crate::tools::ToolAvailability::Planned => " (planned)",
        };
        if definition.availability == crate::tools::ToolAvailability::Planned && !selected {
            style = style.fg(Color::DarkGray);
        }
        tool_items.push(ListItem::new(format!("{}{}", definition.name, suffix)).style(style));
    }

    let tool_list =
        List::new(tool_items).block(Block::default().borders(Borders::ALL).title(" Tools "));
    frame.render_widget(tool_list, list_area);

    let definition = app.tools.selected_definition();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length((definition.fields.len() as u16).saturating_add(4).max(5)),
            Constraint::Percentage(38),
            Constraint::Percentage(42),
        ])
        .split(details_area);

    let mut input_lines = Vec::new();
    input_lines.push(Line::from(Span::styled(
        definition.description,
        Style::default().fg(Color::White),
    )));
    input_lines.push(Line::from(""));

    let selected_input = app.tools.inputs.get(&definition.id);
    for (idx, field) in definition.fields.iter().enumerate() {
        let value = selected_input
            .and_then(|input| input.values.get(field.key))
            .map(String::as_str)
            .unwrap_or("");
        let shown = if value.is_empty() {
            field.placeholder
        } else {
            value
        };
        let marker = if idx == app.tools.selected_field_index {
            ">"
        } else {
            " "
        };
        let style = if idx == app.tools.selected_field_index && app.tools.editing_input {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        input_lines.push(Line::from(vec![
            Span::styled(format!("{marker} {}: ", field.label), style),
            Span::styled(shown.to_string(), style),
        ]));
    }

    if definition.availability == crate::tools::ToolAvailability::Planned {
        input_lines.push(Line::from(""));
        input_lines.push(Line::from(Span::styled(
            "planned / disabled",
            Style::default().fg(Color::DarkGray),
        )));
    }

    frame.render_widget(
        Paragraph::new(input_lines)
            .wrap(Wrap { trim: true })
            .block(Block::default().borders(Borders::ALL).title(" Input ")),
        chunks[0],
    );

    let state = app
        .tools
        .states
        .get(&definition.id)
        .copied()
        .unwrap_or(crate::tools::ToolExecutionState::Idle);
    let mut result_lines = Vec::new();
    match state {
        crate::tools::ToolExecutionState::Running => {
            result_lines.push(Line::from("Running..."));
        }
        crate::tools::ToolExecutionState::Failed => {
            result_lines.push(Line::from(Span::styled(
                "Error",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            )));
            if let Some(error) = app.tools.errors.get(&definition.id) {
                result_lines.push(Line::from(error.clone()));
            }
        }
        _ => {
            if let Some(result) = app.tools.results.get(&definition.id) {
                for section in &result.sections {
                    result_lines.push(Line::from(Span::styled(
                        section.label.clone(),
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    )));
                    for line in &section.lines {
                        result_lines.push(Line::from(line.clone()));
                    }
                    result_lines.push(Line::from(""));
                }
            } else if definition.availability == crate::tools::ToolAvailability::Runnable {
                result_lines.push(Line::from("Press Enter to edit input, then Enter to run."));
            } else {
                result_lines.push(Line::from("This tool is planned for a follow-up slice."));
            }
        }
    }
    frame.render_widget(
        Paragraph::new(result_lines)
            .wrap(Wrap { trim: true })
            .block(Block::default().borders(Borders::ALL).title(" Results ")),
        chunks[1],
    );

    let raw_output = app
        .tools
        .results
        .get(&definition.id)
        .map(|result| result.raw_output.as_str())
        .unwrap_or("Raw output appears here after a tool runs.");
    let raw_output =
        if definition.id == crate::tools::ToolId::DnsLookup && !app.tools.dns_raw_output_expanded {
            "▶ Show Raw Output\nPress o to expand."
        } else {
            raw_output
        };

    let raw_lines: Vec<Line<'_>> = if raw_output.is_empty() {
        vec![Line::from("")]
    } else {
        let mut iter = raw_output.lines();
        let first = iter.next().unwrap_or("");
        let mut lines = Vec::new();
        let command_style = Style::default()
            .fg(Color::Rgb(0, 255, 102))
            .add_modifier(Modifier::BOLD);
        let output_style = Style::default().fg(Color::Rgb(192, 255, 192));
        if !first.is_empty() {
            if first.starts_with("$ ") {
                lines.push(Line::from(vec![
                    Span::styled("$ ", command_style),
                    Span::styled(&first[2..], command_style),
                ]));
            } else {
                lines.push(Line::styled(first, output_style));
            }
        }
        for line in iter {
            lines.push(Line::styled(line.to_string(), output_style));
        }
        lines
    };
    frame.render_widget(
        Paragraph::new(raw_lines)
            .wrap(Wrap { trim: false })
            .scroll((app.tools.raw_scroll, 0))
            .block(Block::default().borders(Borders::ALL).title(" Raw Output ")),
        chunks[2],
    );
}

fn render_tools_input_modal(frame: &mut Frame, app: &App) {
    let definition = app.tools.selected_definition();
    if definition.fields.is_empty() {
        return;
    }

    let area = centered_rect(62, 46, frame.size());
    frame.render_widget(Clear, area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);

    let selected_input = app.tools.inputs.get(&definition.id);
    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        definition.description,
        Style::default().fg(Color::White),
    )));
    lines.push(Line::from(""));

    for (idx, field) in definition.fields.iter().enumerate() {
        let value = selected_input
            .and_then(|input| input.values.get(field.key))
            .map(String::as_str)
            .unwrap_or("");
        let shown = if value.is_empty() {
            field.placeholder
        } else {
            value
        };
        let marker = if idx == app.tools.selected_field_index {
            ">"
        } else {
            " "
        };
        let style = if idx == app.tools.selected_field_index {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{marker} {}: ", field.label), style),
            Span::styled(shown.to_string(), style),
        ]));
    }

    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(Block::default().borders(Borders::ALL).title(" Tool Input ")),
        area,
    );

    frame.render_widget(
        Paragraph::new("Enter run | Tab next field | Esc cancel")
            .style(Style::default().fg(Color::LightYellow).bg(Color::Black)),
        inner[1],
    );
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

fn render_ports_table(frame: &mut Frame, app: &App, block: Block<'_>, area: Rect) {
    let header = Row::new([
        Cell::from(port_header_label(app, PortSortColumn::Proto, "Proto")),
        Cell::from(port_header_label(app, PortSortColumn::Port, "Port")),
        Cell::from(port_header_label(app, PortSortColumn::Command, "Command")),
        Cell::from(port_header_label(app, PortSortColumn::Pid, "PID")),
        Cell::from(port_header_label(app, PortSortColumn::User, "User")),
    ])
    .style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );

    let rows = app
        .navigation_items
        .iter()
        .enumerate()
        .filter_map(|(idx, item)| {
            let NavigationItem::ListeningPort {
                proto,
                port,
                command,
                pid,
                user,
                ..
            } = item
            else {
                return None;
            };

            let style = if idx == app.selected_index {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            Some(
                Row::new([
                    highlighted_filter_cell(proto.to_uppercase(), &app.port_filter),
                    highlighted_filter_cell(port.clone(), &app.port_filter),
                    highlighted_filter_cell(command.clone(), &app.port_filter),
                    highlighted_filter_cell(pid.clone(), &app.port_filter),
                    highlighted_filter_cell(user.clone(), &app.port_filter),
                ])
                .style(style),
            )
        });

    let table = Table::new(
        rows,
        [
            Constraint::Length(5),
            Constraint::Length(6),
            Constraint::Min(8),
            Constraint::Length(7),
            Constraint::Length(8),
        ],
    )
    .header(header)
    .column_spacing(1)
    .block(block);

    frame.render_widget(table, area);
}

fn render_connections_table(frame: &mut Frame, app: &App, block: Block<'_>, area: Rect) {
    let header = Row::new([
        Cell::from(connection_header_label(
            app,
            ConnectionSortColumn::Proto,
            "Proto",
        )),
        Cell::from(connection_header_label(
            app,
            ConnectionSortColumn::Local,
            "Local IP",
        )),
        Cell::from("LPort"),
        Cell::from(connection_header_label(
            app,
            ConnectionSortColumn::Foreign,
            "Foreign IP",
        )),
        Cell::from("FPort"),
        Cell::from(connection_header_label(
            app,
            ConnectionSortColumn::State,
            "State",
        )),
    ])
    .style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );

    let rows = app
        .navigation_items
        .iter()
        .enumerate()
        .filter_map(|(idx, item)| {
            let NavigationItem::Connection {
                proto,
                local_ip,
                local_port,
                foreign_ip,
                foreign_port,
                state,
                ..
            } = item
            else {
                return None;
            };

            let style = if idx == app.selected_index {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            Some(
                Row::new([
                    highlighted_filter_cell(proto.to_uppercase(), &app.connection_filter),
                    highlighted_filter_cell(local_ip.clone(), &app.connection_filter),
                    highlighted_filter_cell(local_port.clone(), &app.connection_filter),
                    highlighted_filter_cell(foreign_ip.clone(), &app.connection_filter),
                    highlighted_filter_cell(foreign_port.clone(), &app.connection_filter),
                    highlighted_filter_cell(
                        state.clone().unwrap_or_default(),
                        &app.connection_filter,
                    ),
                ])
                .style(style),
            )
        });

    let table = Table::new(
        rows,
        [
            Constraint::Length(5),
            Constraint::Length(10),
            Constraint::Length(5),
            Constraint::Length(10),
            Constraint::Length(5),
            Constraint::Length(6),
        ],
    )
    .header(header)
    .column_spacing(1)
    .block(block);

    frame.render_widget(table, area);
}

fn port_header_label(app: &App, column: PortSortColumn, label: &str) -> String {
    if app.port_sort_column != column {
        return label.to_string();
    }

    let arrow = match app.port_sort_direction {
        SortDirection::Ascending => "↑",
        SortDirection::Descending => "↓",
    };
    format!("{label} {arrow}")
}

fn connection_header_label(app: &App, column: ConnectionSortColumn, label: &str) -> String {
    if app.connection_sort_column != column {
        return label.to_string();
    }

    let arrow = match app.connection_sort_direction {
        SortDirection::Ascending => "↑",
        SortDirection::Descending => "↓",
    };
    format!("{label} {arrow}")
}

fn format_endpoint(ip: &str, port: &str) -> String {
    format!("{ip}:{port}")
}

fn highlighted_filter_cell(value: String, query: &str) -> Cell<'static> {
    let query = query.trim();
    if query.is_empty() {
        return Cell::from(value);
    }

    let value_lower = value.to_lowercase();
    let query_lower = query.to_lowercase();
    let mut spans = Vec::new();
    let mut cursor = 0;

    while let Some(offset) = value_lower[cursor..].find(&query_lower) {
        let start = cursor + offset;
        let end = start + query_lower.len();
        if !value.is_char_boundary(start) || !value.is_char_boundary(end) {
            break;
        }

        if start > cursor {
            spans.push(Span::raw(value[cursor..start].to_string()));
        }
        spans.push(Span::styled(
            value[start..end].to_string(),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
        cursor = end;
    }

    if spans.is_empty() {
        Cell::from(value)
    } else {
        if cursor < value.len() {
            spans.push(Span::raw(value[cursor..].to_string()));
        }
        Cell::from(Line::from(spans))
    }
}

fn draw_help(frame: &mut Frame) {
    let area = get_centered_rect(54, 42, frame.size());
    let block = Block::default()
        .title(" Help ")
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black).fg(Color::White));
    let inner = block.inner(area);

    frame.render_widget(Clear, area);
    frame.render_widget(block, area);

    let lines = vec![
        Line::from("q quit    r refresh    a all interfaces"),
        Line::from("i interface  n network  c connections"),
        Line::from("p ports      e timeline g routes"),
        Line::from("u check updates   U apply update"),
        Line::from("R release notes    Esc close popup"),
        Line::from("o raw output ? help     s sort S save"),
        Line::from("j/k up/down  h/l tabs   [/]: details scroll"),
    ];
    let help = Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .style(Style::default().bg(Color::Black).fg(Color::White));
    frame.render_widget(help, inner);
}

fn update_status_label(app: &App) -> String {
    match &app.update_status {
        crate::update::UpdateStatus::Idle => format!("v{}", env!("CARGO_PKG_VERSION")),
        crate::update::UpdateStatus::Checking { manual } => {
            if *manual {
                "update: checking(now)".to_string()
            } else {
                "update: checking".to_string()
            }
        }
        crate::update::UpdateStatus::Available { version } => {
            format!("update: v{version} ready")
        }
        crate::update::UpdateStatus::Installing { version, .. } => {
            format!("update: installing v{version}")
        }
        crate::update::UpdateStatus::Updated { version } => {
            format!("update: v{version} installed")
        }
        crate::update::UpdateStatus::UpToDate => "update: latest".to_string(),
        crate::update::UpdateStatus::Error { .. } => "update: error".to_string(),
    }
}

fn command_panel_height(app: &App) -> u16 {
    if matches!(
        &app.update_status,
        crate::update::UpdateStatus::Available { .. }
    ) && app
        .pending_update
        .as_ref()
        .is_some_and(|update| !update.release_notes.trim().is_empty())
    {
        2
    } else {
        1
    }
}

fn build_command_panel(app: &App) -> (Vec<Line<'static>>, Style) {
    let command_str = get_active_command(app.view_mode);
    match &app.update_status {
        crate::update::UpdateStatus::Available { version } => {
            let mut lines = vec![Line::from(vec![
                Span::styled(
                    " UPDATE READY ",
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(
                    format!("v{version}"),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(
                    "PRESS U TO INSTALL",
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD | Modifier::RAPID_BLINK),
                ),
                Span::raw("   "),
                Span::styled(
                    "u re-check",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
            ])];

            if let Some(update) = &app.pending_update {
                let notes = summarize_release_notes_for_banner(&update.release_notes, 96);
                lines.push(Line::from(vec![
                    Span::styled(
                        " Notes: ",
                        Style::default()
                            .fg(Color::Black)
                            .bg(Color::LightYellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(notes, Style::default().fg(Color::White)),
                ]));
            }

            (lines, Style::default().bg(Color::DarkGray))
        }
        crate::update::UpdateStatus::Installing { version, .. } => (
            vec![Line::from(vec![
                Span::styled(
                    " INSTALLING UPDATE ",
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::LightBlue)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(
                    format!("v{version}"),
                    Style::default()
                        .fg(Color::LightBlue)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(
                    "please wait",
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
            ])],
            Style::default().bg(Color::DarkGray),
        ),
        crate::update::UpdateStatus::Updated { version } => (
            vec![Line::from(vec![
                Span::styled(
                    " UPDATE INSTALLED ",
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(
                    format!("v{version}"),
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(
                    "restart app",
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
            ])],
            Style::default().bg(Color::DarkGray),
        ),
        crate::update::UpdateStatus::Error { .. } => (
            vec![Line::from(vec![
                Span::styled(
                    " UPDATE FAILED ",
                    Style::default()
                        .fg(Color::White)
                        .bg(Color::Red)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(
                    "press u to check again",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
            ])],
            Style::default().bg(Color::DarkGray),
        ),
        _ => (
            vec![Line::from(vec![
                Span::styled(
                    "$ ",
                    Style::default()
                        .fg(Color::Rgb(0, 255, 102))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    command_str,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("   "),
                Span::styled(
                    update_status_label(app),
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("   "),
                Span::styled(
                    "o[output]",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(
                    "?[help]",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
            ])],
            Style::default(),
        ),
    }
}

fn truncate_release_notes(notes: &str, max_chars: usize) -> String {
    let mut out = String::new();

    for ch in notes.chars() {
        if out.chars().count() >= max_chars {
            out.push_str("...");
            return out;
        }
        out.push(ch);
    }

    out
}

fn summarize_release_notes_for_banner(notes: &str, max_chars: usize) -> String {
    let mut summary_parts = Vec::new();

    for raw_line in notes.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let cleaned = trimmed
            .trim_start_matches('#')
            .trim_start_matches('-')
            .trim_start_matches('*')
            .trim_start_matches(' ')
            .trim();

        if cleaned.is_empty() {
            continue;
        }

        summary_parts.push(cleaned.to_string());
        if summary_parts.len() == 2 {
            break;
        }
    }

    let summary = if summary_parts.is_empty() {
        notes.trim().to_string()
    } else {
        summary_parts.join(" | ")
    };

    truncate_release_notes(&summary, max_chars)
}

fn draw_release_notes_viewer(frame: &mut Frame, app: &App) {
    let area = if frame.size().width < 90 || frame.size().height < 28 {
        frame.size()
    } else {
        get_centered_rect(82, 78, frame.size())
    };

    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" Release Notes ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .style(Style::default().bg(Color::Black).fg(Color::White));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(update) = &app.pending_update else {
        let empty = Paragraph::new("No pending release notes. Press 'u' to check for updates.")
            .style(Style::default().bg(Color::Black).fg(Color::White));
        frame.render_widget(empty, inner);
        return;
    };

    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(inner);

    let header = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(
                "Version: ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("v{}", update.target_version),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "Release: ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(update.release_url.as_str()),
        ]),
    ])
    .style(Style::default().bg(Color::Black).fg(Color::White))
    .wrap(Wrap { trim: true });
    frame.render_widget(header, vertical[0]);

    let notes = Paragraph::new(update.release_notes.clone())
        .style(Style::default().bg(Color::Black).fg(Color::White))
        .wrap(Wrap { trim: false })
        .scroll((app.release_notes_viewer.scroll, 0));
    frame.render_widget(notes, vertical[1]);

    let footer = Paragraph::new("Esc/q/R close | j/k or arrows scroll")
        .style(Style::default().bg(Color::DarkGray).fg(Color::White));
    frame.render_widget(footer, vertical[2]);
}

fn get_centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn build_matched_line<'a>(
    line: &'a str,
    matches_in_line: &[&crate::app::SearchMatch],
    text_color: Color,
    highlight_color: Color,
) -> Line<'a> {
    let mut spans = Vec::new();
    let mut last_idx = 0;

    for m in matches_in_line {
        if line.is_char_boundary(m.start_byte) && line.is_char_boundary(m.end_byte) {
            if m.start_byte > last_idx && line.is_char_boundary(last_idx) {
                spans.push(Span::styled(
                    &line[last_idx..m.start_byte],
                    Style::default().fg(text_color),
                ));
            }
            spans.push(Span::styled(
                &line[m.start_byte..m.end_byte],
                Style::default()
                    .fg(Color::Black)
                    .bg(highlight_color)
                    .add_modifier(Modifier::BOLD),
            ));
            last_idx = m.end_byte;
        }
    }

    if last_idx < line.len() && line.is_char_boundary(last_idx) {
        spans.push(Span::styled(
            &line[last_idx..],
            Style::default().fg(text_color),
        ));
    }

    Line::from(spans)
}

fn draw_raw_viewer(frame: &mut Frame, app: &App) {
    let area = if frame.size().width < 80 || frame.size().height < 24 {
        frame.size()
    } else {
        get_centered_rect(80, 85, frame.size())
    };

    frame.render_widget(Clear, area);

    let main_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(68, 68, 68)))
        .style(Style::default().bg(Color::Rgb(0, 0, 0)));
    frame.render_widget(main_block, area);

    // Inner area for contents
    let inner_area = Rect::new(
        area.x + 1,
        area.y + 1,
        area.width.saturating_sub(2),
        area.height.saturating_sub(2),
    );

    let vertical_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Tabs
            Constraint::Length(1), // Separator line
            Constraint::Min(3),    // Content
            Constraint::Length(1), // Status Bar / Search Bar
        ])
        .split(inner_area);

    // 1. Sources Tab Bar
    let mut tab_spans = vec![Span::styled(
        "Sources: ",
        Style::default().fg(Color::DarkGray),
    )];
    for (i, src) in app.raw_viewer.sources.iter().enumerate() {
        if i > 0 {
            tab_spans.push(Span::styled("  |  ", Style::default().fg(Color::DarkGray)));
        }
        let style = if i == app.raw_viewer.selected_index {
            Style::default()
                .bg(Color::Rgb(0, 255, 102))
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Rgb(192, 255, 192))
        };
        tab_spans.push(Span::styled(format!(" {} ", src.as_str()), style));
    }
    let tab_p =
        Paragraph::new(Line::from(tab_spans)).style(Style::default().bg(Color::Rgb(0, 0, 0)));
    frame.render_widget(tab_p, vertical_chunks[0]);

    // 2. Separator Line
    let separator_text = "─".repeat(inner_area.width as usize);
    let separator_p = Paragraph::new(separator_text).style(
        Style::default()
            .fg(Color::Rgb(68, 68, 68))
            .bg(Color::Rgb(0, 0, 0)),
    );
    frame.render_widget(separator_p, vertical_chunks[1]);

    // 3. Command Output Content
    let source_id = app.raw_viewer.sources.get(app.raw_viewer.selected_index);
    let mut lines = Vec::new();
    let mut text_store = String::new();

    if let Some(&src) = source_id {
        if let Some(output) = app.command_outputs.get(&src) {
            // Command prompt
            lines.push(Line::from(vec![
                Span::styled(
                    "$ ",
                    Style::default()
                        .fg(Color::Rgb(0, 255, 102))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    &output.command,
                    Style::default()
                        .fg(Color::Rgb(0, 255, 102))
                        .add_modifier(Modifier::BOLD),
                ),
            ]));

            // Timestamp and Exit Code
            let local_time: DateTime<Local> = output.executed_at.into();
            let time_str = local_time.format("%Y-%m-%d %H:%M:%S").to_string();
            let exit_str = match output.exit_code {
                Some(code) => code.to_string(),
                None => "None".to_string(),
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!("Executed: {}  |  Exit Code: ", time_str),
                    Style::default().fg(Color::Rgb(128, 128, 128)),
                ),
                Span::styled(
                    exit_str,
                    Style::default().fg(if output.exit_code == Some(0) {
                        Color::Rgb(0, 255, 102)
                    } else {
                        Color::Rgb(255, 102, 102)
                    }),
                ),
            ]));
            lines.push(Line::raw(""));

            // Combined output text
            text_store.push_str(&output.stdout);
            text_store.push('\n');
            text_store.push_str(&output.stderr);
            let text_color = if output.exit_code == Some(0) {
                Color::Rgb(192, 255, 192)
            } else {
                Color::Rgb(255, 102, 102)
            };
            let highlight_color = Color::Rgb(255, 204, 0);

            for (line_idx, line) in text_store.lines().enumerate() {
                let matches_in_line: Vec<&crate::app::SearchMatch> = app
                    .raw_viewer
                    .search_matches
                    .iter()
                    .filter(|m| m.line_index == line_idx)
                    .collect();

                if matches_in_line.is_empty() {
                    lines.push(Line::styled(line, Style::default().fg(text_color)));
                } else {
                    lines.push(build_matched_line(
                        line,
                        &matches_in_line,
                        text_color,
                        highlight_color,
                    ));
                }
            }
        } else {
            lines.push(Line::styled(
                "Command execution history not found.",
                Style::default().fg(Color::Rgb(255, 102, 102)),
            ));
        }
    } else {
        lines.push(Line::styled(
            "No source selected.",
            Style::default().fg(Color::Rgb(255, 102, 102)),
        ));
    }

    let lines_count = lines.len();
    let content_height = vertical_chunks[2].height as usize;
    let max_scroll = lines_count.saturating_sub(content_height) as u16;
    let render_scroll = app.raw_viewer.scroll.min(max_scroll);

    let content_p = Paragraph::new(lines)
        .style(Style::default().bg(Color::Rgb(0, 0, 0)))
        .scroll((render_scroll, 0));
    frame.render_widget(content_p, vertical_chunks[2]);

    // 4. Status Bar / Search Prompt
    let status_line = if app.raw_viewer.search_active {
        Line::from(vec![
            Span::styled(
                "Search: ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                &app.raw_viewer.search_query,
                Style::default().fg(Color::White),
            ),
            Span::styled("█", Style::default().fg(Color::Yellow)),
        ])
    } else if !app.raw_viewer.search_query.is_empty() {
        let current = if app.raw_viewer.search_matches.is_empty() {
            0
        } else {
            app.raw_viewer.current_match_index + 1
        };
        let total = app.raw_viewer.search_matches.len();
        Line::from(vec![
            Span::styled("Search: ", Style::default().fg(Color::Yellow)),
            Span::styled(
                format!(
                    "{} ({} / {})  -  n: Next, N: Prev  |  ",
                    app.raw_viewer.search_query, current, total
                ),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                "Esc/q/o: Close | Tab: Next Src | y: Copy Cmd | Y: Copy Output",
                Style::default().fg(Color::Gray),
            ),
        ])
    } else {
        Line::from(Span::styled(
            "Esc/q/o: Close | Tab: Next Src | y: Copy Cmd | Y: Copy Output | /: Search",
            Style::default().fg(Color::Rgb(180, 180, 180)),
        ))
    };

    let status_p = Paragraph::new(status_line).style(Style::default().bg(Color::Rgb(30, 30, 30)));
    frame.render_widget(status_p, vertical_chunks[3]);
}

fn prefix_len_to_ipv4_mask(prefix_len: u8) -> String {
    let mask = if prefix_len == 0 {
        0
    } else if prefix_len >= 32 {
        u32::MAX
    } else {
        u32::MAX << (32 - prefix_len)
    };
    let octets = std::net::Ipv4Addr::from(mask).octets();
    format!("{}.{}.{}.{}", octets[0], octets[1], octets[2], octets[3])
}

fn calculate_ipv4_subnet_u32(ip_val: u32, prefix_len: u8) -> std::net::Ipv4Addr {
    let mask = if prefix_len == 0 {
        0
    } else if prefix_len >= 32 {
        u32::MAX
    } else {
        u32::MAX << (32 - prefix_len)
    };
    std::net::Ipv4Addr::from(ip_val & mask)
}

fn calculate_ipv6_subnet_arr(ip: &std::net::Ipv6Addr, prefix_len: u8) -> std::net::Ipv6Addr {
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
    std::net::Ipv6Addr::from(subnet_octets)
}

fn route_family_label(family: RouteFamily) -> &'static str {
    match family {
        RouteFamily::Ipv4 => "IPv4",
        RouteFamily::Ipv6 => "IPv6",
        RouteFamily::Unknown => "?",
    }
}

fn diagnostic_color(severity: RouteDiagnosticSeverity) -> Color {
    match severity {
        RouteDiagnosticSeverity::Info => Color::Blue,
        RouteDiagnosticSeverity::Warning => Color::Yellow,
        RouteDiagnosticSeverity::Error => Color::Red,
    }
}

fn render_route_inspector_details(frame: &mut Frame, app: &App, area: Rect) {
    match app.route_inspector.active_section {
        RouteInspectorSection::RouteTable => {
            render_route_table(frame, app, Block::default(), area);
        }
        _ => {
            let lines = match app.route_inspector.active_section {
                RouteInspectorSection::Summary => route_summary_lines(app),
                RouteInspectorSection::PathViewer => route_path_lines(app),
                RouteInspectorSection::VpnRoutes => vpn_route_lines(app),
                RouteInspectorSection::Diagnostics => route_diagnostic_lines(app),
                RouteInspectorSection::RouteTable => unreachable!(),
            };

            let paragraph = Paragraph::new(lines)
                .wrap(Wrap { trim: true })
                .scroll((app.details_scroll, 0));
            frame.render_widget(paragraph, area);
        }
    }
}

fn route_summary_lines(app: &App) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(Span::styled(
            "=== Route Summary ===",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    let routes = app
        .current_snapshot
        .as_ref()
        .map(|snapshot| snapshot.routes.as_slice())
        .unwrap_or(&[]);

    if let Some(default_route) = routes.iter().find(|route| is_default_route(route)) {
        lines.push(Line::from(vec![
            Span::styled(
                "Default Gateway: ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(default_route.gateway.clone()),
        ]));
        lines.push(Line::from(vec![
            Span::styled(
                "Default Interface: ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                default_route.interface.clone(),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
    } else {
        lines.push(Line::from(Span::styled(
            "No default route",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )));
    }

    let ipv4_count = routes
        .iter()
        .filter(|route| route.family == RouteFamily::Ipv4)
        .count();
    let ipv6_count = routes
        .iter()
        .filter(|route| route.family == RouteFamily::Ipv6)
        .count();
    let first_vpn_interface = routes
        .iter()
        .find(|route| is_vpn_interface_name(&route.interface))
        .map(|route| route.interface.as_str());
    let warning_count = app
        .route_inspector
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == RouteDiagnosticSeverity::Warning)
        .count();

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(
            "IPv4 Routes: ",
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(ipv4_count.to_string()),
    ]));
    lines.push(Line::from(vec![
        Span::styled(
            "IPv6 Routes: ",
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(ipv6_count.to_string()),
    ]));
    lines.push(Line::from(vec![
        Span::styled("VPN: ", Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(
            if first_vpn_interface.is_some() {
                "Connected"
            } else {
                "Disconnected"
            },
            if first_vpn_interface.is_some() {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            },
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled(
            "First VPN Interface: ",
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(first_vpn_interface.unwrap_or("None").to_string()),
    ]));
    lines.push(Line::from(vec![
        Span::styled("Warnings: ", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(warning_count.to_string()),
    ]));

    lines
}

fn route_path_lines(app: &App) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(Span::styled(
            "=== Path Viewer ===",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from({
            let mut spans = vec![
                Span::styled(
                    "Destination: ",
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(app.route_inspector.destination_input.clone()),
            ];
            if app.route_inspector.destination_input_active {
                spans.push(Span::styled("█", Style::default().fg(Color::Yellow)));
            }
            spans
        }),
        Line::from(""),
    ];

    if let Some(result) = &app.route_inspector.latest_path_result {
        let graph = build_route_graph(result);
        lines.extend(render_route_graph_lines(&graph).into_iter().map(Line::from));
    } else if let Some(error) = &app.route_inspector.latest_path_error {
        lines.push(Line::from(Span::styled(
            error.clone(),
            Style::default().fg(Color::Red),
        )));
    } else {
        lines.push(Line::from(
            "Enter a destination and press Enter to inspect the route.",
        ));
    }

    lines
}

fn route_table_detail_lines(app: &App) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(Span::styled(
            "=== Route Table ===",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Destination         Gateway          Interface  Metric  Proto   Flags   Family",
            Style::default().add_modifier(Modifier::BOLD),
        )),
    ];

    let Some(snapshot) = &app.current_snapshot else {
        lines.push(Line::from("No route snapshot available."));
        return lines;
    };

    if snapshot.routes.is_empty() {
        lines.push(Line::from("No routes detected."));
        return lines;
    }

    for (_, route) in app.filtered_sorted_routes() {
        let text = format!(
            "{:<18} {:<16} {:<10} {:<7} {:<7} {:<7} {}",
            route.destination,
            route.gateway,
            route.interface,
            route
                .metric
                .map(|metric| metric.to_string())
                .unwrap_or_else(|| "-".to_string()),
            route.protocol.as_deref().unwrap_or("-"),
            route.flags.as_deref().unwrap_or("-"),
            route_family_label(route.family),
        );
        let style = if is_default_route(route) {
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD)
        } else if is_vpn_interface_name(&route.interface) {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };
        lines.push(Line::from(Span::styled(text, style)));
    }

    lines
}

fn render_route_table(frame: &mut Frame, app: &App, block: Block<'_>, area: Rect) {
    let header = Row::new([
        Cell::from(route_header_label(
            app,
            RouteSortColumn::Destination,
            "Destination",
        )),
        Cell::from(route_header_label(app, RouteSortColumn::Gateway, "Gateway")),
        Cell::from(route_header_label(
            app,
            RouteSortColumn::Interface,
            "Interface",
        )),
        Cell::from(route_header_label(app, RouteSortColumn::Metric, "Metric")),
        Cell::from("Protocol"),
        Cell::from("Flags"),
        Cell::from("Family"),
    ])
    .style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );

    let rows = app.filtered_sorted_routes().into_iter().map(|(_, route)| {
        let metric = route
            .metric
            .map(|metric| metric.to_string())
            .unwrap_or_else(|| "-".to_string());
        let style = if is_default_route(route) {
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD)
        } else if is_vpn_interface_name(&route.interface) {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };

        Row::new([
            highlighted_filter_cell(route.destination.clone(), &app.route_inspector.route_filter),
            highlighted_filter_cell(route.gateway.clone(), &app.route_inspector.route_filter),
            highlighted_filter_cell(route.interface.clone(), &app.route_inspector.route_filter),
            Cell::from(metric.clone()),
            Cell::from(route.protocol.as_deref().unwrap_or("-")),
            Cell::from(route.flags.as_deref().unwrap_or("-")),
            Cell::from(route_family_label(route.family)),
        ])
        .style(style)
    });

    let table = Table::new(
        rows,
        [
            Constraint::Length(18),
            Constraint::Length(16),
            Constraint::Length(12),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(7),
            Constraint::Length(7),
        ],
    )
    .header(header)
    .column_spacing(1)
    .block(block);
    frame.render_widget(table, area);
}

fn route_header_label(app: &App, column: RouteSortColumn, label: &str) -> String {
    if app.route_inspector.sort_column != column {
        return label.to_string();
    }

    let arrow = match app.route_inspector.route_sort_direction {
        SortDirection::Ascending => "↑",
        SortDirection::Descending => "↓",
    };
    format!("{label} {arrow}")
}

fn vpn_route_lines(app: &App) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(Span::styled(
            "=== VPN Routes ===",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    let routes = app
        .current_snapshot
        .as_ref()
        .map(|snapshot| snapshot.routes.as_slice())
        .unwrap_or(&[]);
    let vpn_routes: Vec<_> = routes
        .iter()
        .filter(|route| is_vpn_interface_name(&route.interface))
        .collect();

    if vpn_routes.is_empty() {
        lines.push(Line::from("No VPN routes detected."));
        return lines;
    }

    for route in vpn_routes {
        lines.push(Line::from(vec![
            Span::styled(
                "Destination: ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(route.destination.clone()),
            Span::raw("  "),
            Span::styled("Interface: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(route.interface.clone(), Style::default().fg(Color::Yellow)),
            Span::raw("  "),
            Span::styled("Gateway: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(route.gateway.clone()),
        ]));
    }

    lines
}

fn route_diagnostic_lines(app: &App) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(Span::styled(
            "=== Diagnostics ===",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    if app.route_inspector.diagnostics.is_empty() {
        lines.push(Line::from(Span::styled(
            "No routing warnings detected.",
            Style::default().fg(Color::Green),
        )));
        return lines;
    }

    for (index, diagnostic) in app.route_inspector.diagnostics.iter().enumerate() {
        if index > 0 {
            lines.push(Line::from(""));
        }
        let severity_style = Style::default().fg(diagnostic_color(diagnostic.severity));
        let severity_bold_style = severity_style.add_modifier(Modifier::BOLD);
        lines.push(Line::from(Span::styled(
            diagnostic.title.clone(),
            severity_bold_style,
        )));
        lines.push(Line::from(vec![
            Span::styled("Description: ", severity_bold_style),
            Span::styled(diagnostic.description.clone(), severity_style),
        ]));
        if let Some(route) = &diagnostic.affected_route {
            lines.push(Line::from(vec![
                Span::styled("Affected Route: ", severity_bold_style),
                Span::styled(
                    format!(
                        "{} via {} dev {} ({})",
                        route.destination,
                        route.gateway,
                        route.interface,
                        route_family_label(route.family),
                    ),
                    severity_style,
                ),
            ]));
        }
        lines.push(Line::from(vec![
            Span::styled("Recommendation: ", severity_bold_style),
            Span::styled(diagnostic.recommendation.clone(), severity_style),
        ]));
    }

    lines
}

fn format_bps(bytes_per_sec: u64) -> String {
    if bytes_per_sec >= 1_000_000_000 {
        format!("{:.1} GB/s", bytes_per_sec as f64 / 1_000_000_000.0)
    } else if bytes_per_sec >= 1_000_000 {
        format!("{:.1} MB/s", bytes_per_sec as f64 / 1_000_000.0)
    } else if bytes_per_sec >= 1_000 {
        format!("{:.1} KB/s", bytes_per_sec as f64 / 1_000.0)
    } else {
        format!("{} B/s", bytes_per_sec)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::model::{
        NetworkSnapshot, ProcessMetrics, RouteDiagnostic, RouteDiagnosticSeverity, RouteEntry,
        RouteFamily, RouteInspectorSection, RoutePathResult,
    };
    use ratatui::{backend::TestBackend, Terminal};

    #[test]
    fn test_ui_draw_no_panic() {
        let app = App::default();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();
        let buffer = terminal.backend().buffer();
        let mut has_borders = false;
        for cell in buffer.content() {
            if cell.symbol() == "│" || cell.symbol() == "─" {
                has_borders = true;
                break;
            }
        }
        assert!(has_borders);
    }

    #[test]
    fn test_ui_draw_network_view_no_panic() {
        let mut app = App::default();
        app.view_mode = ViewMode::Network;
        app.navigation_items = vec![
            NavigationItem::SubnetHeader(crate::model::Subnet::Unassigned),
            NavigationItem::Interface {
                name: "en0".to_string(),
                associated_ip: Some("192.168.0.15".to_string()),
            },
        ];
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();
    }

    #[test]
    fn test_ui_draw_timeline_view_no_panic() {
        let mut app = App::default();
        app.view_mode = ViewMode::Timeline;
        app.recent_events.push(crate::model::NetworkEvent::new(
            crate::model::NetworkEventKind::VpnConnected,
            crate::model::EventSeverity::Info,
            "utun0 connected".to_string(),
        ));
        app.update_navigation_items();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();
    }

    fn route_test_app(section: RouteInspectorSection) -> App {
        let mut app = App::default();
        app.view_mode = ViewMode::Routes;
        app.route_inspector.active_section = section;
        app.current_snapshot = Some(NetworkSnapshot {
            routes: vec![
                route_entry("default", "192.168.0.1", "en0", RouteFamily::Ipv4),
                route_entry("10.8.0.0/24", "10.8.0.1", "utun4", RouteFamily::Ipv4),
            ],
            ..NetworkSnapshot::default()
        });
        app.update_navigation_items();
        app
    }

    fn route_entry(
        destination: &str,
        gateway: &str,
        interface: &str,
        family: RouteFamily,
    ) -> RouteEntry {
        RouteEntry {
            destination: destination.to_string(),
            gateway: gateway.to_string(),
            interface: interface.to_string(),
            metric: Some(100),
            protocol: Some("static".to_string()),
            flags: Some("UGSc".to_string()),
            family,
        }
    }

    fn draw_to_string(app: &mut App) -> String {
        let backend = TestBackend::new(120, 32);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, app)).unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>()
    }

    #[test]
    fn test_ui_draw_routes_summary_section() {
        let mut app = route_test_app(RouteInspectorSection::Summary);

        let rendered = draw_to_string(&mut app);

        assert!(rendered.contains("Route Summary"));
        assert!(rendered.contains("192.168.0.1"));
        assert!(rendered.contains("VPN"));
    }

    #[test]
    fn test_ui_draw_routes_path_viewer_with_result() {
        let mut app = route_test_app(RouteInspectorSection::PathViewer);
        app.route_inspector.latest_path_result = Some(RoutePathResult {
            destination: "8.8.8.8".to_string(),
            resolved_destination: Some("8.8.8.8".to_string()),
            source_ip: Some("192.168.0.25".to_string()),
            interface: Some("en0".to_string()),
            gateway: Some("192.168.0.1".to_string()),
            is_vpn: false,
            raw_output: String::new(),
        });

        let rendered = draw_to_string(&mut app);

        assert!(rendered.contains("Path Viewer"));
        assert!(rendered.contains("This Host"));
        assert!(rendered.contains("8.8.8.8"));
    }

    #[test]
    fn test_ui_draw_routes_diagnostics_section() {
        let mut app = route_test_app(RouteInspectorSection::Diagnostics);
        app.route_inspector.diagnostics = vec![RouteDiagnostic {
            severity: RouteDiagnosticSeverity::Warning,
            title: "Route interface is down".to_string(),
            description: "A route points to an interface that is currently down.".to_string(),
            affected_route: Some(route_entry(
                "default",
                "192.168.0.1",
                "en0",
                RouteFamily::Ipv4,
            )),
            recommendation: "Bring the interface up or remove the stale route.".to_string(),
        }];

        let rendered = draw_to_string(&mut app);

        assert!(rendered.contains("Diagnostics"));
        assert!(rendered.contains("Route interface is down"));
        assert!(rendered.contains("Recommendation"));
    }

    #[test]
    fn test_ui_draw_routes_details_without_selected_route() {
        let mut app = App {
            view_mode: ViewMode::Routes,
            current_snapshot: Some(NetworkSnapshot::default()),
            ..Default::default()
        };
        app.route_inspector.active_section = RouteInspectorSection::Summary;
        app.update_navigation_items();

        let rendered = draw_to_string(&mut app);

        assert!(rendered.contains("Route Summary"));
        assert!(rendered.contains("No default route"));
        assert!(!rendered.contains("No data collected yet"));
    }

    #[test]
    fn test_route_table_detail_uses_active_filter() {
        let mut app = route_test_app(RouteInspectorSection::RouteTable);
        app.route_inspector.route_filter = "utun4".to_string();
        app.update_navigation_items();

        let rendered = route_table_detail_lines(&app)
            .into_iter()
            .flat_map(|line| {
                line.spans
                    .into_iter()
                    .map(|span| span.content.into_owned())
                    .collect::<Vec<_>>()
            })
            .collect::<String>();

        assert!(rendered.contains("10.8.0.0/24"));
        assert!(!rendered.contains("192.168.0.1"));
    }

    #[test]
    fn test_route_table_shows_sorted_headers() {
        let app = route_test_app(RouteInspectorSection::RouteTable);
        let backend = TestBackend::new(140, 32);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();

        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(rendered.contains("Destination ↑"));
        assert!(rendered.contains("Gateway"));
    }

    #[test]
    fn test_route_sort_direction_changes_order() {
        let mut app = route_test_app(RouteInspectorSection::Summary);
        app.current_snapshot = Some(NetworkSnapshot {
            routes: vec![
                route_entry("10.0.0.0/8", "192.168.0.1", "en0", RouteFamily::Ipv4),
                route_entry("0.0.0.0/0", "10.0.0.2", "utun4", RouteFamily::Ipv4),
            ],
            ..NetworkSnapshot::default()
        });
        app.route_inspector.sort_column = RouteSortColumn::Metric;
        app.route_inspector.route_sort_direction = SortDirection::Descending;
        if let Some(snapshot) = app.current_snapshot.as_mut() {
            snapshot.routes[0].metric = Some(20);
            snapshot.routes[1].metric = Some(5);
        }

        let sorted = app
            .filtered_sorted_routes()
            .into_iter()
            .map(|(_, route)| route.destination.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            sorted,
            vec!["10.0.0.0/8".to_string(), "0.0.0.0/0".to_string()]
        );
    }

    #[test]
    fn test_route_diagnostics_color_all_diagnostic_components_by_severity() {
        let mut app = route_test_app(RouteInspectorSection::Diagnostics);
        app.route_inspector.diagnostics = vec![RouteDiagnostic {
            severity: RouteDiagnosticSeverity::Warning,
            title: "Route interface is down".to_string(),
            description: "A route points to an interface that is currently down.".to_string(),
            affected_route: Some(route_entry(
                "default",
                "192.168.0.1",
                "en0",
                RouteFamily::Ipv4,
            )),
            recommendation: "Bring the interface up or remove the stale route.".to_string(),
        }];

        let lines = route_diagnostic_lines(&app);

        assert_eq!(lines[2].spans[0].style.fg, Some(Color::Yellow));
        assert_eq!(lines[3].spans[0].style.fg, Some(Color::Yellow));
        assert_eq!(lines[3].spans[1].style.fg, Some(Color::Yellow));
        assert_eq!(lines[4].spans[0].style.fg, Some(Color::Yellow));
        assert_eq!(lines[4].spans[1].style.fg, Some(Color::Yellow));
        assert_eq!(lines[5].spans[0].style.fg, Some(Color::Yellow));
        assert_eq!(lines[5].spans[1].style.fg, Some(Color::Yellow));
    }

    #[test]
    fn test_get_active_command() {
        let interface_command = if cfg!(target_os = "linux") {
            "ip -details -statistics address show"
        } else {
            "ifconfig"
        };
        assert_eq!(get_active_command(ViewMode::Interface), interface_command);
        assert_eq!(get_active_command(ViewMode::Network), interface_command);
        assert_eq!(get_active_command(ViewMode::Connections), "netstat -an");
        let ports_command = if cfg!(target_os = "linux") {
            "ss -H -ltnp"
        } else {
            "lsof -iTCP -sTCP:LISTEN -P -n"
        };
        assert_eq!(get_active_command(ViewMode::Ports), ports_command);
        let route_command = if cfg!(target_os = "linux") {
            "ip route show"
        } else {
            "netstat -rn"
        };
        assert_eq!(get_active_command(ViewMode::Routes), route_command);
        assert_eq!(get_active_command(ViewMode::Timeline), "event-logger");
    }

    #[test]
    fn test_command_line_shows_output_and_help_hints() {
        let app = App::default();
        let backend = TestBackend::new(120, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();

        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(rendered.contains("o[output]"));
        assert!(rendered.contains("?[help]"));
    }

    #[test]
    fn test_bottom_status_bar_uses_high_contrast_style() {
        let app = App::default();
        let backend = TestBackend::new(120, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();

        let buffer = terminal.backend().buffer();
        let cell = buffer.get(1, 23);

        assert_eq!(cell.bg, Color::Black);
        assert_eq!(cell.fg, Color::LightYellow);
        assert!(cell.modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn test_top_tabs_show_view_shortcuts() {
        let mut app = App::default();
        app.view_mode = ViewMode::Ports;

        let backend = TestBackend::new(120, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();

        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(rendered.contains("Interface(i)"));
        assert!(rendered.contains("Network(n)"));
        assert!(rendered.contains("Port(p)"));
        assert!(rendered.contains("Connection(c)"));
        assert!(rendered.contains("Route(g)"));
        assert!(rendered.contains("Timeline(e)"));
    }

    #[test]
    fn test_top_header_shows_app_name_and_os() {
        let app = App::default();
        let backend = TestBackend::new(120, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();

        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(rendered.contains("🦥"));
        assert!(rendered.contains("Lazyifconfig"));
        assert!(rendered.contains(" - "));
        assert!(rendered.contains(os_display_label()));
    }

    #[test]
    fn test_top_header_shows_process_cpu_and_memory_usage() {
        let mut app = App::default();
        app.process_metrics = Some(ProcessMetrics {
            cpu_usage_tenths: Some(42),
            memory_rss_bytes: Some(128 * 1024 * 1024),
        });

        let backend = TestBackend::new(120, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();

        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(rendered.contains("CPU 4.2%"));
        assert!(rendered.contains("MEM 128.0MB"));
    }

    #[test]
    fn test_ports_view_renders_table_columns() {
        let mut app = App::default();
        app.view_mode = ViewMode::Ports;
        app.navigation_items = vec![NavigationItem::ListeningPort {
            proto: "tcp".to_string(),
            port: "8080".to_string(),
            command: "my-server".to_string(),
            pid: "12345".to_string(),
            user: "alice".to_string(),
            index: 0,
        }];

        let backend = TestBackend::new(120, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();

        let buffer = terminal.backend().buffer();
        let mut left_pane = String::new();
        for y in 0..24 {
            for x in 0..48 {
                left_pane.push_str(buffer.get(x, y).symbol());
            }
        }

        assert!(left_pane.contains("Proto"));
        assert!(left_pane.contains("Port ↑"));
        assert!(left_pane.contains("Command"));
        assert!(left_pane.contains("PID"));
        assert!(left_pane.contains("User"));
        assert!(left_pane.contains("TCP"));
        assert!(left_pane.contains("8080"));
        assert!(left_pane.contains("my-server"));
        assert!(left_pane.contains("12345"));
        assert!(left_pane.contains("alice"));
    }

    #[test]
    fn test_ports_filter_highlights_matching_text() {
        let mut app = App::default();
        app.view_mode = ViewMode::Ports;
        app.port_filter = "server".to_string();
        app.navigation_items = vec![NavigationItem::ListeningPort {
            proto: "tcp".to_string(),
            port: "8080".to_string(),
            command: "my-server".to_string(),
            pid: "12345".to_string(),
            user: "alice".to_string(),
            index: 0,
        }];

        let backend = TestBackend::new(120, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();

        let buffer = terminal.backend().buffer();
        let has_highlight = (2..18).any(|y| {
            (0..48).any(|x| {
                let cell = buffer.get(x, y);
                cell.bg == Color::Yellow
                    && cell.fg == Color::Black
                    && cell.modifier.contains(Modifier::BOLD)
            })
        });

        assert!(has_highlight);
    }

    #[test]
    fn test_connections_view_renders_table_columns() {
        let mut app = App::default();
        app.view_mode = ViewMode::Connections;
        app.navigation_items = vec![NavigationItem::Connection {
            proto: "tcp".to_string(),
            local_ip: "127.0.0.1".to_string(),
            local_port: "5".to_string(),
            foreign_ip: "1.1.1.1".to_string(),
            foreign_port: "443".to_string(),
            state: Some("ESTAB".to_string()),
            index: 0,
        }];

        let backend = TestBackend::new(120, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();

        let buffer = terminal.backend().buffer();
        let mut left_pane = String::new();
        for y in 0..24 {
            for x in 0..48 {
                left_pane.push_str(buffer.get(x, y).symbol());
            }
        }

        assert!(left_pane.contains("Proto"));
        assert!(left_pane.contains("Local IP ↑"));
        assert!(left_pane.contains("LPort"));
        assert!(left_pane.contains("Foreign IP"));
        assert!(left_pane.contains("FPort"));
        assert!(left_pane.contains("State"));
        assert!(left_pane.contains("TCP"));
        assert!(left_pane.contains("127.0.0.1"));
        assert!(left_pane.contains("5"));
        assert!(left_pane.contains("1.1.1.1"));
        assert!(left_pane.contains("443"));
        assert!(left_pane.contains("ESTAB"));
    }

    #[test]
    fn test_connections_filter_highlights_matching_text() {
        let mut app = App::default();
        app.view_mode = ViewMode::Connections;
        app.connection_filter = "1.1.1.1".to_string();
        app.navigation_items = vec![NavigationItem::Connection {
            proto: "tcp".to_string(),
            local_ip: "127.0.0.1".to_string(),
            local_port: "5".to_string(),
            foreign_ip: "1.1.1.1".to_string(),
            foreign_port: "443".to_string(),
            state: Some("ESTAB".to_string()),
            index: 0,
        }];

        let backend = TestBackend::new(120, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();

        let buffer = terminal.backend().buffer();
        let has_highlight = (2..18).any(|y| {
            (0..48).any(|x| {
                let cell = buffer.get(x, y);
                cell.bg == Color::Yellow
                    && cell.fg == Color::Black
                    && cell.modifier.contains(Modifier::BOLD)
            })
        });

        assert!(has_highlight);
    }

    #[test]
    fn draw_tools_view_lists_runnable_and_planned_tools() {
        let mut app = App::default();
        app.set_view_mode(ViewMode::Tools);

        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();

        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(rendered.contains("Tools(t)"));
        assert!(rendered.contains("DNS Lookup"));
        assert!(rendered.contains("Port Check"));
        assert!(rendered.contains("Ping"));
        assert!(rendered.contains("Whois Lookup"));
        assert!(rendered.contains("planned"));
        assert!(rendered.contains("Input"));
        assert!(rendered.contains("Results"));
        assert!(rendered.contains("Raw Output"));
    }

    #[test]
    fn draw_tools_view_shows_input_modal_when_open() {
        let mut app = App::default();
        app.set_view_mode(ViewMode::Tools);
        app.tools.open_input_modal();

        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();

        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(rendered.contains("Tool Input"));
        assert!(rendered.contains("Target"));
        assert!(rendered.contains("Enter run"));
        assert!(rendered.contains("Esc cancel"));
    }

    #[test]
    fn test_update_available_renders_loud_banner() {
        let mut app = App::default();
        app.update_status = crate::update::UpdateStatus::Available {
            version: "9.9.9".to_string(),
        };
        app.pending_update = Some(crate::update::AvailableUpdate {
            current_version: "0.1.0".to_string(),
            target_version: "9.9.9".to_string(),
            release_url: "https://example.com/release".to_string(),
            asset_name: "lazyifconfig-v9.9.9-aarch64-apple-darwin.tar.gz".to_string(),
            download_url: "https://example.com/release.tar.gz".to_string(),
            release_notes: "Big networking refresh\nFaster route parsing\nExtra diagnostics"
                .to_string(),
        });

        let backend = TestBackend::new(120, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();

        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(rendered.contains("UPDATE READY"));
        assert!(rendered.contains("PRESS U TO INSTALL"));
        assert!(rendered.contains("v9.9.9"));
        assert!(rendered.contains("Big networking refresh"));
    }

    #[test]
    fn test_status_text_is_compact_without_raw_output_hint() {
        let modes = [
            ViewMode::Interface,
            ViewMode::Network,
            ViewMode::Connections,
            ViewMode::Ports,
            ViewMode::Timeline,
            ViewMode::Routes,
        ];

        for mode in modes {
            let mut app = App::default();
            app.view_mode = mode;
            let status = get_status_text(&app);

            assert!(!status.contains("Raw Output"));
            let max_len = if mode == ViewMode::Routes { 170 } else { 90 };
            assert!(
                status.len() <= max_len,
                "status too long for {:?}: {}",
                mode,
                status
            );
        }
    }

    #[test]
    fn test_status_text_mentions_update_actions() {
        let app = App::default();
        let status = get_status_text(&app);

        assert!(status.contains("u check"));
        assert!(status.contains("U update"));
        assert!(status.contains("R notes"));
    }

    #[test]
    fn test_help_mentions_update_shortcuts() {
        let mut app = App::default();
        app.help_visible = true;

        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();

        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(rendered.contains("u check updates"));
        assert!(rendered.contains("U apply update"));
        assert!(rendered.contains("R release notes"));
    }

    #[test]
    fn test_release_notes_popup_renders_full_body() {
        let mut app = App::default();
        app.release_notes_viewer.active = true;
        app.pending_update = Some(crate::update::AvailableUpdate {
            current_version: "0.1.0".to_string(),
            target_version: "9.9.9".to_string(),
            release_url: "https://example.com/release".to_string(),
            asset_name: "lazyifconfig-v9.9.9-aarch64-apple-darwin.tar.gz".to_string(),
            download_url: "https://example.com/release.tar.gz".to_string(),
            release_notes: "## Highlights\n- Faster scans\n- Better update UI\n- Route fixes"
                .to_string(),
        });

        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();

        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(rendered.contains("Release Notes"));
        assert!(rendered.contains("v9.9.9"));
        assert!(rendered.contains("Faster scans"));
        assert!(rendered.contains("Better update UI"));
    }
}
