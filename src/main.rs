use std::io;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use lazyifconfig::app::{App, ViewMode, NavigationItem};
use lazyifconfig::command::{run_command_capture, run_kill, run_netstat_ib};
use lazyifconfig::collector::interface::{parse_interfaces, merge_gateways};
use lazyifconfig::collector::stats::merge_stats;
use lazyifconfig::collector::connections::parse_connections;
use lazyifconfig::collector::ports::parse_listening_ports;
use lazyifconfig::collector::routes::parse_routes;
use lazyifconfig::model::{NetworkSnapshot, PublicIpInfo, NetworkEvent, NetworkEventKind, EventSeverity, CommandSourceId, CommandOutput};

pub fn tick_update(app: &mut App) -> Result<(), String> {
    // Merge async command outputs
    if let Ok(lock) = app.async_command_outputs.lock() {
        for (k, v) in lock.iter() {
            app.command_outputs.insert(*k, v.clone());
        }
    }

    let raw_out_res = capture_command_output(app, CommandSourceId::Ifconfig, "ifconfig", "ifconfig", &[]);
    let raw_out = raw_out_res?;
    let mut parsed = parse_interfaces(&raw_out);
    
    let netstat_out_res = capture_command_output(app, CommandSourceId::NetstatRoutes, "netstat -rn", "netstat", &["-rn"]);
    let netstat_out = netstat_out_res.ok();
    if let Some(out) = &netstat_out {
        merge_gateways(&mut parsed, out);
    }
    
    let routes = if let Some(out) = &netstat_out {
        parse_routes(out)
    } else {
        Vec::new()
    };
    
    let _ = capture_command_output(app, CommandSourceId::DefaultRoute, "route -n get default", "route", &["-n", "get", "default"]);

    let stats_out = run_netstat_ib().unwrap_or_else(|_| raw_out.clone());
    let merged = merge_stats(&stats_out, parsed);

    let connections_res = capture_command_output(app, CommandSourceId::NetstatConnections, "netstat -an", "netstat", &["-an"]);
    let connections = if let Ok(netstat_an_out) = &connections_res {
        parse_connections(netstat_an_out)
    } else {
        Vec::new()
    };

    let ports_res = capture_command_output(app, CommandSourceId::LsofPorts, "lsof -iTCP -sTCP:LISTEN -P -n", "lsof", &["-iTCP", "-sTCP:LISTEN", "-P", "-n"]);
    let listening_ports = if let Ok(lsof_out) = &ports_res {
        parse_listening_ports(lsof_out)
    } else {
        Vec::new()
    };
    
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // --- Background Public IP Fetching ---
    let should_fetch = match app.last_public_ip_fetch {
        None => true,
        Some(last) => last.elapsed() >= std::time::Duration::from_secs(300),
    };

    if should_fetch {
        app.last_public_ip_fetch = Some(std::time::Instant::now());
        let public_ip_info_clone = app.public_ip_info.clone();
        let async_outputs_clone = app.async_command_outputs.clone();
        tokio::spawn(async move {
            let start_time = std::time::SystemTime::now();
            let raw_json_capture = run_command_capture("curl", &["-s", "-m", "5", "https://ipinfo.io/json"]);
            let raw_json_res = raw_json_capture.as_ref().map(command_stdout).unwrap_or_else(|e| Err(e.clone()));
            
            if let Ok(mut lock) = async_outputs_clone.lock() {
                lock.insert(CommandSourceId::PublicIp, CommandOutput {
                    command: "curl -s -m 5 https://ipinfo.io/json".to_string(),
                    stdout: raw_json_capture.as_ref().map(|out| out.stdout.clone()).unwrap_or_default(),
                    stderr: raw_json_capture.as_ref().map(|out| out.stderr.clone()).unwrap_or_else(|e| e.clone()),
                    executed_at: start_time,
                    exit_code: raw_json_capture.as_ref().ok().and_then(|out| out.exit_code).or(Some(1)),
                });
            }

            if let Ok(raw_json) = raw_json_res {
                #[derive(serde::Deserialize)]
                struct IpInfoResponse {
                    ip: String,
                    org: Option<String>,
                    country: Option<String>,
                }
                if let Ok(res) = serde_json::from_str::<IpInfoResponse>(&raw_json) {
                    let info = PublicIpInfo {
                        ip: res.ip,
                        provider: res.org,
                        country: res.country,
                    };
                    if let Ok(mut lock) = public_ip_info_clone.lock() {
                        *lock = Some(info);
                    }
                }
            }
        });
    }

    // --- Public IP Change Detection ---
    if let Ok(lock) = app.public_ip_info.lock() {
        if let Some(new_info) = &*lock {
            let mut changed = false;
            let mut ip_changed_msg = None;
            let mut prov_changed_msg = None;

            if let Some(old_info) = &app.current_public_ip_info {
                if old_info.ip != new_info.ip {
                    ip_changed_msg = Some(format!("Public IP Changed: {} -> {}", old_info.ip, new_info.ip));
                    changed = true;
                }
                if old_info.provider != new_info.provider {
                    prov_changed_msg = Some(format!(
                        "Provider Changed: {} -> {}",
                        old_info.provider.as_deref().unwrap_or("Unknown"),
                        new_info.provider.as_deref().unwrap_or("Unknown")
                    ));
                    changed = true;
                }
            } else {
                changed = true;
            }

            if changed {
                if let Some(msg) = ip_changed_msg {
                    app.recent_events.push(NetworkEvent::new(
                        NetworkEventKind::PublicIpChanged,
                        EventSeverity::Info,
                        msg,
                    ));
                }
                if let Some(msg) = prov_changed_msg {
                    app.recent_events.push(NetworkEvent::new(
                        NetworkEventKind::ProviderChanged,
                        EventSeverity::Info,
                        msg,
                    ));
                }
                app.current_public_ip_info = Some(new_info.clone());
            }
        }
    }

    app.replace_snapshot(NetworkSnapshot {
        interfaces: merged,
        connections,
        listening_ports,
        routes,
        captured_at_secs: now,
    });
    Ok(())
}

fn capture_command_output(
    app: &mut App,
    source_id: CommandSourceId,
    command: &str,
    program: &str,
    args: &[&str],
) -> Result<String, String> {
    let captured = run_command_capture(program, args)?;
    let result = command_stdout(&captured);
    app.command_outputs.insert(source_id, CommandOutput {
        command: command.to_string(),
        stdout: captured.stdout,
        stderr: captured.stderr,
        executed_at: std::time::SystemTime::now(),
        exit_code: captured.exit_code,
    });
    result
}

fn command_stdout(output: &lazyifconfig::command::CommandResult) -> Result<String, String> {
    if output.exit_code == Some(0) {
        Ok(output.stdout.clone())
    } else if output.stderr.trim().is_empty() {
        Err(format!("command exited with {:?}", output.exit_code))
    } else {
        Err(output.stderr.clone())
    }
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::default();
    let _ = tick_update(&mut app);

    let mut last_tick = std::time::Instant::now();
    let tick_rate = Duration::from_secs(2);

    loop {
        terminal.draw(|f| lazyifconfig::ui::draw(f, &app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or(Duration::from_secs(0));

        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                // --- Raw viewer mode: intercept all input ---
                if app.raw_viewer.active {
                    if app.raw_viewer.search_active {
                        match key.code {
                            KeyCode::Esc => {
                                app.raw_viewer.search_active = false;
                            }
                            KeyCode::Enter => {
                                app.raw_viewer.search_active = false;
                                if !app.raw_viewer.search_matches.is_empty() {
                                    app.raw_viewer.scroll = app.raw_viewer.search_matches[0].line_index as u16;
                                }
                            }
                            KeyCode::Backspace => {
                                app.raw_viewer.search_query.pop();
                                app.update_raw_viewer_search_matches();
                            }
                            KeyCode::Char(c) => {
                                app.raw_viewer.search_query.push(c);
                                app.update_raw_viewer_search_matches();
                            }
                            _ => {}
                        }
                    } else {
                        match key.code {
                            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('o') | KeyCode::Char('ㅐ') => {
                                app.raw_viewer.active = false;
                            }
                            KeyCode::Tab => {
                                app.raw_viewer.selected_index = (app.raw_viewer.selected_index + 1) % app.raw_viewer.sources.len();
                                app.raw_viewer.scroll = 0;
                                app.update_raw_viewer_search_matches();
                            }
                            KeyCode::BackTab => {
                                app.raw_viewer.selected_index = (app.raw_viewer.selected_index + app.raw_viewer.sources.len() - 1) % app.raw_viewer.sources.len();
                                app.raw_viewer.scroll = 0;
                                app.update_raw_viewer_search_matches();
                            }
                            KeyCode::Char('j') | KeyCode::Down => {
                                app.raw_viewer.scroll = app.raw_viewer.scroll.saturating_add(1);
                            }
                            KeyCode::Char('k') | KeyCode::Up => {
                                app.raw_viewer.scroll = app.raw_viewer.scroll.saturating_sub(1);
                            }
                            KeyCode::PageDown => {
                                app.raw_viewer.scroll = app.raw_viewer.scroll.saturating_add(15);
                            }
                            KeyCode::PageUp => {
                                app.raw_viewer.scroll = app.raw_viewer.scroll.saturating_sub(15);
                            }
                            KeyCode::Home => {
                                app.raw_viewer.scroll = 0;
                            }
                            KeyCode::End => {
                                app.raw_viewer.scroll = u16::MAX;
                            }
                            KeyCode::Char('/') => {
                                app.raw_viewer.search_active = true;
                                app.raw_viewer.search_query.clear();
                                app.raw_viewer.search_matches.clear();
                            }
                            KeyCode::Char('n') => {
                                if !app.raw_viewer.search_matches.is_empty() {
                                    app.raw_viewer.current_match_index = (app.raw_viewer.current_match_index + 1) % app.raw_viewer.search_matches.len();
                                    app.raw_viewer.scroll = app.raw_viewer.search_matches[app.raw_viewer.current_match_index].line_index as u16;
                                }
                            }
                            KeyCode::Char('N') => {
                                if !app.raw_viewer.search_matches.is_empty() {
                                    app.raw_viewer.current_match_index = (app.raw_viewer.current_match_index + app.raw_viewer.search_matches.len() - 1) % app.raw_viewer.search_matches.len();
                                    app.raw_viewer.scroll = app.raw_viewer.search_matches[app.raw_viewer.current_match_index].line_index as u16;
                                }
                            }
                            KeyCode::Char('y') => {
                                if let Some(&src_id) = app.raw_viewer.sources.get(app.raw_viewer.selected_index) {
                                    let _ = lazyifconfig::command::copy_to_clipboard(src_id.as_str());
                                    app.recent_events.push(NetworkEvent::new(
                                        NetworkEventKind::ActionCopied,
                                        EventSeverity::Info,
                                        format!("Copied command: {}", src_id.as_str()),
                                    ));
                                }
                            }
                            KeyCode::Char('Y') => {
                                if let Some(&src_id) = app.raw_viewer.sources.get(app.raw_viewer.selected_index) {
                                    if let Some(out) = app.command_outputs.get(&src_id) {
                                        let text = format!("{}\n{}", out.stdout, out.stderr);
                                        let _ = lazyifconfig::command::copy_to_clipboard(&text);
                                        app.recent_events.push(NetworkEvent::new(
                                            NetworkEventKind::ActionCopied,
                                            EventSeverity::Info,
                                            format!("Copied raw output for: {}", src_id.as_str()),
                                        ));
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    continue;
                }

                // --- Filter mode: intercept all input ---
                if app.port_filter_active {
                    match key.code {
                        KeyCode::Esc => {
                            app.port_filter.clear();
                            app.port_filter_active = false;
                            app.update_navigation_items();
                        }
                        KeyCode::Enter => {
                            app.port_filter_active = false;
                        }
                        KeyCode::Backspace => {
                            app.port_filter.pop();
                            app.update_navigation_items();
                            app.selected_index = 0;
                        }
                        KeyCode::Char(c) => {
                            app.port_filter.push(c);
                            app.update_navigation_items();
                            app.selected_index = 0;
                        }
                        _ => {}
                    }
                    continue;
                }

                // --- Normal mode ---
                match key.code {
                    KeyCode::Esc => {
                        app.help_visible = false;
                    }
                    KeyCode::Char('?') => {
                        app.help_visible = !app.help_visible;
                    }
                    KeyCode::Char('o') | KeyCode::Char('ㅐ') => {
                        app.help_visible = false;
                        let sources = match app.view_mode {
                            ViewMode::Interface | ViewMode::Network => vec![CommandSourceId::Ifconfig],
                            ViewMode::Connections => vec![CommandSourceId::NetstatConnections],
                            ViewMode::Ports => vec![CommandSourceId::LsofPorts],
                            ViewMode::Routes => vec![CommandSourceId::NetstatRoutes, CommandSourceId::DefaultRoute, CommandSourceId::PublicIp],
                            ViewMode::Timeline => vec![CommandSourceId::Ifconfig, CommandSourceId::NetstatRoutes, CommandSourceId::DefaultRoute, CommandSourceId::PublicIp],
                        };
                        if !sources.is_empty() {
                            app.raw_viewer.active = true;
                            app.raw_viewer.sources = sources;
                            app.raw_viewer.selected_index = 0;
                            app.raw_viewer.scroll = 0;
                            app.raw_viewer.search_query.clear();
                            app.raw_viewer.search_active = false;
                            app.raw_viewer.search_matches.clear();
                        }
                    }
                    KeyCode::Char('q') | KeyCode::Char('ㅂ') => break,
                    KeyCode::Char('r') | KeyCode::Char('ㄱ') => {
                        app.help_visible = false;
                        let _ = tick_update(&mut app);
                        last_tick = std::time::Instant::now();
                    }
                    KeyCode::Char('j') | KeyCode::Down | KeyCode::Char('ㅓ') => {
                        app.select_next();
                    }
                    KeyCode::Char('k') | KeyCode::Up | KeyCode::Char('ㅏ') => {
                        app.select_previous();
                    }
                    KeyCode::Char('K') => {
                        app.help_visible = false;
                        if app.view_mode == ViewMode::Ports {
                            // Kill the selected process
                            if let Some(NavigationItem::ListeningPort { pid, command, port, .. }) =
                                app.navigation_items.get(app.selected_index)
                            {
                                let pid = pid.clone();
                                let command = command.clone();
                                let port = port.clone();
                                match run_kill(&pid) {
                                    Ok(()) => {
                                        app.recent_events.push(lazyifconfig::model::NetworkEvent::new(
                                            lazyifconfig::model::NetworkEventKind::ProcessKilled,
                                            lazyifconfig::model::EventSeverity::Info,
                                            format!("Killed {} (PID: {}) on :{}", command, pid, port),
                                        ));
                                        let _ = tick_update(&mut app);
                                        last_tick = std::time::Instant::now();
                                    }
                                    Err(e) => {
                                        app.recent_events.push(lazyifconfig::model::NetworkEvent::new(
                                            lazyifconfig::model::NetworkEventKind::SystemError,
                                            lazyifconfig::model::EventSeverity::Error,
                                            format!("Kill failed (PID: {}): {}", pid, e),
                                        ));
                                    }
                                }
                                if app.recent_events.len() > 100 {
                                    let overflow = app.recent_events.len() - 100;
                                    app.recent_events.drain(0..overflow);
                                }
                            }
                        }
                    }
                    KeyCode::Char('f') | KeyCode::Char('ㄹ') => {
                        app.help_visible = false;
                        if app.view_mode == ViewMode::Ports {
                            app.port_filter_active = true;
                        }
                    }
                    KeyCode::Char('a') | KeyCode::Char('ㅁ') => {
                        app.help_visible = false;
                        app.show_all = !app.show_all;
                        let _ = tick_update(&mut app);
                        last_tick = std::time::Instant::now();
                    }
                    KeyCode::Char('i') | KeyCode::Char('ㅑ') => {
                        app.help_visible = false;
                        app.set_view_mode(ViewMode::Interface);
                    }
                    KeyCode::Char('n') | KeyCode::Char('ㅜ') => {
                        app.help_visible = false;
                        app.set_view_mode(ViewMode::Network);
                    }
                    KeyCode::Char('p') | KeyCode::Char('ㅔ') => {
                        app.help_visible = false;
                        app.set_view_mode(ViewMode::Ports);
                    }
                    KeyCode::Char('e') | KeyCode::Char('ㄷ') => {
                        app.help_visible = false;
                        app.set_view_mode(ViewMode::Timeline);
                    }
                    KeyCode::Char('g') | KeyCode::Char('ㅎ') => {
                        app.help_visible = false;
                        app.set_view_mode(ViewMode::Routes);
                    }
                    KeyCode::Char('c') | KeyCode::Char('ㅊ') => {
                        app.help_visible = false;
                        if app.view_mode == ViewMode::Connections {
                            if let Some(NavigationItem::Connection { foreign, .. }) = app.navigation_items.get(app.selected_index) {
                                let foreign_ip = if let Some(pos) = foreign.rfind(':') {
                                    &foreign[..pos]
                                } else {
                                    foreign.as_str()
                                };
                                if foreign_ip != "*" && foreign_ip != "::" && foreign_ip != "0.0.0.0" && foreign_ip != "*.*" {
                                    if let Err(e) = lazyifconfig::command::copy_to_clipboard(foreign_ip) {
                                        app.recent_events.push(lazyifconfig::model::NetworkEvent::new(
                                            lazyifconfig::model::NetworkEventKind::SystemError,
                                            lazyifconfig::model::EventSeverity::Error,
                                            format!("Failed to copy IP: {}", e),
                                        ));
                                        if app.recent_events.len() > 100 {
                                            let overflow = app.recent_events.len() - 100;
                                            app.recent_events.drain(0..overflow);
                                        }
                                    } else {
                                        app.recent_events.push(lazyifconfig::model::NetworkEvent::new(
                                            lazyifconfig::model::NetworkEventKind::ActionCopied,
                                            lazyifconfig::model::EventSeverity::Info,
                                            format!("Copied IP {} to clipboard", foreign_ip),
                                        ));
                                        if app.recent_events.len() > 100 {
                                            let overflow = app.recent_events.len() - 100;
                                            app.recent_events.drain(0..overflow);
                                        }
                                    }
                                }
                            }
                        } else {
                            app.set_view_mode(ViewMode::Connections);
                        }
                    }
                    KeyCode::Char('w') | KeyCode::Char('ㅈ') => {
                        app.help_visible = false;
                        if app.view_mode == ViewMode::Connections {
                            if let Some(NavigationItem::Connection { foreign, .. }) = app.navigation_items.get(app.selected_index) {
                                let foreign_ip = if let Some(pos) = foreign.rfind(':') {
                                    &foreign[..pos]
                                } else {
                                    foreign.as_str()
                                };
                                if foreign_ip != "*" && foreign_ip != "::" && foreign_ip != "0.0.0.0" && foreign_ip != "*.*" {
                                    let mut should_fetch = false;
                                    if let Ok(lock) = app.whois_cache.lock() {
                                        if !lock.contains_key(foreign_ip) || lock.get(foreign_ip).map(|s| s.as_str()) != Some("Loading...") {
                                            should_fetch = true;
                                        }
                                    }
                                    
                                    if should_fetch {
                                        if let Ok(mut lock) = app.whois_cache.lock() {
                                            lock.insert(foreign_ip.to_string(), "Loading...".to_string());
                                        }
                                        
                                        app.recent_events.push(lazyifconfig::model::NetworkEvent::new(
                                            lazyifconfig::model::NetworkEventKind::ActionWhois,
                                            lazyifconfig::model::EventSeverity::Info,
                                            format!("Starting WHOIS lookup for {}", foreign_ip),
                                        ));
                                        if app.recent_events.len() > 100 {
                                            let overflow = app.recent_events.len() - 100;
                                            app.recent_events.drain(0..overflow);
                                        }
                                        
                                        let cache_clone = app.whois_cache.clone();
                                        let ip_clone = foreign_ip.to_string();
                                        
                                        tokio::spawn(async move {
                                            let result = match lazyifconfig::command::run_whois(&ip_clone) {
                                                Ok(out) => out,
                                                Err(e) => format!("Error running whois: {}", e),
                                            };
                                            if let Ok(mut lock) = cache_clone.lock() {
                                                lock.insert(ip_clone, result);
                                            }
                                        });
                                    }
                                }
                            }
                        }
                    }
                    KeyCode::Char('[') => {
                        app.help_visible = false;
                        app.scroll_details_up();
                    }
                    KeyCode::Char(']') => {
                        app.help_visible = false;
                        app.scroll_details_down();
                    }
                    _ => {}
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            let _ = tick_update(&mut app);
            last_tick = std::time::Instant::now();
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tick_update() {
        let mut app = App::default();
        let res = tick_update(&mut app);
        assert!(res.is_ok());
        assert!(app.current_snapshot.is_some());
    }
}
