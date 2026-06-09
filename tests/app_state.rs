use lazyifconfig::app::App;
use lazyifconfig::model::{
    InterfaceAddress, InterfaceStats, InterfaceStatus, InterfaceType, NetworkEvent,
    NetworkInterface, NetworkSnapshot,
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
        vec![interface_with_stats("en0", Some("192.168.0.10"), Some((1_000, 400)))],
    ));
    app.replace_snapshot(snapshot_with_interfaces(
        15,
        vec![interface_with_stats("en0", Some("192.168.0.10"), Some((1_600, 700)))],
    ));

    assert_eq!(app.selected_rates(), Some((120, 60)));
}

#[test]
fn replace_snapshot_does_not_emit_events_for_first_snapshot() {
    let mut app = App::default();

    app.replace_snapshot(snapshot_with_interfaces(
        10,
        vec![interface_with_stats("en0", Some("192.168.0.10"), Some((100, 50)))],
    ));

    assert!(app.recent_events.is_empty());
}

#[test]
fn replace_snapshot_emits_appearance_event() {
    let mut app = App::default();

    app.replace_snapshot(snapshot_with_interfaces(10, vec![]));
    app.replace_snapshot(snapshot_with_interfaces(
        20,
        vec![interface_with_stats("en0", Some("192.168.0.10"), Some((100, 50)))],
    ));

    assert_eq!(app.recent_events.len(), 2);
    assert_eq!(app.recent_events[0].message, "en0 appeared");
    assert_eq!(app.recent_events[0].kind, lazyifconfig::model::NetworkEventKind::InterfaceAppeared);
    assert_eq!(app.recent_events[1].message, "en0: added IPv4 192.168.0.10");
    assert_eq!(app.recent_events[1].kind, lazyifconfig::model::NetworkEventKind::Ipv4Added);
}

#[test]
fn replace_snapshot_emits_disappearance_event() {
    let mut app = App::default();

    app.replace_snapshot(snapshot_with_interfaces(
        10,
        vec![interface_with_stats("en0", Some("192.168.0.10"), Some((100, 50)))],
    ));
    app.replace_snapshot(snapshot_with_interfaces(20, vec![]));

    assert_eq!(app.recent_events.len(), 1);
    assert_eq!(app.recent_events[0].message, "en0 disappeared");
    assert_eq!(app.recent_events[0].kind, lazyifconfig::model::NetworkEventKind::InterfaceRemoved);
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
    assert_eq!(app.recent_events[0].message, "en0 status changed: up -> down");
    assert_eq!(app.recent_events[0].kind, lazyifconfig::model::NetworkEventKind::InterfaceDown);
}

#[test]
fn replace_snapshot_emits_ipv4_change_event() {
    let mut app = App::default();

    app.replace_snapshot(snapshot_with_interfaces(
        10,
        vec![interface_with_stats("en0", Some("192.168.0.10"), Some((100, 50)))],
    ));
    app.replace_snapshot(snapshot_with_interfaces(
        20,
        vec![interface_with_stats("en0", Some("192.168.0.11"), Some((200, 150)))],
    ));

    assert_eq!(app.recent_events.len(), 1);
    assert_eq!(
        app.recent_events[0].message,
        "en0: 192.168.0.10 -> 192.168.0.11"
    );
    assert_eq!(app.recent_events[0].kind, lazyifconfig::model::NetworkEventKind::Ipv4Changed);
}

#[test]
fn replace_snapshot_keeps_only_most_recent_fifty_events() {
    let mut app = App::default();

    app.replace_snapshot(snapshot_with_interfaces(
        0,
        vec![interface_with_stats("en0", Some("192.168.0.0"), Some((0, 0)))],
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
        app.recent_events.first().map(|event| event.message.as_str()),
        Some("en0: 192.168.0.10 -> 192.168.0.11")
    );
    assert_eq!(
        app.recent_events.last().map(|event| event.message.as_str()),
        Some("en0: 192.168.0.109 -> 192.168.0.110")
    );
}

fn snapshot_with_interfaces(captured_at_secs: u64, interfaces: Vec<NetworkInterface>) -> NetworkSnapshot {
    NetworkSnapshot {
        interfaces,
        connections: vec![],
        listening_ports: vec![],
        routes: vec![],
        captured_at_secs,
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
    assert!(matches!(app.navigation_items[0], lazyifconfig::app::NavigationItem::SubnetHeader(_)));
    assert!(matches!(app.navigation_items[1], lazyifconfig::app::NavigationItem::Interface { .. }));
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

    if let lazyifconfig::app::NavigationItem::Event { index, kind, message, .. } = &app.navigation_items[0] {
        assert_eq!(*index, 0);
        assert_eq!(*kind, lazyifconfig::model::NetworkEventKind::VpnConnected);
        assert_eq!(message, "utun0 connected");
    } else {
        panic!("Expected NavigationItem::Event at index 0");
    }

    if let lazyifconfig::app::NavigationItem::Event { index, kind, message, .. } = &app.navigation_items[1] {
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
    let r1 = lazyifconfig::model::RouteEntry {
        destination: "default".to_string(),
        gateway: "192.168.0.1".to_string(),
        interface: "en0".to_string(),
    };
    let r2 = lazyifconfig::model::RouteEntry {
        destination: "10.8.0.0/24".to_string(),
        gateway: "10.8.0.1".to_string(),
        interface: "utun4".to_string(),
    };

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

    if let lazyifconfig::app::NavigationItem::Route { destination, gateway, interface, index } = &app.navigation_items[0] {
        assert_eq!(destination, "default");
        assert_eq!(gateway, "192.168.0.1");
        assert_eq!(interface, "en0");
        assert_eq!(*index, 0);
    } else {
        panic!("Expected NavigationItem::Route at index 0");
    }

    if let lazyifconfig::app::NavigationItem::Route { destination, gateway, interface, index } = &app.navigation_items[1] {
        assert_eq!(destination, "10.8.0.0/24");
        assert_eq!(gateway, "10.8.0.1");
        assert_eq!(interface, "utun4");
        assert_eq!(*index, 1);
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
fn test_raw_viewer_search_highlights() {
    let mut app = App::default();
    let source_id = lazyifconfig::model::CommandSourceId::Ifconfig;
    
    let stdout = "en0: flags=8863<UP,BROADCAST,SMART,RUNNING,SIMPLEX,MULTICAST> mtu 1500\n\
                  ether aa:bb:cc:dd:ee:ff\n\
                  inet 192.168.1.12 netmask 0xffffff00 broadcast 192.168.1.255\n\
                  한글 테스트 문장입니다. test line with unicode.";
    let stderr = "some error message";
    
    app.command_outputs.insert(source_id, lazyifconfig::model::CommandOutput {
        command: "ifconfig".to_string(),
        stdout: stdout.to_string(),
        stderr: stderr.to_string(),
        executed_at: std::time::SystemTime::now(),
        exit_code: Some(0),
    });
    
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


