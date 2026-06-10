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
fn parses_linux_ip_addr_output() {
    let input = "\
1: lo: <LOOPBACK,UP,LOWER_UP> mtu 65536 qdisc noqueue state UNKNOWN group default qlen 1000
    link/loopback 00:00:00:00:00:00 brd 00:00:00:00:00:00
    inet 127.0.0.1/8 scope host lo
       valid_lft forever preferred_lft forever
    inet6 ::1/128 scope host noprefixroute
       valid_lft forever preferred_lft forever
2: eth0@if3: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 qdisc noqueue state UP group default
    link/ether 02:42:ac:11:00:02 brd ff:ff:ff:ff:ff:ff link-netnsid 0
    inet 172.17.0.2/16 brd 172.17.255.255 scope global eth0
       valid_lft forever preferred_lft forever
    inet6 fe80::42:acff:fe11:2/64 scope link
       valid_lft forever preferred_lft forever
";

    let interfaces = parse_interfaces(input);
    let lo = interfaces.iter().find(|item| item.name == "lo").unwrap();
    let eth0 = interfaces.iter().find(|item| item.name == "eth0").unwrap();

    assert_eq!(lo.interface_type, InterfaceType::Loopback);
    assert_eq!(lo.status, InterfaceStatus::Up);
    assert_eq!(lo.ipv4[0].value, "127.0.0.1");
    assert_eq!(lo.ipv4[0].prefix_len, Some(8));
    assert_eq!(lo.ipv6[0].value, "::1");
    assert_eq!(lo.ipv6[0].prefix_len, Some(128));

    assert_eq!(eth0.status, InterfaceStatus::Up);
    assert_eq!(eth0.mac_address.as_deref(), Some("02:42:ac:11:00:02"));
    assert_eq!(eth0.mtu, Some(1500));
    assert_eq!(eth0.ipv4[0].value, "172.17.0.2");
    assert_eq!(eth0.ipv4[0].prefix_len, Some(16));
    assert_eq!(eth0.ipv6[0].value, "fe80::42:acff:fe11:2");
    assert_eq!(eth0.ipv6[0].prefix_len, Some(64));
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

#[test]
fn merges_linux_ip_route_default_gateway() {
    let mut interfaces = vec![lazyifconfig::model::NetworkInterface {
        name: "eth0".to_string(),
        network_kind: lazyifconfig::model::NetworkKind::Lan,
        interface_type: lazyifconfig::model::InterfaceType::WifiOrEthernet,
        status: lazyifconfig::model::InterfaceStatus::Up,
        ipv4: vec![lazyifconfig::model::InterfaceAddress {
            value: "172.17.0.2".to_string(),
            prefix_len: Some(16),
            gateway: None,
        }],
        ipv6: vec![],
        mac_address: None,
        mtu: None,
        stats: None,
    }];

    lazyifconfig::collector::interface::merge_gateways(
        &mut interfaces,
        "default via 172.17.0.1 dev eth0 proto static",
    );

    assert_eq!(interfaces[0].ipv4[0].gateway, Some("172.17.0.1".to_string()));
}

#[test]
fn test_parse_connections() {
    let input = "
Active Internet connections (including servers)
Proto Recv-Q Send-Q  Local Address          Foreign Address        (state)
tcp4       0      0  192.168.0.193.56780    211.242.12.227.995     ESTABLISHED
tcp6       0      0  *.56642                *.*                    LISTEN
udp4       0      0  *.5353                 *.*
";
    let connections = lazyifconfig::collector::connections::parse_connections(input);
    assert_eq!(connections.len(), 3);

    let c0 = &connections[0];
    assert_eq!(c0.proto, "tcp4");
    assert_eq!(c0.local_ip, "192.168.0.193");
    assert_eq!(c0.local_port, "56780");
    assert_eq!(c0.foreign_ip, "211.242.12.227");
    assert_eq!(c0.foreign_port, "995");
    assert_eq!(c0.state, Some("ESTABLISHED".to_string()));

    let c1 = &connections[1];
    assert_eq!(c1.proto, "tcp6");
    assert_eq!(c1.local_ip, "*");
    assert_eq!(c1.local_port, "56642");
    assert_eq!(c1.state, Some("LISTEN".to_string()));

    let c2 = &connections[2];
    assert_eq!(c2.proto, "udp4");
    assert_eq!(c2.local_ip, "*");
    assert_eq!(c2.local_port, "5353");
    assert_eq!(c2.foreign_ip, "*");
    assert_eq!(c2.foreign_port, "*");
    assert_eq!(c2.state, None);
}

#[test]
fn test_merge_stats_from_netstat_ib() {
    let netstat_input = "\
Name       Mtu   Network       Address            Ipkts Ierrs     Ibytes    Opkts Oerrs     Obytes  Coll
lo0        16384 <Link#1>                       2457315     0 1256093331  2457315     0 1256093331     0
en0        1500  <Link#14>   32:f7:1c:75:c4:c5 11885040     0 9178588284  9544740     0 4905232533     0
gif0*      1280  <Link#2>                             0     0          0        0     0          0     0
";
    let en0 = lazyifconfig::model::NetworkInterface {
        name: "en0".to_string(),
        network_kind: lazyifconfig::model::NetworkKind::Lan,
        interface_type: lazyifconfig::model::InterfaceType::WifiOrEthernet,
        status: lazyifconfig::model::InterfaceStatus::Up,
        ipv4: vec![],
        ipv6: vec![],
        mac_address: None,
        mtu: None,
        stats: None,
    };
    let interfaces = vec![en0];
    let merged = merge_stats(netstat_input, interfaces);
    let en0_res = &merged[0];
    assert!(en0_res.stats.is_some());
    let stats = en0_res.stats.as_ref().unwrap();
    assert_eq!(stats.rx_packets, 11885040);
    assert_eq!(stats.rx_bytes, 9178588284);
    assert_eq!(stats.tx_packets, 9544740);
    assert_eq!(stats.tx_bytes, 4905232533);
}
