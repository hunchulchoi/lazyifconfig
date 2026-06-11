use lazyifconfig::app::App;
use lazyifconfig::model::{
    ActiveConnection, InterfaceAddress, InterfaceStats, InterfaceStatus, InterfaceType,
    ListeningPort, NetworkEvent, NetworkInterface, NetworkSnapshot,
};

#[test]
fn snapshot_can_hold_interfaces_and_events() {
    let interface = NetworkInterface {
        name: "en0".to_string(),
        network_kind: lazyifconfig::model::NetworkKind::Unknown,
        interface_type: InterfaceType::WifiOrEthernet,
        status: InterfaceStatus::Up,
        ipv4: vec![InterfaceAddress::new("192.168.0.10")],
        ipv6: vec![],
        mac_address: Some("aa:bb:cc:dd:ee:ff".to_string()),
        mtu: Some(1500),
        stats: Some(InterfaceStats {
            rx_bytes: 100,
            tx_bytes: 50,
            rx_packets: 10,
            tx_packets: 5,
        }),
    };

    let event = NetworkEvent::new(
        lazyifconfig::model::NetworkEventKind::InterfaceAppeared,
        lazyifconfig::model::EventSeverity::Info,
        "en0 appeared".to_string(),
    );

    let snapshot = NetworkSnapshot {
        interfaces: vec![interface],
        connections: vec![],
        listening_ports: vec![],
        routes: vec![],
        captured_at_secs: 10,
    };

    assert_eq!(snapshot.interfaces.len(), 1);
    assert_eq!(snapshot.interfaces[0].stats.as_ref().unwrap().rx_bytes, 100);
    assert_eq!(event.message, "en0 appeared");
}

#[test]
fn replace_snapshot_preserves_selection_by_interface_name() {
    let mut app = App::default();

    app.replace_snapshot(snapshot_with_interfaces(
        10,
        vec![
            interface_with_stats("en0", Some("192.168.0.10"), Some((100, 50))),
            interface_with_stats("utun0", None, Some((20, 10))),
        ],
    ));
    app.selected_index = 1;

    app.replace_snapshot(snapshot_with_interfaces(
        20,
        vec![
            interface_with_stats("lo0", Some("127.0.0.1"), None),
            interface_with_stats("utun0", None, Some((40, 30))),
            interface_with_stats("en0", Some("192.168.0.10"), Some((200, 80))),
        ],
    ));

    assert_eq!(app.selected_interface_name(), Some("utun0"));
    assert_eq!(app.selected_index, 1);
}

#[test]
fn selected_rates_are_computed_from_consecutive_snapshots() {
    let mut app = App::default();

    app.replace_snapshot(snapshot_with_interfaces(
        10,
        vec![interface_with_stats(
            "en0",
            Some("192.168.0.10"),
            Some((1_000, 400)),
        )],
    ));
    app.replace_snapshot(snapshot_with_interfaces(
        15,
        vec![interface_with_stats(
            "en0",
            Some("192.168.0.10"),
            Some((1_600, 700)),
        )],
    ));

    assert_eq!(app.selected_rates(), Some((120, 60)));
}

#[test]
fn view_mode_cycles_in_top_tab_order() {
    let mut app = App::default();

    app.select_next_view_mode();
    assert_eq!(app.view_mode, lazyifconfig::app::ViewMode::Network);

    app.select_next_view_mode();
    assert_eq!(app.view_mode, lazyifconfig::app::ViewMode::Ports);

    app.select_next_view_mode();
    assert_eq!(app.view_mode, lazyifconfig::app::ViewMode::Connections);

    app.select_next_view_mode();
    assert_eq!(app.view_mode, lazyifconfig::app::ViewMode::Routes);

    app.select_next_view_mode();
    assert_eq!(app.view_mode, lazyifconfig::app::ViewMode::Tools);

    app.select_next_view_mode();
    assert_eq!(app.view_mode, lazyifconfig::app::ViewMode::Timeline);

    app.select_next_view_mode();
    assert_eq!(app.view_mode, lazyifconfig::app::ViewMode::Interface);

    app.select_previous_view_mode();
    assert_eq!(app.view_mode, lazyifconfig::app::ViewMode::Timeline);
}

#[test]
fn tools_view_is_in_tab_cycle_between_routes_and_timeline() {
    let mut app = App::default();

    app.set_view_mode(lazyifconfig::app::ViewMode::Routes);
    app.select_next_view_mode();
    assert_eq!(app.view_mode, lazyifconfig::app::ViewMode::Tools);

    app.select_next_view_mode();
    assert_eq!(app.view_mode, lazyifconfig::app::ViewMode::Timeline);
}

#[test]
fn tools_state_selects_follow_up_registry_entries_and_keeps_them_runnable() {
    let mut app = App::default();
    app.set_view_mode(lazyifconfig::app::ViewMode::Tools);

    assert_eq!(
        app.tools.selected_tool_id(),
        lazyifconfig::tools::ToolId::DnsLookup
    );
    app.tools.select_next_tool();
    assert_eq!(
        app.tools.selected_tool_id(),
        lazyifconfig::tools::ToolId::WhoisLookup
    );
    assert!(app.tools.selected_tool_is_runnable());
}

#[test]
fn tools_input_editing_updates_selected_field() {
    let mut app = App::default();
    app.set_view_mode(lazyifconfig::app::ViewMode::Tools);

    app.tools.open_input_modal();
    app.tools.push_input_char('g');
    app.tools.push_input_char('h');

    let value = app
        .tools
        .input_for_selected_tool()
        .get("target")
        .unwrap()
        .to_string();
    assert_eq!(value, "gh");
}

#[test]
fn tools_input_editing_ignores_lines_after_first_when_pasting() {
    let mut app = App::default();
    app.set_view_mode(lazyifconfig::app::ViewMode::Tools);

    app.tools.open_input_modal();
    app.tools.push_input_text("github.com\nsecond-line");

    let value = app
        .tools
        .input_for_selected_tool()
        .get("target")
        .unwrap()
        .to_string();
    assert_eq!(value, "github.com");
}

#[test]
fn tools_validation_flags_invalid_port_and_target_formats() {
    let mut app = App::default();
    app.set_view_mode(lazyifconfig::app::ViewMode::Tools);

    app.tools.select_next_tool();
    app.tools.select_next_tool();
    app.tools.select_next_tool();
    app.tools.open_input_modal();
    app.tools.push_input_text("github.com");
    app.tools.select_next_field();
    app.tools.push_input_text("70000");

    let errors = app.tools.selected_input_validation_errors();
    assert!(errors.iter().any(|line| line.contains("Port must be a number from 1 to 65535.")));

    app.tools.select_next_tool();
    app.tools.open_input_modal();
    app.tools.push_input_text("github.com:not-a-port");
    let tls_errors = app.tools.selected_input_validation_errors();
    assert!(tls_errors
        .iter()
        .any(|line| line.contains("Target must look like host:port or host.")));
}

#[test]
fn tools_input_modal_tracks_editing_scope() {
    let mut app = App::default();
    app.set_view_mode(lazyifconfig::app::ViewMode::Tools);

    assert!(!app.tools.input_modal_open);
    assert!(!app.tools.editing_input);

    app.tools.open_input_modal();
    assert!(app.tools.input_modal_open);
    assert!(app.tools.editing_input);

    app.tools.close_input_modal();
    assert!(!app.tools.input_modal_open);
    assert!(!app.tools.editing_input);
}

#[test]
fn ports_filter_then_sort_by_port_number() {
    let mut app = App::default();
    app.view_mode = lazyifconfig::app::ViewMode::Ports;
    app.port_filter = "server".to_string();

    app.replace_snapshot(snapshot_with_ports(vec![
        listening_port("tcp", "9000", "server-b", "300", "alice"),
        listening_port("tcp", "443", "client", "100", "bob"),
        listening_port("tcp", "8080", "server-a", "200", "carol"),
    ]));

    let ports: Vec<String> = app
        .navigation_items
        .iter()
        .map(|item| match item {
            lazyifconfig::app::NavigationItem::ListeningPort { port, .. } => port.clone(),
            other => panic!("expected port item, got {other:?}"),
        })
        .collect();

    assert_eq!(ports, vec!["8080", "9000"]);
}

#[test]
fn ports_sort_column_cycles_and_direction_toggles() {
    let mut app = App::default();
    app.view_mode = lazyifconfig::app::ViewMode::Ports;
    app.replace_snapshot(snapshot_with_ports(vec![
        listening_port("tcp", "9000", "zulu", "300", "alice"),
        listening_port("tcp", "8080", "alpha", "200", "carol"),
    ]));

    app.cycle_port_sort_column();
    assert_eq!(
        app.port_sort_column,
        lazyifconfig::app::PortSortColumn::Command
    );
    assert_eq!(
        app.port_sort_direction,
        lazyifconfig::app::SortDirection::Ascending
    );

    app.update_navigation_items();
    let first_command = match &app.navigation_items[0] {
        lazyifconfig::app::NavigationItem::ListeningPort { command, .. } => command.as_str(),
        other => panic!("expected port item, got {other:?}"),
    };
    assert_eq!(first_command, "alpha");

    app.toggle_port_sort_direction();
    assert_eq!(
        app.port_sort_direction,
        lazyifconfig::app::SortDirection::Descending
    );

    app.update_navigation_items();
    let first_command = match &app.navigation_items[0] {
        lazyifconfig::app::NavigationItem::ListeningPort { command, .. } => command.as_str(),
        other => panic!("expected port item, got {other:?}"),
    };
    assert_eq!(first_command, "zulu");
}

#[test]
fn connections_filter_then_sort_by_local_address() {
    let mut app = App::default();
    app.view_mode = lazyifconfig::app::ViewMode::Connections;
    app.connection_filter = "est".to_string();

    app.replace_snapshot(snapshot_with_connections(vec![
        active_connection(
            "tcp",
            "127.0.0.1",
            "9000",
            "1.1.1.1",
            "443",
            Some("ESTABLISHED"),
        ),
        active_connection("udp", "127.0.0.1", "53", "*", "*", None),
        active_connection(
            "tcp",
            "127.0.0.1",
            "8080",
            "8.8.8.8",
            "443",
            Some("ESTABLISHED"),
        ),
    ]));

    let locals: Vec<(String, String)> = app
        .navigation_items
        .iter()
        .map(|item| match item {
            lazyifconfig::app::NavigationItem::Connection {
                local_ip,
                local_port,
                ..
            } => (local_ip.clone(), local_port.clone()),
            other => panic!("expected connection item, got {other:?}"),
        })
        .collect();

    assert_eq!(
        locals,
        vec![
            ("127.0.0.1".to_string(), "8080".to_string()),
            ("127.0.0.1".to_string(), "9000".to_string())
        ]
    );
}

#[test]
fn connections_sort_column_cycles_and_direction_toggles() {
    let mut app = App::default();
    app.view_mode = lazyifconfig::app::ViewMode::Connections;
    app.replace_snapshot(snapshot_with_connections(vec![
        active_connection(
            "tcp",
            "127.0.0.1",
            "9000",
            "9.9.9.9",
            "443",
            Some("ESTABLISHED"),
        ),
        active_connection(
            "tcp",
            "127.0.0.1",
            "8080",
            "1.1.1.1",
            "443",
            Some("CLOSE_WAIT"),
        ),
    ]));

    app.cycle_connection_sort_column();
    assert_eq!(
        app.connection_sort_column,
        lazyifconfig::app::ConnectionSortColumn::Foreign
    );
    assert_eq!(
        app.connection_sort_direction,
        lazyifconfig::app::SortDirection::Ascending
    );

    app.update_navigation_items();
    let first_foreign = match &app.navigation_items[0] {
        lazyifconfig::app::NavigationItem::Connection {
            foreign_ip,
            foreign_port,
            ..
        } => (foreign_ip.as_str(), foreign_port.as_str()),
        other => panic!("expected connection item, got {other:?}"),
    };
    assert_eq!(first_foreign, ("1.1.1.1", "443"));

    app.toggle_connection_sort_direction();
    assert_eq!(
        app.connection_sort_direction,
        lazyifconfig::app::SortDirection::Descending
    );

    app.update_navigation_items();
    let first_foreign = match &app.navigation_items[0] {
        lazyifconfig::app::NavigationItem::Connection {
            foreign_ip,
            foreign_port,
            ..
        } => (foreign_ip.as_str(), foreign_port.as_str()),
        other => panic!("expected connection item, got {other:?}"),
    };
    assert_eq!(first_foreign, ("9.9.9.9", "443"));
}

#[test]
fn replace_snapshot_does_not_emit_events_for_first_snapshot() {
    let mut app = App::default();

    app.replace_snapshot(snapshot_with_interfaces(
        10,
        vec![interface_with_stats(
            "en0",
            Some("192.168.0.10"),
            Some((100, 50)),
        )],
    ));

    assert!(app.recent_events.is_empty());
}

#[test]
fn replace_snapshot_emits_appearance_event() {
    let mut app = App::default();

    app.replace_snapshot(snapshot_with_interfaces(10, vec![]));
    app.replace_snapshot(snapshot_with_interfaces(
        20,
        vec![interface_with_stats(
            "en0",
            Some("192.168.0.10"),
            Some((100, 50)),
        )],
    ));

    assert_eq!(app.recent_events.len(), 2);
    assert_eq!(app.recent_events[0].message, "en0 appeared");
    assert_eq!(
        app.recent_events[0].kind,
        lazyifconfig::model::NetworkEventKind::InterfaceAppeared
    );
    assert_eq!(app.recent_events[1].message, "en0: added IPv4 192.168.0.10");
    assert_eq!(
        app.recent_events[1].kind,
        lazyifconfig::model::NetworkEventKind::Ipv4Added
    );
}

#[test]
fn replace_snapshot_emits_disappearance_event() {
    let mut app = App::default();

    app.replace_snapshot(snapshot_with_interfaces(
        10,
        vec![interface_with_stats(
            "en0",
            Some("192.168.0.10"),
            Some((100, 50)),
        )],
    ));
    app.replace_snapshot(snapshot_with_interfaces(20, vec![]));

    assert_eq!(app.recent_events.len(), 1);
    assert_eq!(app.recent_events[0].message, "en0 disappeared");
    assert_eq!(
        app.recent_events[0].kind,
        lazyifconfig::model::NetworkEventKind::InterfaceRemoved
    );
}

#[test]
fn replace_snapshot_emits_status_change_event() {
    let mut app = App::default();
    app.show_all = true;

    app.replace_snapshot(snapshot_with_interfaces(
        10,
        vec![interface_with_status(
            "en0",
            InterfaceStatus::Up,
            Some("192.168.0.10"),
            Some((100, 50)),
        )],
    ));
    app.replace_snapshot(snapshot_with_interfaces(
        20,
        vec![interface_with_status(
            "en0",
            InterfaceStatus::Down,
            Some("192.168.0.10"),
            Some((200, 150)),
        )],
    ));

    assert_eq!(app.recent_events.len(), 1);
    assert_eq!(
        app.recent_events[0].message,
        "en0 status changed: up -> down"
    );
    assert_eq!(
        app.recent_events[0].kind,
        lazyifconfig::model::NetworkEventKind::InterfaceDown
    );
}

#[test]
fn replace_snapshot_emits_ipv4_change_event() {
    let mut app = App::default();

    app.replace_snapshot(snapshot_with_interfaces(
        10,
        vec![interface_with_stats(
            "en0",
            Some("192.168.0.10"),
            Some((100, 50)),
        )],
    ));
    app.replace_snapshot(snapshot_with_interfaces(
        20,
        vec![interface_with_stats(
            "en0",
            Some("192.168.0.11"),
            Some((200, 150)),
        )],
    ));

    assert_eq!(app.recent_events.len(), 1);
    assert_eq!(
        app.recent_events[0].message,
        "en0: 192.168.0.10 -> 192.168.0.11"
    );
    assert_eq!(
        app.recent_events[0].kind,
        lazyifconfig::model::NetworkEventKind::Ipv4Changed
    );
}

#[test]
fn replace_snapshot_keeps_only_most_recent_fifty_events() {
    let mut app = App::default();

    app.replace_snapshot(snapshot_with_interfaces(
        0,
        vec![interface_with_stats(
            "en0",
            Some("192.168.0.0"),
            Some((0, 0)),
        )],
    ));

    for idx in 1..=110 {
        app.replace_snapshot(snapshot_with_interfaces(
            idx,
            vec![interface_with_stats(
                "en0",
                Some(&format!("192.168.0.{idx}")),
                Some((idx * 100, idx * 50)),
            )],
        ));
    }

    assert_eq!(app.recent_events.len(), 100);
    assert_eq!(
        app.recent_events
            .first()
            .map(|event| event.message.as_str()),
        Some("en0: 192.168.0.10 -> 192.168.0.11")
    );
    assert_eq!(
        app.recent_events.last().map(|event| event.message.as_str()),
        Some("en0: 192.168.0.109 -> 192.168.0.110")
    );
}

fn snapshot_with_interfaces(
    captured_at_secs: u64,
    interfaces: Vec<NetworkInterface>,
) -> NetworkSnapshot {
    NetworkSnapshot {
        interfaces,
        connections: vec![],
        listening_ports: vec![],
        routes: vec![],
        captured_at_secs,
    }
}

fn snapshot_with_ports(listening_ports: Vec<ListeningPort>) -> NetworkSnapshot {
    NetworkSnapshot {
        interfaces: vec![],
        connections: vec![],
        listening_ports,
        routes: vec![],
        captured_at_secs: 10,
    }
}

fn snapshot_with_connections(connections: Vec<ActiveConnection>) -> NetworkSnapshot {
    NetworkSnapshot {
        interfaces: vec![],
        connections,
        listening_ports: vec![],
        routes: vec![],
        captured_at_secs: 10,
    }
}

fn active_connection(
    proto: &str,
    local_ip: &str,
    local_port: &str,
    foreign_ip: &str,
    foreign_port: &str,
    state: Option<&str>,
) -> ActiveConnection {
    ActiveConnection {
        proto: proto.to_string(),
        local_ip: local_ip.to_string(),
        local_port: local_port.to_string(),
        foreign_ip: foreign_ip.to_string(),
        foreign_port: foreign_port.to_string(),
        state: state.map(str::to_string),
    }
}

fn listening_port(
    proto: &str,
    local_port: &str,
    command: &str,
    pid: &str,
    user: &str,
) -> ListeningPort {
    ListeningPort {
        proto: proto.to_string(),
        local_ip: "0.0.0.0".to_string(),
        local_port: local_port.to_string(),
        pid: pid.to_string(),
        command: command.to_string(),
        user: user.to_string(),
    }
}

fn interface_with_stats(
    name: &str,
    ipv4: Option<&str>,
    stats: Option<(u64, u64)>,
) -> NetworkInterface {
    interface_with_status(name, InterfaceStatus::Up, ipv4, stats)
}

fn interface_with_status(
    name: &str,
    status: InterfaceStatus,
    ipv4: Option<&str>,
    stats: Option<(u64, u64)>,
) -> NetworkInterface {
    NetworkInterface {
        name: name.to_string(),
        network_kind: lazyifconfig::model::NetworkKind::Unknown,
        interface_type: InterfaceType::WifiOrEthernet,
        status,
        ipv4: ipv4.into_iter().map(InterfaceAddress::new).collect(),
        ipv6: vec![],
        mac_address: None,
        mtu: Some(1500),
        stats: stats.map(|(rx_bytes, tx_bytes)| InterfaceStats {
            rx_bytes,
            tx_bytes,
            rx_packets: 0,
            tx_packets: 0,
        }),
    }
}

#[test]
fn test_app_navigation() {
    let mut app = App::default();
    app.replace_snapshot(NetworkSnapshot {
        interfaces: vec![
            interface_with_stats("lo0", None, None),
            interface_with_stats("en0", None, None),
            interface_with_stats("utun0", None, None),
        ],
        connections: vec![],
        listening_ports: vec![],
        routes: vec![],
        captured_at_secs: 10,
    });
    assert_eq!(app.selected_index, 0);

    app.select_next();
    assert_eq!(app.selected_index, 1);

    app.select_next();
    assert_eq!(app.selected_index, 2);

    app.select_next();
    assert_eq!(app.selected_index, 0);

    app.select_previous();
    assert_eq!(app.selected_index, 2);
}

#[test]
fn test_app_network_view_grouping() {
    let mut app = App::default();
    let en0 = NetworkInterface {
        name: "en0".to_string(),
        network_kind: lazyifconfig::model::NetworkKind::Lan,
        interface_type: lazyifconfig::model::InterfaceType::WifiOrEthernet,
        status: lazyifconfig::model::InterfaceStatus::Up,
        ipv4: vec![lazyifconfig::model::InterfaceAddress {
            value: "192.168.0.15".to_string(),
            prefix_len: Some(24),
            gateway: None,
        }],
        ipv6: vec![],
        mac_address: None,
        mtu: None,
        stats: None,
    };
    let lo0 = NetworkInterface {
        name: "lo0".to_string(),
        network_kind: lazyifconfig::model::NetworkKind::Loopback,
        interface_type: lazyifconfig::model::InterfaceType::Loopback,
        status: lazyifconfig::model::InterfaceStatus::Up,
        ipv4: vec![lazyifconfig::model::InterfaceAddress {
            value: "127.0.0.1".to_string(),
            prefix_len: Some(8),
            gateway: None,
        }],
        ipv6: vec![],
        mac_address: None,
        mtu: None,
        stats: None,
    };

    app.replace_snapshot(lazyifconfig::model::NetworkSnapshot {
        interfaces: vec![en0, lo0],
        connections: vec![],
        listening_ports: vec![],
        routes: vec![],
        captured_at_secs: 100,
    });

    // 기본 뷰 모드는 Interface
    assert_eq!(app.view_mode, lazyifconfig::app::ViewMode::Interface);

    // 네트워크 뷰로 전환
    app.set_view_mode(lazyifconfig::app::ViewMode::Network);
    assert_eq!(app.view_mode, lazyifconfig::app::ViewMode::Network);

    // navigation_items 검증: SubnetHeader(127.0.0.0/8) -> lo0 -> SubnetHeader(192.168.0.0/24) -> en0
    assert!(matches!(
        app.navigation_items[0],
        lazyifconfig::app::NavigationItem::SubnetHeader(_)
    ));
    assert!(matches!(
        app.navigation_items[1],
        lazyifconfig::app::NavigationItem::Interface { .. }
    ));
}

#[test]
fn test_traffic_history_bounding_and_cleanup() {
    let mut app = App::default();

    for idx in 0..=50 {
        let stats = Some(InterfaceStats {
            rx_bytes: idx as u64 * 100,
            tx_bytes: idx as u64 * 50,
            rx_packets: 0,
            tx_packets: 0,
        });

        let interface = NetworkInterface {
            name: "en0".to_string(),
            network_kind: lazyifconfig::model::NetworkKind::Lan,
            interface_type: InterfaceType::WifiOrEthernet,
            status: InterfaceStatus::Up,
            ipv4: vec![],
            ipv6: vec![],
            mac_address: None,
            mtu: None,
            stats,
        };

        app.replace_snapshot(NetworkSnapshot {
            interfaces: vec![interface],
            connections: vec![],
            listening_ports: vec![],
            routes: vec![],
            captured_at_secs: idx as u64,
        });
    }

    let history = app.traffic_history.get("en0").unwrap();
    assert_eq!(history.rx_rates.len(), 40);
    assert_eq!(history.tx_rates.len(), 40);
    assert_eq!(history.rx_rates[0], 100);
    assert_eq!(history.tx_rates[0], 50);

    app.replace_snapshot(NetworkSnapshot {
        interfaces: vec![],
        connections: vec![],
        listening_ports: vec![],
        routes: vec![],
        captured_at_secs: 100,
    });
    assert!(app.traffic_history.get("en0").is_none());
}

#[test]
fn test_event_timeline_functionality() {
    let mut app = App::default();

    app.recent_events.push(NetworkEvent::new(
        lazyifconfig::model::NetworkEventKind::VpnConnected,
        lazyifconfig::model::EventSeverity::Info,
        "utun0 connected".to_string(),
    ));
    app.recent_events.push(NetworkEvent::new(
        lazyifconfig::model::NetworkEventKind::InterfaceDown,
        lazyifconfig::model::EventSeverity::Warning,
        "en0 status changed: up -> down".to_string(),
    ));

    app.set_view_mode(lazyifconfig::app::ViewMode::Timeline);
    assert_eq!(app.view_mode, lazyifconfig::app::ViewMode::Timeline);
    assert_eq!(app.navigation_items.len(), 2);

    if let lazyifconfig::app::NavigationItem::Event {
        index,
        kind,
        message,
        ..
    } = &app.navigation_items[0]
    {
        assert_eq!(*index, 0);
        assert_eq!(*kind, lazyifconfig::model::NetworkEventKind::VpnConnected);
        assert_eq!(message, "utun0 connected");
    } else {
        panic!("Expected NavigationItem::Event at index 0");
    }

    if let lazyifconfig::app::NavigationItem::Event {
        index,
        kind,
        message,
        ..
    } = &app.navigation_items[1]
    {
        assert_eq!(*index, 1);
        assert_eq!(*kind, lazyifconfig::model::NetworkEventKind::InterfaceDown);
        assert_eq!(message, "en0 status changed: up -> down");
    } else {
        panic!("Expected NavigationItem::Event at index 1");
    }
}

#[test]
fn test_routes_navigation() {
    let mut app = App::default();
    let r1 = lazyifconfig::model::RouteEntry::new("default", "192.168.0.1", "en0");
    let r2 = lazyifconfig::model::RouteEntry::new("10.8.0.0/24", "10.8.0.1", "utun4");

    app.replace_snapshot(NetworkSnapshot {
        interfaces: vec![],
        connections: vec![],
        listening_ports: vec![],
        routes: vec![r1, r2],
        captured_at_secs: 10,
    });

    // Switch to Routes view
    app.set_view_mode(lazyifconfig::app::ViewMode::Routes);
    assert_eq!(app.view_mode, lazyifconfig::app::ViewMode::Routes);
    assert_eq!(app.navigation_items.len(), 2);

    if let lazyifconfig::app::NavigationItem::Route {
        destination,
        gateway,
        interface,
        index,
    } = &app.navigation_items[0]
    {
        assert_eq!(destination, "10.8.0.0/24");
        assert_eq!(gateway, "10.8.0.1");
        assert_eq!(interface, "utun4");
        assert_eq!(*index, 1);
    } else {
        panic!("Expected NavigationItem::Route at index 0");
    }

    if let lazyifconfig::app::NavigationItem::Route {
        destination,
        gateway,
        interface,
        index,
    } = &app.navigation_items[1]
    {
        assert_eq!(destination, "default");
        assert_eq!(gateway, "192.168.0.1");
        assert_eq!(interface, "en0");
        assert_eq!(*index, 0);
    } else {
        panic!("Expected NavigationItem::Route at index 1");
    }

    assert_eq!(app.selected_index, 0);
    app.select_next();
    assert_eq!(app.selected_index, 1);
    app.select_next();
    assert_eq!(app.selected_index, 0);
}

#[test]
fn route_filter_matches_destination_gateway_and_interface() {
    let mut app = App::default();
    app.replace_snapshot(NetworkSnapshot {
        interfaces: vec![],
        connections: vec![],
        listening_ports: vec![],
        routes: vec![
            lazyifconfig::model::RouteEntry::new("default", "192.168.0.1", "en0"),
            lazyifconfig::model::RouteEntry::new("10.8.0.0/24", "link", "utun4"),
        ],
        captured_at_secs: 10,
    });

    app.set_view_mode(lazyifconfig::app::ViewMode::Routes);
    app.route_inspector.route_filter = "utun".to_string();
    app.update_navigation_items();

    assert_eq!(app.navigation_items.len(), 1);
    match &app.navigation_items[0] {
        lazyifconfig::app::NavigationItem::Route { interface, .. } => {
            assert_eq!(interface, "utun4")
        }
        other => panic!("expected route item, got {other:?}"),
    }
}

#[test]
fn route_inspector_sections_cycle_without_leaving_routes_view() {
    let mut app = App::default();

    assert_eq!(
        app.route_inspector.active_section,
        lazyifconfig::model::RouteInspectorSection::Summary
    );

    app.select_next_route_section();
    assert_eq!(
        app.route_inspector.active_section,
        lazyifconfig::model::RouteInspectorSection::PathViewer
    );

    app.select_previous_route_section();
    assert_eq!(
        app.route_inspector.active_section,
        lazyifconfig::model::RouteInspectorSection::Summary
    );
}

#[test]
fn route_diagnostics_refresh_when_snapshot_is_replaced() {
    let mut app = App::default();

    app.replace_snapshot(NetworkSnapshot {
        interfaces: vec![],
        connections: vec![],
        listening_ports: vec![],
        routes: vec![],
        captured_at_secs: 10,
    });

    assert!(app
        .route_inspector
        .diagnostics
        .iter()
        .any(|item| item.title == "No default route"));
}

#[test]
fn route_diagnostics_use_down_interfaces_when_show_all_is_false() {
    let mut app = App::default();

    app.replace_snapshot(NetworkSnapshot {
        interfaces: vec![interface_with_status(
            "en0",
            InterfaceStatus::Down,
            Some("192.168.0.10"),
            None,
        )],
        connections: vec![],
        listening_ports: vec![],
        routes: vec![lazyifconfig::model::RouteEntry::new(
            "default",
            "192.168.0.1",
            "en0",
        )],
        captured_at_secs: 10,
    });

    assert!(app
        .route_inspector
        .diagnostics
        .iter()
        .any(|item| item.title == "Route interface is down"));
}

#[test]
fn route_filter_clears_when_leaving_routes_view() {
    let mut app = App::default();

    app.route_inspector.route_filter = "utun".to_string();
    app.route_inspector.route_filter_active = true;

    app.set_view_mode(lazyifconfig::app::ViewMode::Routes);
    app.set_view_mode(lazyifconfig::app::ViewMode::Interface);

    assert!(app.route_inspector.route_filter.is_empty());
    assert!(!app.route_inspector.route_filter_active);
}

#[test]
fn test_raw_viewer_search_highlights() {
    let mut app = App::default();
    let source_id = lazyifconfig::model::CommandSourceId::Ifconfig;

    let stdout = "en0: flags=8863<UP,BROADCAST,SMART,RUNNING,SIMPLEX,MULTICAST> mtu 1500\n\
                  ether aa:bb:cc:dd:ee:ff\n\
                  inet 192.168.1.12 netmask 0xffffff00 broadcast 192.168.1.255\n\
                  한글 테스트 문장입니다. test line with unicode.";
    let stderr = "some error message";

    app.command_outputs.insert(
        source_id,
        lazyifconfig::model::CommandOutput {
            command: "ifconfig".to_string(),
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
            executed_at: std::time::SystemTime::now(),
            exit_code: Some(0),
        },
    );

    app.raw_viewer.sources = vec![source_id];
    app.raw_viewer.selected_index = 0;

    // Search query: "inet" (case-insensitive test)
    app.raw_viewer.search_query = "iNeT".to_string();
    app.update_raw_viewer_search_matches();

    assert_eq!(app.raw_viewer.search_matches.len(), 1);
    assert_eq!(app.raw_viewer.search_matches[0].line_index, 2);
    assert_eq!(app.raw_viewer.search_matches[0].start_byte, 0);
    assert_eq!(app.raw_viewer.search_matches[0].end_byte, 4);

    // Unicode search test
    app.raw_viewer.search_query = "테스트".to_string();
    app.update_raw_viewer_search_matches();
    assert_eq!(app.raw_viewer.search_matches.len(), 1);
    assert_eq!(app.raw_viewer.search_matches[0].line_index, 3);

    let text = format!("{}\n{}", stdout, stderr);
    let line_content = text.lines().nth(3).unwrap();
    let m = app.raw_viewer.search_matches[0];
    assert!(line_content.is_char_boundary(m.start_byte));
    assert!(line_content.is_char_boundary(m.end_byte));
    assert_eq!(&line_content[m.start_byte..m.end_byte], "테스트");
}
