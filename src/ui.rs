use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Sparkline, Wrap},
    Frame,
};
use crate::app::{App, NavigationItem, ViewMode};
use crate::model::{InterfaceStatus, Subnet, NetworkKind};
use chrono::{DateTime, Local};

pub fn render_title() -> &'static str {
    "lazyifconfig"
}

fn get_active_command(view_mode: ViewMode) -> &'static str {
    match view_mode {
        ViewMode::Interface | ViewMode::Network => "ifconfig",
        ViewMode::Connections => "netstat -an",
        ViewMode::Ports => "lsof -iTCP -sTCP:LISTEN -P -n",
        ViewMode::Routes => "netstat -rn",
        ViewMode::Timeline => "event-logger",
    }
}

fn get_status_text(app: &App) -> String {
    match app.view_mode {
        ViewMode::Connections => {
            " q quit | c copy | w whois | [/]: scroll | i/n/p/e/g view | j/k nav ".to_string()
        }
        ViewMode::Ports => {
            if app.port_filter_active {
                " filter: type | Enter apply | Esc clear | Backspace delete ".to_string()
            } else {
                " q quit | f filter | K kill | r refresh | [/]: scroll | i/n/c/e/g view | j/k nav ".to_string()
            }
        }
        ViewMode::Timeline => {
            " q quit | [/]: scroll | i/n/c/p/g view | j/k nav ".to_string()
        }
        ViewMode::Routes => {
            " q quit | [/]: scroll | i/n/c/p/e view | j/k nav ".to_string()
        }
        _ => {
            format!(
                " q quit | r refresh | a all:{} | i/n/c/p/e/g view | j/k nav ",
                if app.show_all { "on" } else { "off" }
            )
        }
    }
}

pub fn draw(frame: &mut Frame, app: &App) {
    // When in port filter mode, allocate an extra line for the filter bar
    let filter_bar_height: u16 = if app.port_filter_active || (app.view_mode == ViewMode::Ports && !app.port_filter.is_empty()) { 1 } else { 0 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),                    // 0: Top pane
            Constraint::Length(1),                 // 1: Active Command Panel (NEW)
            Constraint::Length(5),                 // 2: Recent Events Panel
            Constraint::Length(filter_bar_height), // 3: Filter Bar
            Constraint::Length(1),                 // 4: Status Bar
        ])
        .split(frame.size());

    let top_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(top_chunks_area(chunks[0]));

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
    };
    let list_block = Block::default().borders(Borders::ALL).title(title);
    
    let mut list_items = Vec::new();
    for (idx, item) in app.navigation_items.iter().enumerate() {
        let style = if idx == app.selected_index {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        match item {
            NavigationItem::SubnetHeader(subnet) => {
                let text = match subnet {
                    Subnet::Ipv4 { network, prefix_len } => format!("▼ {}/{}", network, prefix_len),
                    Subnet::Ipv6 { network, prefix_len } => format!("▼ {}/{}", network, prefix_len),
                    Subnet::Unassigned => "▼ Unassigned / No IP".to_string(),
                };
                let header_style = if idx == app.selected_index {
                    style
                } else {
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                };
                list_items.push(ListItem::new(text).style(header_style));
            }
            NavigationItem::Interface { name, associated_ip } => {
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
                    format!("  {} {} ({})", status_indicator, name, associated_ip.as_deref().unwrap_or("no IP"))
                } else {
                    format!("{} {} ({})", status_indicator, name, associated_ip.as_deref().unwrap_or("no IP"))
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
            NavigationItem::Connection { proto, local, foreign, state, .. } => {
                let state_str = state.as_ref().map(|s| format!(" ({})", s)).unwrap_or_default();
                let text = format!("[{}] {} -> {}{}", proto.to_uppercase(), local, foreign, state_str);
                list_items.push(ListItem::new(text).style(style));
            }
            NavigationItem::ListeningPort { proto, port, command, pid, .. } => {
                let text = format!("[{}] :{:<6} {} (PID: {})", proto.to_uppercase(), port, command, pid);
                list_items.push(ListItem::new(text).style(style));
            }
            NavigationItem::Event { index, kind, timestamp, message } => {
                let datetime: DateTime<Local> = (*timestamp).into();
                let time_str = datetime.format("%H:%M:%S").to_string();
                let text = format!("{} [{}] {}", time_str, kind.as_str(), message);
                
                // Color code based on severity
                let mut item_style = style;
                if idx != app.selected_index {
                    if let Some(event) = app.recent_events.get(*index) {
                        match event.severity {
                            crate::model::EventSeverity::Warning => item_style = item_style.fg(Color::Yellow),
                            crate::model::EventSeverity::Error => item_style = item_style.fg(Color::Red),
                            crate::model::EventSeverity::Info => {}
                        }
                    }
                }
                list_items.push(ListItem::new(text).style(item_style));
            }
            NavigationItem::Route { destination, interface, .. } => {
                let text = format!("{:<18} → {}", destination, interface);
                list_items.push(ListItem::new(text).style(style));
            }
        }
    }
    let list_widget = List::new(list_items).block(list_block);
    frame.render_widget(list_widget, top_chunks[0]);

    // 2. Right Pane: Details Panel
    let details_block = Block::default()
        .borders(Borders::ALL)
        .title(" Details ");
    
    let details_inner = details_block.inner(top_chunks[1]);
    frame.render_widget(details_block, top_chunks[1]);
    
    if let Some(selected_item) = app.navigation_items.get(app.selected_index) {
        match selected_item {
            NavigationItem::SubnetHeader(subnet) => {
                let mut details_text = String::new();
                details_text.push_str("=== Subnet Information ===\n\n");
                match subnet {
                    Subnet::Ipv4 { network, prefix_len } => {
                        details_text.push_str(&format!("Protocol:       IPv4\n"));
                        details_text.push_str(&format!("Network Addr:   {}\n", network));
                        details_text.push_str(&format!("Prefix Length:  {}\n", prefix_len));
                        details_text.push_str(&format!("Subnet Mask:    {}\n", prefix_len_to_ipv4_mask(*prefix_len)));
                    }
                    Subnet::Ipv6 { network, prefix_len } => {
                        details_text.push_str(&format!("Protocol:       IPv6\n"));
                        details_text.push_str(&format!("Network Addr:   {}\n", network));
                        details_text.push_str(&format!("Prefix Length:  {}\n", prefix_len));
                    }
                    Subnet::Unassigned => {
                        details_text.push_str("Protocol:       N/A\n");
                        details_text.push_str("Description:    Interfaces without an IP Address assigned.\n");
                    }
                }
                
                details_text.push_str("\nMember Interfaces:\n");
                if let Some(snapshot) = &app.current_snapshot {
                    for interface in &snapshot.interfaces {
                        let mut matches_subnet = false;
                        let mut ip_val = "no IP".to_string();

                        match subnet {
                            Subnet::Ipv4 { network, prefix_len } => {
                                for addr in &interface.ipv4 {
                                    if let Some(p) = addr.prefix_len {
                                        if p == *prefix_len {
                                            if let Ok(ip) = addr.value.parse::<std::net::Ipv4Addr>() {
                                                let net_ip = calculate_ipv4_subnet_u32(u32::from(ip), p);
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
                            Subnet::Ipv6 { network, prefix_len } => {
                                for addr in &interface.ipv6 {
                                    if let Some(p) = addr.prefix_len {
                                        if p == *prefix_len {
                                            if let Ok(ip) = addr.value.parse::<std::net::Ipv6Addr>() {
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
                                let has_ipv4 = interface.ipv4.iter().any(|a| a.prefix_len.is_some());
                                let has_ipv6 = interface.ipv6.iter().any(|a| a.prefix_len.is_some());
                                if !has_ipv4 && !has_ipv6 {
                                    matches_subnet = true;
                                }
                            }
                        }

                        if matches_subnet {
                            details_text.push_str(&format!("  - {} ({})\n", interface.name, ip_val));
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
                    if let Some(interface) = snapshot.interfaces.iter().find(|i| i.name == *name) {
                        let sub_chunks = Layout::default()
                            .direction(Direction::Vertical)
                            .constraints([
                                Constraint::Min(5),
                                Constraint::Length(6),
                            ])
                            .split(details_inner);

                        let mut details_text = String::new();
                        details_text.push_str(&format!("Name:           {}\n", interface.name));
                        details_text.push_str(&format!("Classification: {}\n", interface.network_kind.as_str()));
                        details_text.push_str(&format!("Status:         {}\n", match interface.status {
                            InterfaceStatus::Up => "Active / Up",
                            InterfaceStatus::Down => "Inactive / Down",
                        }));
                        details_text.push_str(&format!("MAC Address:    {}\n", interface.mac_address.as_deref().unwrap_or("N/A")));
                        details_text.push_str(&format!("MTU:            {}\n", interface.mtu.map(|m| m.to_string()).unwrap_or_else(|| "N/A".to_string())));
                        
                        details_text.push_str("\nIPv4 Addresses:\n");
                        for addr in &interface.ipv4 {
                            let gw_str = addr.gateway.as_ref().map(|g| format!(" (Gateway: {})", g)).unwrap_or_default();
                            details_text.push_str(&format!("  - {} / {}{}\n", addr.value, addr.prefix_len.map(|p| p.to_string()).unwrap_or_else(|| "?".to_string()), gw_str));
                        }
                        details_text.push_str("IPv6 Addresses:\n");
                        for addr in &interface.ipv6 {
                            let gw_str = addr.gateway.as_ref().map(|g| format!(" (Gateway: {})", g)).unwrap_or_default();
                            details_text.push_str(&format!("  - {} / {}{}\n", addr.value, addr.prefix_len.map(|p| p.to_string()).unwrap_or_else(|| "?".to_string()), gw_str));
                        }

                        details_text.push_str("\nTraffic Cumulative Stats:\n");
                        if let Some(stats) = &interface.stats {
                            details_text.push_str(&format!("  Packets: RX {} / TX {}\n", stats.rx_packets, stats.tx_packets));
                            details_text.push_str(&format!("  Bytes:   RX {} / TX {}\n", stats.rx_bytes, stats.tx_bytes));
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
            NavigationItem::Connection { proto, local, foreign, state, index: _ } => {
                let mut details_text = String::new();
                details_text.push_str("=== Active Connection Details ===\n\n");
                details_text.push_str(&format!("Protocol:             {}\n", proto.to_uppercase()));
                
                let local_parts: Vec<&str> = local.split(':').collect();
                let local_ip = local_parts[0];
                let local_port = local_parts.get(1).unwrap_or(&"*");
                
                details_text.push_str(&format!("Local IP Address:     {}\n", local_ip));
                details_text.push_str(&format!("Local Port:           {}\n", local_port));
                
                let foreign_parts: Vec<&str> = foreign.split(':').collect();
                let foreign_ip = foreign_parts[0];
                let foreign_port = foreign_parts.get(1).unwrap_or(&"*");
                
                details_text.push_str(&format!("Foreign IP Address:   {}\n", foreign_ip));
                details_text.push_str(&format!("Foreign Port:         {}\n", foreign_port));
                
                if let Some(s) = state {
                    details_text.push_str(&format!("TCP State:            {}\n", s));
                }

                // Map local IP to local interfaces
                let mut mapped_interface = "N/A (External/Wildcard)".to_string();
                if let Some(snapshot) = &app.current_snapshot {
                    for interface in &snapshot.interfaces {
                        let matches_ipv4 = interface.ipv4.iter().any(|addr| addr.value == local_ip);
                        let matches_ipv6 = interface.ipv6.iter().any(|addr| addr.value == local_ip);
                        if matches_ipv4 || matches_ipv6 {
                            mapped_interface = format!("{} ({})", interface.name, interface.network_kind.as_str());
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

                if foreign_ip != "*" && foreign_ip != "::" && foreign_ip != "0.0.0.0" && foreign_ip != "*.*" {
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
                            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
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
            NavigationItem::ListeningPort { proto, port, command, pid, user, .. } => {
                let mut lines = Vec::new();
                lines.push(Line::from(Span::styled(
                    "=== Listening Port Details ===",
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                )));
                lines.push(Line::from(""));
                lines.push(Line::from(vec![
                    Span::styled("Protocol:   ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(proto.to_uppercase()),
                ]));
                lines.push(Line::from(vec![
                    Span::styled("Port:       ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::styled(port.as_str(), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                ]));
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "=== Process Information ===",
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                )));
                lines.push(Line::from(""));
                lines.push(Line::from(vec![
                    Span::styled("Command:    ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::styled(command.as_str(), Style::default().fg(Color::Green)),
                ]));
                lines.push(Line::from(vec![
                    Span::styled("PID:        ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(pid.as_str()),
                ]));
                lines.push(Line::from(vec![
                    Span::styled("User:       ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(user.as_str()),
                ]));

                let details_p = Paragraph::new(lines)
                    .wrap(Wrap { trim: true })
                    .scroll((app.details_scroll, 0));
                frame.render_widget(details_p, details_inner);
            }
            NavigationItem::Event { index, kind, timestamp, message } => {
                let mut lines = Vec::new();
                lines.push(Line::from(Span::styled(
                    "=== Event Details ===",
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                )));
                lines.push(Line::from(""));
                lines.push(Line::from(vec![
                    Span::styled("Type:        ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::styled(kind.as_str(), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                ]));
                
                let datetime: DateTime<Local> = (*timestamp).into();
                let time_str = datetime.format("%Y-%m-%d %H:%M:%S").to_string();
                lines.push(Line::from(vec![
                    Span::styled("Time:        ", Style::default().add_modifier(Modifier::BOLD)),
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
                    Span::styled("Severity:    ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::styled(severity_str, Style::default().fg(severity_color).add_modifier(Modifier::BOLD)),
                ]));
                
                lines.push(Line::from(""));
                lines.push(Line::from(vec![
                    Span::styled("Description: ", Style::default().add_modifier(Modifier::BOLD)),
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
                };
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "=== Expected Impact ===",
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                )));
                lines.push(Line::from(""));
                lines.push(Line::from(Span::raw(impact)));

                let details_p = Paragraph::new(lines)
                    .wrap(Wrap { trim: true })
                    .scroll((app.details_scroll, 0));
                frame.render_widget(details_p, details_inner);
            }
            NavigationItem::Route { destination, gateway, interface, .. } => {
                let mut lines = Vec::new();
                
                // 1. Selected Route
                lines.push(Line::from(Span::styled(
                    "=== Selected Route ===",
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                )));
                lines.push(Line::from(""));
                lines.push(Line::from(vec![
                    Span::styled("Destination:   ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(destination.as_str()),
                ]));
                lines.push(Line::from(vec![
                    Span::styled("Gateway:       ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(gateway.as_str()),
                ]));
                lines.push(Line::from(vec![
                    Span::styled("Interface:     ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::styled(interface.as_str(), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                ]));
                lines.push(Line::from(""));

                // 2. Default Route Summary
                lines.push(Line::from(Span::styled(
                    "=== Default Route Summary ===",
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                )));
                lines.push(Line::from(""));
                
                let mut def_gw = "None".to_string();
                let mut def_if = "None".to_string();
                if let Some(snapshot) = &app.current_snapshot {
                    // Search for active default route in routes table
                    if let Some(def_route) = snapshot.routes.iter().find(|r| {
                        r.destination == "default" && !r.gateway.starts_with("link#")
                    }) {
                        def_gw = def_route.gateway.clone();
                        def_if = def_route.interface.clone();
                    }
                }
                
                lines.push(Line::from(vec![
                    Span::styled("Gateway:       ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(def_gw),
                ]));
                lines.push(Line::from(vec![
                    Span::styled("Interface:     ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::styled(def_if, Style::default().fg(Color::Green)),
                ]));
                lines.push(Line::from(""));

                // 3. Public IP connectivity
                lines.push(Line::from(Span::styled(
                    "=== Public IP Connectivity ===",
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                )));
                lines.push(Line::from(""));
                
                if let Some(ip_info) = &app.current_public_ip_info {
                    lines.push(Line::from(vec![
                        Span::styled("IP Address:    ", Style::default().add_modifier(Modifier::BOLD)),
                        Span::styled(ip_info.ip.as_str(), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                    ]));
                    lines.push(Line::from(vec![
                        Span::styled("ISP/Provider:  ", Style::default().add_modifier(Modifier::BOLD)),
                        Span::raw(ip_info.provider.as_deref().unwrap_or("Unknown")),
                    ]));
                    lines.push(Line::from(vec![
                        Span::styled("Country:       ", Style::default().add_modifier(Modifier::BOLD)),
                        Span::raw(ip_info.country.as_deref().unwrap_or("Unknown")),
                    ]));
                } else {
                    lines.push(Line::from("Loading public IP info..."));
                }

                let details_p = Paragraph::new(lines)
                    .wrap(Wrap { trim: true })
                    .scroll((app.details_scroll, 0));
                frame.render_widget(details_p, details_inner);
            }
        }
    } else {
        let details_p = Paragraph::new("No data collected yet. Press 'r' to refresh.")
            .wrap(Wrap { trim: true })
            .scroll((app.details_scroll, 0));
        frame.render_widget(details_p, details_inner);
    }

    // 3. Active Command Panel
    let command_str = get_active_command(app.view_mode);
    let command_line = Line::from(vec![
        Span::styled("$ ", Style::default().fg(Color::Rgb(0, 255, 102)).add_modifier(Modifier::BOLD)),
        Span::styled(command_str, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::raw("   "),
        Span::styled("o[output]", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw(" "),
        Span::styled("?[help]", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
    ]);
    let command_p = Paragraph::new(command_line);
    frame.render_widget(command_p, chunks[1]);

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
        
        event_items.push(ListItem::new(format!("[{}] {}", time_str, event.message)).style(item_style));
    }
    let event_list = List::new(event_items).block(event_block);
    frame.render_widget(event_list, chunks[2]);

    // 5. Filter Bar (Ports view only)
    if filter_bar_height > 0 {
        let filter_text = if app.port_filter_active {
            format!(" 🔍 Filter: {}▌", app.port_filter)
        } else {
            format!(" 🔍 Filter: {}  (f: edit, Esc: clear)", app.port_filter)
        };
        let filter_style = if app.port_filter_active {
            Style::default().bg(Color::DarkGray).fg(Color::Yellow)
        } else {
            Style::default().bg(Color::DarkGray).fg(Color::White)
        };
        let filter_p = Paragraph::new(filter_text).style(filter_style);
        frame.render_widget(filter_p, chunks[3]);
    }

    // 6. Status Bar
    let status_idx = 4;
    let status_text = get_status_text(app);
    let status_p = Paragraph::new(status_text)
        .style(Style::default().bg(Color::Blue).fg(Color::White));
    frame.render_widget(status_p, chunks[status_idx]);

    if app.help_visible {
        draw_help(frame);
    }

    if app.raw_viewer.active {
        draw_raw_viewer(frame, app);
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
        Line::from("o raw output ? help     Esc close"),
        Line::from("j/k or arrows move      [/]: details scroll"),
    ];
    let help = Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .style(Style::default().bg(Color::Black).fg(Color::White));
    frame.render_widget(help, inner);
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

fn build_matched_line<'a>(line: &'a str, matches_in_line: &[&crate::app::SearchMatch], text_color: Color, highlight_color: Color) -> Line<'a> {
    let mut spans = Vec::new();
    let mut last_idx = 0;

    for m in matches_in_line {
        if line.is_char_boundary(m.start_byte) && line.is_char_boundary(m.end_byte) {
            if m.start_byte > last_idx && line.is_char_boundary(last_idx) {
                spans.push(Span::styled(&line[last_idx..m.start_byte], Style::default().fg(text_color)));
            }
            spans.push(Span::styled(
                &line[m.start_byte..m.end_byte],
                Style::default().fg(Color::Black).bg(highlight_color).add_modifier(Modifier::BOLD)
            ));
            last_idx = m.end_byte;
        }
    }

    if last_idx < line.len() && line.is_char_boundary(last_idx) {
        spans.push(Span::styled(&line[last_idx..], Style::default().fg(text_color)));
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
    let inner_area = Rect::new(area.x + 1, area.y + 1, area.width.saturating_sub(2), area.height.saturating_sub(2));

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
    let mut tab_spans = vec![Span::styled("Sources: ", Style::default().fg(Color::DarkGray))];
    for (i, src) in app.raw_viewer.sources.iter().enumerate() {
        if i > 0 {
            tab_spans.push(Span::styled("  |  ", Style::default().fg(Color::DarkGray)));
        }
        let style = if i == app.raw_viewer.selected_index {
            Style::default().bg(Color::Rgb(0, 255, 102)).fg(Color::Black).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Rgb(192, 255, 192))
        };
        tab_spans.push(Span::styled(format!(" {} ", src.as_str()), style));
    }
    let tab_p = Paragraph::new(Line::from(tab_spans)).style(Style::default().bg(Color::Rgb(0, 0, 0)));
    frame.render_widget(tab_p, vertical_chunks[0]);

    // 2. Separator Line
    let separator_text = "─".repeat(inner_area.width as usize);
    let separator_p = Paragraph::new(separator_text).style(Style::default().fg(Color::Rgb(68, 68, 68)).bg(Color::Rgb(0, 0, 0)));
    frame.render_widget(separator_p, vertical_chunks[1]);

    // 3. Command Output Content
    let source_id = app.raw_viewer.sources.get(app.raw_viewer.selected_index);
    let mut lines = Vec::new();
    let mut text_store = String::new();

    if let Some(&src) = source_id {
        if let Some(output) = app.command_outputs.get(&src) {
            // Command prompt
            lines.push(Line::from(vec![
                Span::styled("$ ", Style::default().fg(Color::Rgb(0, 255, 102)).add_modifier(Modifier::BOLD)),
                Span::styled(&output.command, Style::default().fg(Color::Rgb(0, 255, 102)).add_modifier(Modifier::BOLD)),
            ]));

            // Timestamp and Exit Code
            let local_time: DateTime<Local> = output.executed_at.into();
            let time_str = local_time.format("%Y-%m-%d %H:%M:%S").to_string();
            let exit_str = match output.exit_code {
                Some(code) => code.to_string(),
                None => "None".to_string(),
            };
            lines.push(Line::from(vec![
                Span::styled(format!("Executed: {}  |  Exit Code: ", time_str), Style::default().fg(Color::Rgb(128, 128, 128))),
                Span::styled(
                    exit_str,
                    Style::default().fg(if output.exit_code == Some(0) { Color::Rgb(0, 255, 102) } else { Color::Rgb(255, 102, 102) })
                ),
            ]));
            lines.push(Line::raw(""));

            // Combined output text
            text_store.push_str(&output.stdout);
            text_store.push('\n');
            text_store.push_str(&output.stderr);
            let text_color = if output.exit_code == Some(0) { Color::Rgb(192, 255, 192) } else { Color::Rgb(255, 102, 102) };
            let highlight_color = Color::Rgb(255, 204, 0);

            for (line_idx, line) in text_store.lines().enumerate() {
                let matches_in_line: Vec<&crate::app::SearchMatch> = app.raw_viewer.search_matches
                    .iter()
                    .filter(|m| m.line_index == line_idx)
                    .collect();

                if matches_in_line.is_empty() {
                    lines.push(Line::styled(line, Style::default().fg(text_color)));
                } else {
                    lines.push(build_matched_line(line, &matches_in_line, text_color, highlight_color));
                }
            }
        } else {
            lines.push(Line::styled("Command execution history not found.", Style::default().fg(Color::Rgb(255, 102, 102))));
        }
    } else {
        lines.push(Line::styled("No source selected.", Style::default().fg(Color::Rgb(255, 102, 102))));
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
            Span::styled("Search: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(&app.raw_viewer.search_query, Style::default().fg(Color::White)),
            Span::styled("█", Style::default().fg(Color::Yellow)),
        ])
    } else if !app.raw_viewer.search_query.is_empty() {
        let current = if app.raw_viewer.search_matches.is_empty() { 0 } else { app.raw_viewer.current_match_index + 1 };
        let total = app.raw_viewer.search_matches.len();
        Line::from(vec![
            Span::styled("Search: ", Style::default().fg(Color::Yellow)),
            Span::styled(format!("{} ({} / {})  -  n: Next, N: Prev  |  ", app.raw_viewer.search_query, current, total), Style::default().fg(Color::White)),
            Span::styled("Esc/q/o: Close | Tab: Next Src | y: Copy Cmd | Y: Copy Output", Style::default().fg(Color::Gray)),
        ])
    } else {
        Line::from(Span::styled(
            "Esc/q/o: Close | Tab: Next Src | y: Copy Cmd | Y: Copy Output | /: Search",
            Style::default().fg(Color::Rgb(180, 180, 180))
        ))
    };

    let status_p = Paragraph::new(status_line)
        .style(Style::default().bg(Color::Rgb(30, 30, 30)));
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
    use ratatui::{backend::TestBackend, Terminal};
    use crate::app::App;

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
            }
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

    #[test]
    fn test_get_active_command() {
        assert_eq!(get_active_command(ViewMode::Interface), "ifconfig");
        assert_eq!(get_active_command(ViewMode::Network), "ifconfig");
        assert_eq!(get_active_command(ViewMode::Connections), "netstat -an");
        assert_eq!(get_active_command(ViewMode::Ports), "lsof -iTCP -sTCP:LISTEN -P -n");
        assert_eq!(get_active_command(ViewMode::Routes), "netstat -rn");
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
            assert!(status.len() <= 90, "status too long for {:?}: {}", mode, status);
        }
    }
}
