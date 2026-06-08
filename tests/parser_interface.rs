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
