use lazyifconfig::collector::routes::{
    parse_linux_route_path, parse_macos_route_path, parse_routes,
};
use lazyifconfig::model::{
    InterfaceAddress, InterfaceStatus, InterfaceType, NetworkInterface, NetworkKind,
    RouteDiagnosticSeverity, RouteEntry, RouteFamily, RoutePathResult,
};
use lazyifconfig::route_inspector::diagnostics::build_route_diagnostics;
use lazyifconfig::route_inspector::graph::{build_route_graph, render_route_graph_lines};
use lazyifconfig::route_inspector::vpn::is_vpn_interface_name;

#[test]
fn parses_macos_ipv4_and_ipv6_routes_with_metadata() {
    let sample = "\
Routing tables

Internet:
Destination        Gateway            Flags               Netif Expire
default            192.168.0.1        UGScg                 en0
10.8.0.0/24        link#20            UCS                 utun4

Internet6:
Destination                             Gateway                         Flags               Netif Expire
default                                 fe80::1%en0                     UGcI                  en0
::1                                     ::1                             UHL                   lo0
";

    let routes = parse_routes(sample);

    assert_eq!(routes.len(), 4);
    assert_eq!(routes[0].destination, "default");
    assert_eq!(routes[0].gateway, "192.168.0.1");
    assert_eq!(routes[0].interface, "en0");
    assert_eq!(routes[0].flags.as_deref(), Some("UGScg"));
    assert_eq!(routes[0].family, RouteFamily::Ipv4);

    assert_eq!(routes[2].destination, "default");
    assert_eq!(routes[2].gateway, "fe80::1%en0");
    assert_eq!(routes[2].interface, "en0");
    assert_eq!(routes[2].family, RouteFamily::Ipv6);
}

#[test]
fn parses_linux_ipv4_routes_with_metric_and_protocol() {
    let sample = "\
default via 172.17.0.1 dev eth0 proto static metric 100
172.17.0.0/16 dev eth0 proto kernel scope link src 172.17.0.2
10.8.0.0/24 via 10.8.0.1 dev tun0 metric 50
";

    let routes = parse_routes(sample);

    assert_eq!(routes.len(), 3);
    assert_eq!(routes[0].destination, "default");
    assert_eq!(routes[0].gateway, "172.17.0.1");
    assert_eq!(routes[0].interface, "eth0");
    assert_eq!(routes[0].protocol.as_deref(), Some("static"));
    assert_eq!(routes[0].metric, Some(100));
    assert_eq!(routes[0].family, RouteFamily::Ipv4);

    assert_eq!(routes[2].destination, "10.8.0.0/24");
    assert_eq!(routes[2].gateway, "10.8.0.1");
    assert_eq!(routes[2].interface, "tun0");
    assert_eq!(routes[2].metric, Some(50));
}

#[test]
fn parses_linux_ipv6_routes() {
    let sample = "\
default via fe80::1 dev eth0 proto ra metric 1024
fe80::/64 dev eth0 proto kernel metric 256
";

    let routes = parse_routes(sample);

    assert_eq!(routes.len(), 2);
    assert_eq!(routes[0].family, RouteFamily::Ipv6);
    assert_eq!(routes[0].gateway, "fe80::1");
    assert_eq!(routes[0].metric, Some(1024));
}

#[test]
fn parses_linux_route_get_output() {
    let output = "8.8.8.8 via 172.17.0.1 dev eth0 src 172.17.0.2 uid 501\n    cache";

    let result = parse_linux_route_path("8.8.8.8", output).unwrap();

    assert_eq!(result.destination, "8.8.8.8");
    assert_eq!(result.resolved_destination.as_deref(), Some("8.8.8.8"));
    assert_eq!(result.gateway.as_deref(), Some("172.17.0.1"));
    assert_eq!(result.interface.as_deref(), Some("eth0"));
    assert_eq!(result.source_ip.as_deref(), Some("172.17.0.2"));
    assert_eq!(result.raw_output, output);
}

#[test]
fn rejects_malformed_linux_route_get_output() {
    assert!(parse_linux_route_path("x", "not a route").is_err());
}

#[test]
fn does_not_parse_linux_route_get_output_as_table_route() {
    let output = "8.8.8.8 via 172.17.0.1 dev eth0 src 172.17.0.2 uid 501\n    cache";

    let routes = parse_routes(output);

    assert!(routes.is_empty());
}

#[test]
fn parses_macos_route_get_output() {
    let output = "\
   route to: 8.8.8.8
destination: default
       mask: default
    gateway: 192.168.0.1
  interface: en0
      flags: <UP,GATEWAY,DONE,STATIC,PRCLONING>
 recvpipe  sendpipe  ssthresh  rtt,msec    rttvar  hopcount      mtu     expire
       0         0         0         0         0         0      1500         0
";

    let result = parse_macos_route_path("8.8.8.8", output).unwrap();

    assert_eq!(result.destination, "8.8.8.8");
    assert_eq!(result.resolved_destination.as_deref(), Some("8.8.8.8"));
    assert_eq!(result.gateway.as_deref(), Some("192.168.0.1"));
    assert_eq!(result.interface.as_deref(), Some("en0"));
    assert_eq!(result.raw_output, output);
}

#[test]
fn rejects_malformed_macos_route_get_output() {
    assert!(parse_macos_route_path("x", "not a route").is_err());
}

#[test]
fn detects_common_vpn_interface_names() {
    for name in ["tun0", "tap0", "utun4", "wg0", "tailscale0", "ztabc"] {
        assert!(is_vpn_interface_name(name), "{name} should be VPN");
    }

    for name in ["en0", "eth0", "lo0", "bridge0"] {
        assert!(!is_vpn_interface_name(name), "{name} should not be VPN");
    }
}

#[test]
fn diagnostics_find_missing_and_multiple_default_routes() {
    let no_default = build_route_diagnostics(&[], &[]);
    assert!(no_default
        .iter()
        .any(|item| item.title == "No default route"));

    let routes = vec![
        route("default", "192.168.0.1", "en0", None),
        route("0.0.0.0/0", "10.8.0.1", "utun4", Some(100)),
    ];
    let diagnostics = build_route_diagnostics(&routes, &[]);

    assert!(diagnostics
        .iter()
        .any(|item| item.title == "Multiple default routes"));
}

#[test]
fn diagnostics_find_down_and_missing_interfaces() {
    let routes = vec![
        route("default", "192.168.0.1", "en0", None),
        route("10.8.0.0/24", "link", "utun4", None),
    ];
    let interfaces = vec![interface("en0", InterfaceStatus::Down)];

    let diagnostics = build_route_diagnostics(&routes, &interfaces);

    assert!(diagnostics
        .iter()
        .any(|item| item.title == "Route interface is down"));
    assert!(diagnostics
        .iter()
        .any(|item| item.title == "Route references missing interface"));
}

#[test]
fn diagnostics_mark_vpn_default_as_info() {
    let routes = vec![route("default", "10.8.0.1", "utun4", None)];
    let diagnostics = build_route_diagnostics(&routes, &[]);

    let item = diagnostics
        .iter()
        .find(|item| item.title == "VPN overrides default route")
        .unwrap();
    assert_eq!(item.severity, RouteDiagnosticSeverity::Info);
}

#[test]
fn graph_renders_plain_and_vpn_paths() {
    let plain = RoutePathResult {
        destination: "8.8.8.8".to_string(),
        resolved_destination: Some("8.8.8.8".to_string()),
        source_ip: Some("192.168.0.25".to_string()),
        interface: Some("en0".to_string()),
        gateway: Some("192.168.0.1".to_string()),
        is_vpn: false,
        raw_output: String::new(),
    };
    let graph = build_route_graph(&plain);
    let lines = render_route_graph_lines(&graph);
    assert!(lines.iter().any(|line| line.contains("This Host")));
    assert!(lines.iter().any(|line| line.contains("Gateway")));
    assert!(lines.iter().any(|line| line.contains("8.8.8.8")));

    let vpn = RoutePathResult {
        interface: Some("utun4".to_string()),
        is_vpn: true,
        ..plain
    };
    let graph = build_route_graph(&vpn);
    let lines = render_route_graph_lines(&graph);
    assert!(lines.iter().any(|line| line.contains("VPN Tunnel")));
    assert!(lines.iter().any(|line| line.contains("utun4")));
}

fn route(destination: &str, gateway: &str, interface: &str, metric: Option<u32>) -> RouteEntry {
    RouteEntry {
        destination: destination.to_string(),
        gateway: gateway.to_string(),
        interface: interface.to_string(),
        metric,
        protocol: None,
        flags: None,
        family: RouteFamily::Ipv4,
    }
}

fn interface(name: &str, status: InterfaceStatus) -> NetworkInterface {
    NetworkInterface {
        name: name.to_string(),
        network_kind: NetworkKind::Unknown,
        interface_type: InterfaceType::Unknown,
        status,
        ipv4: vec![InterfaceAddress::new("192.168.0.25")],
        ipv6: vec![],
        mac_address: None,
        mtu: None,
        stats: None,
    }
}
