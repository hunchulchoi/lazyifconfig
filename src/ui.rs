use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};
use crate::app::App;
use crate::model::InterfaceStatus;

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
        .split(chunks[0]);

    // 1. Left Pane: Interfaces list
    let interfaces_block = Block::default()
        .borders(Borders::ALL)
        .title(" Interfaces ");
    
    let mut list_items = Vec::new();
    if let Some(snapshot) = &app.current_snapshot {
        for (idx, interface) in snapshot.interfaces.iter().enumerate() {
            let status_indicator = match interface.status {
                InterfaceStatus::Up => "●",
                InterfaceStatus::Down => "○",
            };
            let ip = interface.ipv4.first().map(|addr| addr.value.as_str()).unwrap_or("no IP");
            let mut style = if idx == app.selected_index {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            if interface.status == InterfaceStatus::Down {
                if idx == app.selected_index {
                    style = style.add_modifier(Modifier::DIM);
                } else {
                    style = style.fg(Color::DarkGray);
                }
            }
            list_items.push(ListItem::new(format!("{} {} ({})", status_indicator, interface.name, ip)).style(style));
        }
    }
    let interfaces_list = List::new(list_items).block(interfaces_block);
    frame.render_widget(interfaces_list, top_chunks[0]);

    // 2. Right Pane: Interface Details
    let details_block = Block::default()
        .borders(Borders::ALL)
        .title(" Interface Details ");
    
    let mut details_text = String::new();
    if let Some(snapshot) = &app.current_snapshot {
        if let Some(interface) = snapshot.interfaces.get(app.selected_index) {
            details_text.push_str(&format!("Name: {}\n", interface.name));
            details_text.push_str(&format!("Type: {:?}\n", interface.interface_type));
            details_text.push_str(&format!("Status: {}\n", match interface.status {
                InterfaceStatus::Up => "Active / Up",
                InterfaceStatus::Down => "Inactive / Down",
            }));
            details_text.push_str(&format!("MAC Address: {}\n", interface.mac_address.as_deref().unwrap_or("N/A")));
            details_text.push_str(&format!("MTU: {}\n", interface.mtu.map(|m| m.to_string()).unwrap_or_else(|| "N/A".to_string())));
            
            details_text.push_str("\nIPv4 Addresses:\n");
            for addr in &interface.ipv4 {
                details_text.push_str(&format!("  - {}\n", addr.value));
            }
            details_text.push_str("IPv6 Addresses:\n");
            for addr in &interface.ipv6 {
                details_text.push_str(&format!("  - {}\n", addr.value));
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
        } else {
            details_text.push_str("No interface selected\n");
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
        " q: Quit | r: Manual Refresh | a: Toggle -a ({}) | j/k or Arrow Keys: Navigation ",
        if app.show_all { "ON" } else { "OFF" }
    );
    let status_p = Paragraph::new(status_text)
        .style(Style::default().bg(Color::Blue).fg(Color::White));
    frame.render_widget(status_p, chunks[2]);
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
}
