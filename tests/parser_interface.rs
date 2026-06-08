use lazyifconfig::collector::interface::parse_interfaces;
use lazyifconfig::collector::stats::merge_stats;
use lazyifconfig::model::{InterfaceStatus, InterfaceType};

#[test]
fn parses_en0_from_fixture() {
    let input = include_str!("../fixtures/macos14.txt");

    let interfaces = parse_interfaces(input);
    let en0 = interfaces.iter().find(|item| item.name == "en0").unwrap();

    assert_eq!(en0.interface_type, InterfaceType::WifiOrEthernet);
    assert_eq!(en0.status, InterfaceStatus::Up);
    assert_eq!(en0.mac_address.as_deref(), Some("aa:bb:cc:dd:ee:ff"));
    assert_eq!(en0.mtu, Some(1500));
    assert_eq!(en0.ipv4[0].value, "192.168.0.10");
    assert_eq!(en0.ipv6[0].value, "fe80::1234%en0");
    assert_eq!(en0.stats, None);
}

#[test]
fn parses_utun_interface_from_vpn_fixture() {
    let input = include_str!("../fixtures/vpn.txt");

    let interfaces = parse_interfaces(input);
    let utun4 = interfaces.iter().find(|item| item.name == "utun4").unwrap();

    assert_eq!(utun4.interface_type, InterfaceType::Vpn);
    assert_eq!(utun4.status, InterfaceStatus::Up);
    assert_eq!(utun4.ipv4[0].value, "10.10.0.2");
    assert_eq!(utun4.ipv6[0].value, "fe80::abcd%utun4");
    assert_eq!(utun4.stats, None);
}

#[test]
fn infers_interface_types_from_name_rules() {
    let wifi_like = parse_interfaces(include_str!("../fixtures/macos14.txt"));
    let vpn_like = parse_interfaces(include_str!("../fixtures/vpn.txt"));
    let mixed = parse_interfaces(include_str!("../fixtures/docker.txt"));

    assert_eq!(
        wifi_like.iter().find(|item| item.name == "en0").unwrap().interface_type,
        InterfaceType::WifiOrEthernet
    );
    assert_eq!(
        wifi_like.iter().find(|item| item.name == "lo0").unwrap().interface_type,
        InterfaceType::Loopback
    );
    assert_eq!(
        vpn_like.iter().find(|item| item.name == "utun4").unwrap().interface_type,
        InterfaceType::Vpn
    );
    assert_eq!(
        mixed.iter().find(|item| item.name == "bridge0").unwrap().interface_type,
        InterfaceType::Bridge
    );
    assert_eq!(
        mixed.iter().find(|item| item.name == "awdl0").unwrap().interface_type,
        InterfaceType::AirDrop
    );
    assert_eq!(
        mixed.iter().find(|item| item.name == "mystery0").unwrap().interface_type,
        InterfaceType::Unknown
    );
}

#[test]
fn merges_stats_into_interface_records() {
    let input = include_str!("../fixtures/macos15.txt");
    let interfaces = parse_interfaces(input);

    let merged = merge_stats(input, interfaces);
    let en0 = merged.iter().find(|item| item.name == "en0").unwrap();

    assert_eq!(
        en0.stats.as_ref(),
        Some(&lazyifconfig::model::InterfaceStats {
            rx_bytes: 4096,
            tx_bytes: 2048,
            rx_packets: 200,
            tx_packets: 100,
        })
    );
}

#[test]
fn test_network_classification_priority() {
    // utun은 사설 IP가 있어도 VPN으로 분류되어야 함
    let input = "utun4: flags=8051<UP,POINTOPOINT,RUNNING,MULTICAST> mtu 1500\n\tinet 10.8.0.2 netmask 0xffffff00";
    let parsed = parse_interfaces(input);
    assert_eq!(parsed[0].network_kind, lazyifconfig::model::NetworkKind::Vpn);

    // en0에 사설 IP가 있으면 LAN
    let input2 = "en0: flags=8863<UP,BROADCAST,SMART,RUNNING,SIMPLEX,MULTICAST> mtu 1500\n\tinet 192.168.0.15 netmask 0xffffff00";
    let parsed2 = parse_interfaces(input2);
    assert_eq!(parsed2[0].network_kind, lazyifconfig::model::NetworkKind::Lan);
}

#[test]
fn test_gateway_parsing() {
    let input = "utun4: flags=8051<UP,POINTOPOINT,RUNNING,MULTICAST> mtu 1380\n\tinet 10.10.0.2 --> 10.10.0.1 netmask 0xffffffff";
    let interfaces = parse_interfaces(input);
    assert_eq!(interfaces[0].ipv4[0].gateway, Some("10.10.0.1".to_string()));

    let netstat_input = "Routing tables\n\nInternet:\nDestination        Gateway            Flags               Netif\ndefault            192.168.0.1        UGScg                 en0";
    let en0 = lazyifconfig::model::NetworkInterface {
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
    let mut interfaces2 = vec![en0];
    lazyifconfig::collector::interface::merge_gateways(&mut interfaces2, netstat_input);
    assert_eq!(interfaces2[0].ipv4[0].gateway, Some("192.168.0.1".to_string()));
}
