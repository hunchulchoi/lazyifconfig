use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};
use crate::app::{App, NavigationItem, ViewMode};
use crate::model::{InterfaceStatus, Subnet, NetworkKind};

pub fn render_title() -> &'static str {
    "lazyifconfig"
}

pub fn draw(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),
            Constraint::Length(5),
            Constraint::Length(1),
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
        }
    }
    let list_widget = List::new(list_items).block(list_block);
    frame.render_widget(list_widget, top_chunks[0]);

    // 2. Right Pane: Details Panel
    let details_block = Block::default()
        .borders(Borders::ALL)
        .title(" Details ");
    
    let mut details_text = String::new();
    
    if let Some(selected_item) = app.navigation_items.get(app.selected_index) {
        match selected_item {
            NavigationItem::SubnetHeader(subnet) => {
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
            }
            NavigationItem::Interface { name, .. } => {
                if let Some(snapshot) = &app.current_snapshot {
                    if let Some(interface) = snapshot.interfaces.iter().find(|i| i.name == *name) {
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

                        details_text.push_str("\nTraffic Statistics:\n");
                        if let Some(stats) = &interface.stats {
                            details_text.push_str(&format!("  RX Packets: {}\n", stats.rx_packets));
                            details_text.push_str(&format!("  TX Packets: {}\n", stats.tx_packets));
                            details_text.push_str(&format!("  RX Bytes:   {}\n", stats.rx_bytes));
                            details_text.push_str(&format!("  TX Bytes:   {}\n", stats.tx_bytes));
                            if let Some((rx_rate, tx_rate)) = app.selected_rates() {
                                details_text.push_str(&format!("  RX Rate:    {} B/s\n", rx_rate));
                                details_text.push_str(&format!("  TX Rate:    {} B/s\n", tx_rate));
                            } else {
                                details_text.push_str("  RX Rate:    0 B/s (calculating...)\n");
                                details_text.push_str("  TX Rate:    0 B/s (calculating...)\n");
                            }
                        } else {
                            details_text.push_str("  No stats available\n");
                        }
                    }
                }
            }
        }
    } else {
        details_text.push_str("No data collected yet. Press 'r' to refresh.\n");
    }

    let details_p = Paragraph::new(details_text).block(details_block).wrap(Wrap { trim: true });
    frame.render_widget(details_p, top_chunks[1]);

    // 3. Event Panel
    let event_block = Block::default()
        .borders(Borders::ALL)
        .title(" Recent Events ");
    let mut event_items = Vec::new();
    for event in app.recent_events.iter().rev().take(10) {
        event_items.push(ListItem::new(format!("[{}] {}", event.captured_at_secs, event.message)));
    }
    let event_list = List::new(event_items).block(event_block);
    frame.render_widget(event_list, chunks[1]);

    // 4. Status Bar
    let status_text = format!(
        " q: Quit | r: Refresh | a: Toggle -a ({}) | i: Interface View | n: Network View | j/k: Nav ",
        if app.show_all { "ON" } else { "OFF" }
    );
    let status_p = Paragraph::new(status_text)
        .style(Style::default().bg(Color::Blue).fg(Color::White));
    frame.render_widget(status_p, chunks[2]);
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
}
