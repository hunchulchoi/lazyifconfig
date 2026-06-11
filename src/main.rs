use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use lazyifconfig::app::{App, NavigationItem, ViewMode};
use lazyifconfig::collector::connections::parse_connections;
use lazyifconfig::collector::interface::{merge_gateways, parse_interfaces};
use lazyifconfig::collector::ports::parse_listening_ports;
use lazyifconfig::collector::routes::{
    parse_linux_route_path, parse_macos_route_path, parse_routes,
};
use lazyifconfig::collector::stats::merge_stats;
use lazyifconfig::command::{
    default_route_command_spec, interface_command_spec, listening_ports_command_spec,
    route_table_command_spec, run_command_capture, run_kill, run_netstat_ib,
};
use lazyifconfig::model::{
    CommandOutput, CommandSourceId, EventSeverity, NetworkEvent, NetworkEventKind, NetworkSnapshot,
    PublicIpInfo, RouteInspectorSection,
};
use lazyifconfig::update::{self, CheckOutcome, UpdateMessage, UpdateStatus};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const RELEASE_CHECK_INTERVAL_SECS: u64 = 6 * 60 * 60;

pub fn tick_update(app: &mut App) -> Result<(), String> {
    // Merge async command outputs
    if let Ok(lock) = app.async_command_outputs.lock() {
        for (k, v) in lock.iter() {
            app.command_outputs.insert(*k, v.clone());
        }
    }

    drain_update_messages(app);
    maybe_start_auto_update_check(app);
    maybe_start_auto_update_install(app);

    let interface_command = interface_command_spec();
    let raw_out_res = capture_command_output(
        app,
        CommandSourceId::Ifconfig,
        interface_command.display,
        interface_command.program,
        interface_command.args,
    );
    let raw_out = raw_out_res?;
    let mut parsed = parse_interfaces(&raw_out);

    let route_table_command = route_table_command_spec();
    let netstat_out_res = capture_command_output(
        app,
        CommandSourceId::NetstatRoutes,
        route_table_command.display,
        route_table_command.program,
        route_table_command.args,
    );
    let netstat_out = netstat_out_res.ok();
    if let Some(out) = &netstat_out {
        merge_gateways(&mut parsed, out);
    }

    let mut routes = if let Some(out) = &netstat_out {
        parse_routes(out)
    } else {
        Vec::new()
    };

    let default_route_command = default_route_command_spec();
    let _ = capture_command_output(
        app,
        CommandSourceId::DefaultRoute,
        default_route_command.display,
        default_route_command.program,
        default_route_command.args,
    );

    if let Some(command) = lazyifconfig::command::ipv6_route_table_command_spec() {
        let ipv6_route_out =
            capture_owned_command_output(app, CommandSourceId::Ipv6Routes, &command).ok();
        routes = merge_additional_route_output(routes, ipv6_route_out.as_deref());
    }

    if let Some(command) = lazyifconfig::command::ip_rule_command_spec() {
        let _ = capture_owned_command_output(app, CommandSourceId::IpRules, &command);
    }

    let stats_out = run_netstat_ib().unwrap_or_else(|_| raw_out.clone());
    let merged = merge_stats(&stats_out, parsed);

    let connections_res = capture_command_output(
        app,
        CommandSourceId::NetstatConnections,
        "netstat -an",
        "netstat",
        &["-an"],
    );
    let connections = if let Ok(netstat_an_out) = &connections_res {
        parse_connections(netstat_an_out)
    } else {
        Vec::new()
    };

    let listening_ports_command = listening_ports_command_spec();
    let ports_res = capture_command_output(
        app,
        CommandSourceId::LsofPorts,
        listening_ports_command.display,
        listening_ports_command.program,
        listening_ports_command.args,
    );
    let listening_ports = if let Ok(ports_out) = &ports_res {
        parse_listening_ports(ports_out)
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
            let raw_json_capture =
                run_command_capture("curl", &["-s", "-m", "5", "https://ipinfo.io/json"]);
            let raw_json_res = raw_json_capture
                .as_ref()
                .map(command_stdout)
                .unwrap_or_else(|e| Err(e.clone()));

            if let Ok(mut lock) = async_outputs_clone.lock() {
                lock.insert(
                    CommandSourceId::PublicIp,
                    CommandOutput {
                        command: "curl -s -m 5 https://ipinfo.io/json".to_string(),
                        stdout: raw_json_capture
                            .as_ref()
                            .map(|out| out.stdout.clone())
                            .unwrap_or_default(),
                        stderr: raw_json_capture
                            .as_ref()
                            .map(|out| out.stderr.clone())
                            .unwrap_or_else(|e| e.clone()),
                        executed_at: start_time,
                        exit_code: raw_json_capture
                            .as_ref()
                            .ok()
                            .and_then(|out| out.exit_code)
                            .or(Some(1)),
                    },
                );
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
                    ip_changed_msg = Some(format!(
                        "Public IP Changed: {} -> {}",
                        old_info.ip, new_info.ip
                    ));
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
    app.command_outputs.insert(
        source_id,
        CommandOutput {
            command: command.to_string(),
            stdout: captured.stdout,
            stderr: captured.stderr,
            executed_at: std::time::SystemTime::now(),
            exit_code: captured.exit_code,
        },
    );
    result
}

fn capture_owned_command_output(
    app: &mut App,
    source_id: CommandSourceId,
    command: &lazyifconfig::command::OwnedCommandSpec,
) -> Result<String, String> {
    let args: Vec<&str> = command.args.iter().map(String::as_str).collect();
    let captured = run_command_capture(command.program.as_str(), &args)?;
    let result = command_stdout(&captured);
    app.command_outputs.insert(
        source_id,
        CommandOutput {
            command: command.display.clone(),
            stdout: captured.stdout,
            stderr: captured.stderr,
            executed_at: std::time::SystemTime::now(),
            exit_code: captured.exit_code,
        },
    );
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

fn merge_additional_route_output(
    mut routes: Vec<lazyifconfig::model::RouteEntry>,
    additional_output: Option<&str>,
) -> Vec<lazyifconfig::model::RouteEntry> {
    if let Some(output) = additional_output {
        routes.extend(parse_routes(output));
    }
    routes
}

fn run_route_path_lookup(app: &mut App) {
    let destination = app.route_inspector.destination_input.trim().to_string();
    if destination.is_empty() {
        app.route_inspector.latest_path_result = None;
        app.route_inspector.latest_path_error = Some("Enter a destination first.".to_string());
        return;
    }

    let command = lazyifconfig::command::route_path_command_spec(&destination);
    match capture_owned_command_output(app, CommandSourceId::RoutePath, &command) {
        Ok(output) => {
            let parsed = if cfg!(target_os = "linux") {
                parse_linux_route_path(&destination, &output)
            } else {
                parse_macos_route_path(&destination, &output)
            };

            match parsed {
                Ok(mut result) => {
                    result.is_vpn = result
                        .interface
                        .as_deref()
                        .map(lazyifconfig::route_inspector::vpn::is_vpn_interface_name)
                        .unwrap_or(false);
                    app.route_inspector.latest_path_result = Some(result);
                    app.route_inspector.latest_path_error = None;
                }
                Err(error) => {
                    app.route_inspector.latest_path_result = None;
                    app.route_inspector.latest_path_error = Some(error);
                }
            }
        }
        Err(error) => {
            app.route_inspector.latest_path_result = None;
            app.route_inspector.latest_path_error = Some(route_path_command_error_message(&error));
        }
    }
}

fn route_path_command_error_message(error: &str) -> String {
    format!("destination could not be resolved by route command: {error}")
}

fn routes_raw_sources(app: &App) -> Vec<CommandSourceId> {
    let mut sources = vec![
        CommandSourceId::NetstatRoutes,
        CommandSourceId::DefaultRoute,
    ];
    if app
        .command_outputs
        .contains_key(&CommandSourceId::Ipv6Routes)
    {
        sources.push(CommandSourceId::Ipv6Routes);
    }
    if app.command_outputs.contains_key(&CommandSourceId::IpRules) {
        sources.push(CommandSourceId::IpRules);
    }
    if app
        .command_outputs
        .contains_key(&CommandSourceId::RoutePath)
    {
        sources.push(CommandSourceId::RoutePath);
    }
    if app.command_outputs.contains_key(&CommandSourceId::PublicIp) {
        sources.push(CommandSourceId::PublicIp);
    }
    sources
}

fn raw_viewer_command_to_copy(app: &App, src_id: CommandSourceId) -> String {
    app.command_outputs
        .get(&src_id)
        .map(|out| out.command.clone())
        .unwrap_or_else(|| src_id.as_str().to_string())
}

fn maybe_start_auto_update_check(app: &mut App) {
    let is_busy = matches!(
        app.update_status,
        UpdateStatus::Checking { .. } | UpdateStatus::Installing { .. }
    );
    if is_busy {
        return;
    }

    let should_check = match app.last_update_check {
        None => true,
        Some(last) => last.elapsed() >= Duration::from_secs(RELEASE_CHECK_INTERVAL_SECS),
    };

    if should_check {
        start_update_check(app, false);
    }
}

fn maybe_start_auto_update_install(app: &mut App) {
    let Some(update) = app.pending_update.clone() else {
        return;
    };

    let is_busy = matches!(
        app.update_status,
        UpdateStatus::Checking { .. } | UpdateStatus::Installing { .. }
    );
    if is_busy {
        return;
    }

    if app.attempted_update_version.as_deref() == Some(update.target_version.as_str()) {
        return;
    }

    start_update_install(app, false);
}

fn start_update_check(app: &mut App, manual: bool) {
    let is_busy = matches!(
        app.update_status,
        UpdateStatus::Checking { .. } | UpdateStatus::Installing { .. }
    );
    if is_busy {
        return;
    }

    let Ok(url) = update::release_api_url() else {
        app.update_status = UpdateStatus::Error {
            message: "invalid GitHub repository URL".to_string(),
        };
        app.push_event(NetworkEvent::new(
            NetworkEventKind::UpdateCheckFailed,
            EventSeverity::Error,
            "Update check failed: invalid GitHub repository URL".to_string(),
        ));
        return;
    };

    app.update_status = UpdateStatus::Checking { manual };
    app.last_update_check = Some(std::time::Instant::now());

    let update_messages = app.update_messages.clone();
    let async_outputs = app.async_command_outputs.clone();
    tokio::spawn(async move {
        let started_at = std::time::SystemTime::now();
        let capture = run_command_capture(
            "curl",
            &[
                "-sS",
                "-L",
                "-m",
                "10",
                "-H",
                "Accept: application/vnd.github+json",
                "-H",
                "User-Agent: lazyifconfig",
                &url,
            ],
        );

        if let Ok(mut lock) = async_outputs.lock() {
            lock.insert(
                CommandSourceId::GitHubRelease,
                CommandOutput {
                    command: format!(
                        "curl -sS -L -m 10 -H 'Accept: application/vnd.github+json' -H 'User-Agent: lazyifconfig' {url}"
                    ),
                    stdout: capture.as_ref().map(|out| out.stdout.clone()).unwrap_or_default(),
                    stderr: capture
                        .as_ref()
                        .map(|out| out.stderr.clone())
                        .unwrap_or_else(|err| err.clone()),
                    executed_at: started_at,
                    exit_code: capture.as_ref().ok().and_then(|out| out.exit_code).or(Some(1)),
                },
            );
        }

        let result = capture
            .and_then(|out| command_stdout(&out))
            .and_then(|stdout| update::evaluate_release_json(&stdout));

        if let Ok(mut lock) = update_messages.lock() {
            lock.push(UpdateMessage::CheckFinished { manual, result });
        }
    });
}

fn start_update_install(app: &mut App, manual: bool) {
    let Some(update) = app.pending_update.clone() else {
        if manual {
            app.push_event(NetworkEvent::new(
                NetworkEventKind::UpdateCheckFailed,
                EventSeverity::Warning,
                "No pending update found. Press 'u' to check now.".to_string(),
            ));
        }
        return;
    };

    let is_busy = matches!(
        app.update_status,
        UpdateStatus::Checking { .. } | UpdateStatus::Installing { .. }
    );
    if is_busy {
        return;
    }

    app.attempted_update_version = Some(update.target_version.clone());
    app.update_status = UpdateStatus::Installing {
        version: update.target_version.clone(),
        manual,
    };

    let update_messages = app.update_messages.clone();
    tokio::spawn(async move {
        let current_exe = std::env::current_exe().map_err(|e| e.to_string());
        let result = match current_exe {
            Ok(path) => update::install_update(&update, &path),
            Err(err) => Err(err),
        };

        if let Ok(mut lock) = update_messages.lock() {
            lock.push(UpdateMessage::InstallFinished {
                manual,
                version: update.target_version.clone(),
                result,
            });
        }
    });
}

fn drain_update_messages(app: &mut App) {
    let messages = if let Ok(mut lock) = app.update_messages.lock() {
        lock.drain(..).collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    for message in messages {
        match message {
            UpdateMessage::CheckFinished { manual, result } => match result {
                Ok(CheckOutcome::UpToDate { .. }) => {
                    app.pending_update = None;
                    app.update_status = UpdateStatus::UpToDate;
                    if manual {
                        app.push_event(NetworkEvent::new(
                            NetworkEventKind::UpdateInstalled,
                            EventSeverity::Info,
                            "Already running the latest release.".to_string(),
                        ));
                    }
                }
                Ok(CheckOutcome::Available(update)) => {
                    let version = update.target_version.clone();
                    app.pending_update = Some(update);
                    app.update_status = UpdateStatus::Available {
                        version: version.clone(),
                    };
                    app.push_event(NetworkEvent::new(
                        NetworkEventKind::UpdateAvailable,
                        EventSeverity::Info,
                        if manual {
                            format!("Update available: v{version}. Starting install.")
                        } else {
                            format!("Auto-update found v{version}. Starting install.")
                        },
                    ));
                }
                Err(err) => {
                    app.update_status = UpdateStatus::Error {
                        message: err.clone(),
                    };
                    app.push_event(NetworkEvent::new(
                        NetworkEventKind::UpdateCheckFailed,
                        EventSeverity::Error,
                        format!("Update check failed: {err}"),
                    ));
                }
            },
            UpdateMessage::InstallFinished {
                version, result, ..
            } => match result {
                Ok(()) => {
                    app.pending_update = None;
                    app.update_status = UpdateStatus::Updated {
                        version: version.clone(),
                    };
                    app.push_event(NetworkEvent::new(
                        NetworkEventKind::UpdateInstalled,
                        EventSeverity::Info,
                        format!("Updated binary to v{version}. Restart lazyifconfig to use it."),
                    ));
                }
                Err(err) => {
                    app.update_status = UpdateStatus::Error {
                        message: err.clone(),
                    };
                    app.push_event(NetworkEvent::new(
                        NetworkEventKind::UpdateCheckFailed,
                        EventSeverity::Error,
                        format!("Update install failed for v{version}: {err}"),
                    ));
                }
            },
        }
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
                                    app.raw_viewer.scroll =
                                        app.raw_viewer.search_matches[0].line_index as u16;
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
                            KeyCode::Esc
                            | KeyCode::Char('q')
                            | KeyCode::Char('o')
                            | KeyCode::Char('ㅐ') => {
                                app.raw_viewer.active = false;
                            }
                            KeyCode::Tab => {
                                app.raw_viewer.selected_index = (app.raw_viewer.selected_index + 1)
                                    % app.raw_viewer.sources.len();
                                app.raw_viewer.scroll = 0;
                                app.update_raw_viewer_search_matches();
                            }
                            KeyCode::BackTab => {
                                app.raw_viewer.selected_index = (app.raw_viewer.selected_index
                                    + app.raw_viewer.sources.len()
                                    - 1)
                                    % app.raw_viewer.sources.len();
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
                                    app.raw_viewer.current_match_index =
                                        (app.raw_viewer.current_match_index + 1)
                                            % app.raw_viewer.search_matches.len();
                                    app.raw_viewer.scroll = app.raw_viewer.search_matches
                                        [app.raw_viewer.current_match_index]
                                        .line_index
                                        as u16;
                                }
                            }
                            KeyCode::Char('N') => {
                                if !app.raw_viewer.search_matches.is_empty() {
                                    app.raw_viewer.current_match_index =
                                        (app.raw_viewer.current_match_index
                                            + app.raw_viewer.search_matches.len()
                                            - 1)
                                            % app.raw_viewer.search_matches.len();
                                    app.raw_viewer.scroll = app.raw_viewer.search_matches
                                        [app.raw_viewer.current_match_index]
                                        .line_index
                                        as u16;
                                }
                            }
                            KeyCode::Char('y') => {
                                if let Some(&src_id) =
                                    app.raw_viewer.sources.get(app.raw_viewer.selected_index)
                                {
                                    let command = raw_viewer_command_to_copy(&app, src_id);
                                    let _ = lazyifconfig::command::copy_to_clipboard(&command);
                                    app.recent_events.push(NetworkEvent::new(
                                        NetworkEventKind::ActionCopied,
                                        EventSeverity::Info,
                                        format!("Copied command: {command}"),
                                    ));
                                }
                            }
                            KeyCode::Char('Y') => {
                                if let Some(&src_id) =
                                    app.raw_viewer.sources.get(app.raw_viewer.selected_index)
                                {
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

                if app.release_notes_viewer.active {
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('R') => {
                            app.release_notes_viewer.active = false;
                        }
                        KeyCode::Char('j') | KeyCode::Down => {
                            app.release_notes_viewer.scroll =
                                app.release_notes_viewer.scroll.saturating_add(1);
                        }
                        KeyCode::Char('k') | KeyCode::Up => {
                            app.release_notes_viewer.scroll =
                                app.release_notes_viewer.scroll.saturating_sub(1);
                        }
                        KeyCode::PageDown => {
                            app.release_notes_viewer.scroll =
                                app.release_notes_viewer.scroll.saturating_add(12);
                        }
                        KeyCode::PageUp => {
                            app.release_notes_viewer.scroll =
                                app.release_notes_viewer.scroll.saturating_sub(12);
                        }
                        KeyCode::Home => {
                            app.release_notes_viewer.scroll = 0;
                        }
                        KeyCode::End => {
                            app.release_notes_viewer.scroll = u16::MAX;
                        }
                        _ => {}
                    }
                    continue;
                }

                if app.route_inspector.destination_input_active {
                    match key.code {
                        KeyCode::Esc => {
                            app.route_inspector.destination_input_active = false;
                        }
                        KeyCode::Enter => {
                            app.route_inspector.destination_input_active = false;
                            run_route_path_lookup(&mut app);
                        }
                        KeyCode::Backspace => {
                            app.route_inspector.destination_input.pop();
                        }
                        KeyCode::Char(c) => {
                            app.route_inspector.destination_input.push(c);
                        }
                        _ => {}
                    }
                    continue;
                }

                if app.route_inspector.route_filter_active {
                    match key.code {
                        KeyCode::Esc => {
                            app.route_inspector.route_filter.clear();
                            app.route_inspector.route_filter_active = false;
                            app.update_navigation_items();
                        }
                        KeyCode::Enter => {
                            app.route_inspector.route_filter_active = false;
                        }
                        KeyCode::Backspace => {
                            app.route_inspector.route_filter.pop();
                            app.update_navigation_items();
                            app.selected_index = 0;
                        }
                        KeyCode::Char(c) => {
                            app.route_inspector.route_filter.push(c);
                            app.update_navigation_items();
                            app.selected_index = 0;
                        }
                        _ => {}
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
                            ViewMode::Interface | ViewMode::Network => {
                                vec![CommandSourceId::Ifconfig]
                            }
                            ViewMode::Connections => vec![CommandSourceId::NetstatConnections],
                            ViewMode::Ports => vec![CommandSourceId::LsofPorts],
                            ViewMode::Routes => routes_raw_sources(&app),
                            ViewMode::Timeline => vec![
                                CommandSourceId::Ifconfig,
                                CommandSourceId::NetstatRoutes,
                                CommandSourceId::DefaultRoute,
                                CommandSourceId::PublicIp,
                                CommandSourceId::GitHubRelease,
                            ],
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
                    KeyCode::Char('u') | KeyCode::Char('ㅕ') => {
                        app.help_visible = false;
                        start_update_check(&mut app, true);
                    }
                    KeyCode::Char('U') => {
                        app.help_visible = false;
                        start_update_install(&mut app, true);
                    }
                    KeyCode::Char('R') => {
                        app.help_visible = false;
                        app.release_notes_viewer.active = true;
                        app.release_notes_viewer.scroll = 0;
                    }
                    KeyCode::Char('j') | KeyCode::Down | KeyCode::Char('ㅓ') => {
                        app.select_next();
                    }
                    KeyCode::Char('k') | KeyCode::Up | KeyCode::Char('ㅏ') => {
                        app.select_previous();
                    }
                    KeyCode::Tab => {
                        if app.view_mode == ViewMode::Routes {
                            app.select_next_route_section();
                        }
                    }
                    KeyCode::BackTab => {
                        if app.view_mode == ViewMode::Routes {
                            app.select_previous_route_section();
                        }
                    }
                    KeyCode::Enter => {
                        if app.view_mode == ViewMode::Routes {
                            app.route_inspector.destination_input_active = true;
                            app.route_inspector.active_section = RouteInspectorSection::PathViewer;
                        }
                    }
                    KeyCode::Char('K') => {
                        app.help_visible = false;
                        if app.view_mode == ViewMode::Ports {
                            // Kill the selected process
                            if let Some(NavigationItem::ListeningPort {
                                pid, command, port, ..
                            }) = app.navigation_items.get(app.selected_index)
                            {
                                let pid = pid.clone();
                                let command = command.clone();
                                let port = port.clone();
                                match run_kill(&pid) {
                                    Ok(()) => {
                                        app.recent_events
                                            .push(lazyifconfig::model::NetworkEvent::new(
                                            lazyifconfig::model::NetworkEventKind::ProcessKilled,
                                            lazyifconfig::model::EventSeverity::Info,
                                            format!(
                                                "Killed {} (PID: {}) on :{}",
                                                command, pid, port
                                            ),
                                        ));
                                        let _ = tick_update(&mut app);
                                        last_tick = std::time::Instant::now();
                                    }
                                    Err(e) => {
                                        app.recent_events.push(
                                            lazyifconfig::model::NetworkEvent::new(
                                                lazyifconfig::model::NetworkEventKind::SystemError,
                                                lazyifconfig::model::EventSeverity::Error,
                                                format!("Kill failed (PID: {}): {}", pid, e),
                                            ),
                                        );
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
                    KeyCode::Char('/') => {
                        app.help_visible = false;
                        match app.view_mode {
                            ViewMode::Ports => app.port_filter_active = true,
                            ViewMode::Routes => app.route_inspector.route_filter_active = true,
                            _ => {}
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
                            if let Some(NavigationItem::Connection { foreign, .. }) =
                                app.navigation_items.get(app.selected_index)
                            {
                                let foreign_ip = if let Some(pos) = foreign.rfind(':') {
                                    &foreign[..pos]
                                } else {
                                    foreign.as_str()
                                };
                                if foreign_ip != "*"
                                    && foreign_ip != "::"
                                    && foreign_ip != "0.0.0.0"
                                    && foreign_ip != "*.*"
                                {
                                    if let Err(e) =
                                        lazyifconfig::command::copy_to_clipboard(foreign_ip)
                                    {
                                        app.recent_events.push(
                                            lazyifconfig::model::NetworkEvent::new(
                                                lazyifconfig::model::NetworkEventKind::SystemError,
                                                lazyifconfig::model::EventSeverity::Error,
                                                format!("Failed to copy IP: {}", e),
                                            ),
                                        );
                                        if app.recent_events.len() > 100 {
                                            let overflow = app.recent_events.len() - 100;
                                            app.recent_events.drain(0..overflow);
                                        }
                                    } else {
                                        app.recent_events.push(
                                            lazyifconfig::model::NetworkEvent::new(
                                                lazyifconfig::model::NetworkEventKind::ActionCopied,
                                                lazyifconfig::model::EventSeverity::Info,
                                                format!("Copied IP {} to clipboard", foreign_ip),
                                            ),
                                        );
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
                            if let Some(NavigationItem::Connection { foreign, .. }) =
                                app.navigation_items.get(app.selected_index)
                            {
                                let foreign_ip = if let Some(pos) = foreign.rfind(':') {
                                    &foreign[..pos]
                                } else {
                                    foreign.as_str()
                                };
                                if foreign_ip != "*"
                                    && foreign_ip != "::"
                                    && foreign_ip != "0.0.0.0"
                                    && foreign_ip != "*.*"
                                {
                                    let mut should_fetch = false;
                                    if let Ok(lock) = app.whois_cache.lock() {
                                        if !lock.contains_key(foreign_ip)
                                            || lock.get(foreign_ip).map(|s| s.as_str())
                                                != Some("Loading...")
                                        {
                                            should_fetch = true;
                                        }
                                    }

                                    if should_fetch {
                                        if let Ok(mut lock) = app.whois_cache.lock() {
                                            lock.insert(
                                                foreign_ip.to_string(),
                                                "Loading...".to_string(),
                                            );
                                        }

                                        app.recent_events.push(
                                            lazyifconfig::model::NetworkEvent::new(
                                                lazyifconfig::model::NetworkEventKind::ActionWhois,
                                                lazyifconfig::model::EventSeverity::Info,
                                                format!("Starting WHOIS lookup for {}", foreign_ip),
                                            ),
                                        );
                                        if app.recent_events.len() > 100 {
                                            let overflow = app.recent_events.len() - 100;
                                            app.recent_events.drain(0..overflow);
                                        }

                                        let cache_clone = app.whois_cache.clone();
                                        let ip_clone = foreign_ip.to_string();

                                        tokio::spawn(async move {
                                            let result =
                                                match lazyifconfig::command::run_whois(&ip_clone) {
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

    #[test]
    fn route_path_lookup_requires_destination() {
        let mut app = App::default();
        app.route_inspector.destination_input = "   ".to_string();
        app.route_inspector.latest_path_result = Some(lazyifconfig::model::RoutePathResult {
            destination: "8.8.8.8".to_string(),
            ..Default::default()
        });

        run_route_path_lookup(&mut app);

        assert!(app.route_inspector.latest_path_result.is_none());
        assert_eq!(
            app.route_inspector.latest_path_error.as_deref(),
            Some("Enter a destination first.")
        );
    }

    #[test]
    fn route_path_command_error_message_uses_literal_destination_label() {
        assert_eq!(
            route_path_command_error_message("lookup failed"),
            "destination could not be resolved by route command: lookup failed"
        );
    }

    #[test]
    fn routes_raw_sources_include_available_optional_outputs_in_order() {
        let mut app = App::default();
        app.command_outputs.insert(
            CommandSourceId::RoutePath,
            CommandOutput {
                command: "ip route get 8.8.8.8".to_string(),
                stdout: String::new(),
                stderr: String::new(),
                executed_at: SystemTime::now(),
                exit_code: Some(0),
            },
        );
        app.command_outputs.insert(
            CommandSourceId::Ipv6Routes,
            CommandOutput {
                command: "ip -6 route show".to_string(),
                stdout: String::new(),
                stderr: String::new(),
                executed_at: SystemTime::now(),
                exit_code: Some(0),
            },
        );

        assert_eq!(
            routes_raw_sources(&app),
            vec![
                CommandSourceId::NetstatRoutes,
                CommandSourceId::DefaultRoute,
                CommandSourceId::Ipv6Routes,
                CommandSourceId::RoutePath,
            ]
        );
    }

    #[test]
    fn raw_viewer_command_to_copy_prefers_captured_command_and_falls_back_to_source_label() {
        let mut app = App::default();
        app.command_outputs.insert(
            CommandSourceId::RoutePath,
            CommandOutput {
                command: "ip route get 8.8.8.8".to_string(),
                stdout: String::new(),
                stderr: String::new(),
                executed_at: SystemTime::now(),
                exit_code: Some(0),
            },
        );

        assert_eq!(
            raw_viewer_command_to_copy(&app, CommandSourceId::RoutePath),
            "ip route get 8.8.8.8"
        );
        assert_eq!(
            raw_viewer_command_to_copy(&app, CommandSourceId::Ifconfig),
            CommandSourceId::Ifconfig.as_str()
        );
    }

    #[test]
    fn additional_linux_ipv6_route_output_is_merged_into_snapshot_routes() {
        let routes = parse_routes("default via 172.17.0.1 dev eth0 proto static metric 100");

        let merged = merge_additional_route_output(
            routes,
            Some("default via fe80::1 dev eth0 proto ra metric 100\n2001:db8::/64 dev eth0 proto kernel metric 256"),
        );

        assert!(merged
            .iter()
            .any(|route| route.family == lazyifconfig::model::RouteFamily::Ipv6));
        assert_eq!(merged.len(), 3);
    }

    #[tokio::test]
    async fn test_tick_update() {
        let mut app = App::default();
        let res = tick_update(&mut app);
        assert!(res.is_ok());
        assert!(app.current_snapshot.is_some());
    }
}
