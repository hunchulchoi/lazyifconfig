# Route Inspector Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Upgrade the existing Routes view into a visual Route Inspector with route summary, destination path lookup, VPN route detection, diagnostics, improved route table metadata, and raw output integration.

**Architecture:** Keep the current `ViewMode::Routes` entry point and expand it instead of adding a new top-level app mode. Route collection remains in `src/command.rs` and `src/main.rs`; parsing stays in `src/collector/routes.rs`; route-specific interpretation lives in new focused modules under `src/route_inspector/`; `src/app.rs` owns interactive state; `src/ui.rs` renders the inspector.

**Tech Stack:** Rust 2021, ratatui, crossterm, tokio, petgraph, existing `cargo test` suite.

---

## File Structure

- Modify `Cargo.toml`: add `petgraph` dependency.
- Modify `src/lib.rs`: export the new `route_inspector` module.
- Modify `src/model.rs`: extend `RouteEntry`; add route family, path result, graph, diagnostics, route sort, and command source variants.
- Modify `src/command.rs`: add OS-specific route path, IPv6 route, and Linux rule command specs.
- Modify `src/collector/routes.rs`: parse richer route table metadata plus route-get outputs.
- Create `src/route_inspector/mod.rs`: module exports.
- Create `src/route_inspector/vpn.rs`: VPN interface detection.
- Create `src/route_inspector/diagnostics.rs`: rule-based route diagnostics.
- Create `src/route_inspector/graph.rs`: graph builder and terminal topology line generation.
- Modify `src/app.rs`: add `RouteInspectorState`, filtering, section cycling, diagnostics refresh, and route navigation behavior.
- Modify `src/main.rs`: collect extra route raw outputs, execute path lookup from destination input, and wire route keys.
- Modify `src/ui.rs`: split Routes view rendering into a Route Inspector layout.
- Modify `tests/app_state.rs`: add state tests for route inspector filtering and section behavior.
- Add `tests/route_inspector.rs`: integration-style tests for parser, diagnostics, VPN detection, graph output, and commands.

Keep the existing unstaged `src/collector/ports.rs` change out of every Route Inspector commit unless the user explicitly asks to include it.

---

### Task 1: Model And Command Foundations

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/lib.rs`
- Modify: `src/model.rs`
- Modify: `src/command.rs`
- Test: `src/command.rs`

- [ ] **Step 1: Add failing command spec tests**

Append these tests to the existing `#[cfg(test)] mod tests` in `src/command.rs`.

```rust
#[test]
fn route_path_command_uses_ip_route_get_on_linux() {
    let command = route_path_command_spec_for_os("linux", "8.8.8.8");

    assert_eq!(command.display, "ip route get 8.8.8.8");
    assert_eq!(command.program, "ip");
    assert_eq!(command.args, vec!["route", "get", "8.8.8.8"]);
}

#[test]
fn route_path_command_uses_route_get_on_non_linux() {
    let command = route_path_command_spec_for_os("macos", "8.8.8.8");

    assert_eq!(command.display, "route -n get 8.8.8.8");
    assert_eq!(command.program, "route");
    assert_eq!(command.args, vec!["-n", "get", "8.8.8.8"]);
}

#[test]
fn linux_route_support_commands_are_available() {
    let ipv6 = ipv6_route_table_command_spec_for_os("linux");
    let rules = ip_rule_command_spec_for_os("linux");

    assert_eq!(ipv6.display, "ip -6 route show");
    assert_eq!(ipv6.program, "ip");
    assert_eq!(ipv6.args, vec!["-6", "route", "show"]);

    assert_eq!(rules.display, "ip rule");
    assert_eq!(rules.program, "ip");
    assert_eq!(rules.args, vec!["rule"]);
}

#[test]
fn non_linux_route_support_commands_are_not_required() {
    assert_eq!(ipv6_route_table_command_spec_for_os("macos"), None);
    assert_eq!(ip_rule_command_spec_for_os("macos"), None);
}
```

- [ ] **Step 2: Run the tests and verify they fail**

Run:

```bash
cargo test command::tests
```

Expected: FAIL because `route_path_command_spec_for_os`, `ipv6_route_table_command_spec_for_os`, and `ip_rule_command_spec_for_os` do not exist.

- [ ] **Step 3: Add petgraph dependency**

In `Cargo.toml`, add this dependency under `[dependencies]`.

```toml
petgraph = "0.6.5"
```

- [ ] **Step 4: Export the route inspector module**

In `src/lib.rs`, add:

```rust
pub mod route_inspector;
```

- [ ] **Step 5: Extend route models and command sources**

In `src/model.rs`, replace the current `RouteEntry` with this expanded model and add the related types near it.

```rust
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RouteFamily {
    Ipv4,
    Ipv6,
    #[default]
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteEntry {
    pub destination: String,
    pub gateway: String,
    pub interface: String,
    pub metric: Option<u32>,
    pub protocol: Option<String>,
    pub flags: Option<String>,
    pub family: RouteFamily,
}

impl RouteEntry {
    pub fn new(destination: impl Into<String>, gateway: impl Into<String>, interface: impl Into<String>) -> Self {
        Self {
            destination: destination.into(),
            gateway: gateway.into(),
            interface: interface.into(),
            metric: None,
            protocol: None,
            flags: None,
            family: RouteFamily::Unknown,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RoutePathResult {
    pub destination: String,
    pub resolved_destination: Option<String>,
    pub source_ip: Option<String>,
    pub interface: Option<String>,
    pub gateway: Option<String>,
    pub is_vpn: bool,
    pub raw_output: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteGraphNodeKind {
    Host,
    Interface,
    Gateway,
    VpnTunnel,
    Internet,
    Destination,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteGraphNode {
    pub kind: RouteGraphNodeKind,
    pub label: String,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RouteGraph {
    pub nodes: Vec<RouteGraphNode>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteDiagnosticSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteDiagnostic {
    pub severity: RouteDiagnosticSeverity,
    pub title: String,
    pub description: String,
    pub affected_route: Option<RouteEntry>,
    pub recommendation: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RouteInspectorSection {
    #[default]
    Summary,
    PathViewer,
    RouteTable,
    VpnRoutes,
    Diagnostics,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RouteSortColumn {
    #[default]
    Destination,
    Gateway,
    Interface,
    Metric,
}
```

Add these variants to `CommandSourceId`.

```rust
Ipv6Routes,
IpRules,
RoutePath,
```

Add these arms to `CommandSourceId::as_str`.

```rust
CommandSourceId::Ipv6Routes => "ip -6 route show",
CommandSourceId::IpRules => "ip rule",
CommandSourceId::RoutePath => "route path lookup",
```

- [ ] **Step 6: Add owned command specs**

In `src/command.rs`, add an owned command spec because route path arguments include user input.

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedCommandSpec {
    pub display: String,
    pub program: String,
    pub args: Vec<String>,
}

pub fn route_path_command_spec(destination: &str) -> OwnedCommandSpec {
    route_path_command_spec_for_os(std::env::consts::OS, destination)
}

pub fn ipv6_route_table_command_spec() -> Option<OwnedCommandSpec> {
    ipv6_route_table_command_spec_for_os(std::env::consts::OS)
}

pub fn ip_rule_command_spec() -> Option<OwnedCommandSpec> {
    ip_rule_command_spec_for_os(std::env::consts::OS)
}

pub fn route_path_command_spec_for_os(os: &str, destination: &str) -> OwnedCommandSpec {
    if os == "linux" {
        OwnedCommandSpec {
            display: format!("ip route get {destination}"),
            program: "ip".to_string(),
            args: vec!["route".to_string(), "get".to_string(), destination.to_string()],
        }
    } else {
        OwnedCommandSpec {
            display: format!("route -n get {destination}"),
            program: "route".to_string(),
            args: vec!["-n".to_string(), "get".to_string(), destination.to_string()],
        }
    }
}

pub fn ipv6_route_table_command_spec_for_os(os: &str) -> Option<OwnedCommandSpec> {
    (os == "linux").then(|| OwnedCommandSpec {
        display: "ip -6 route show".to_string(),
        program: "ip".to_string(),
        args: vec!["-6".to_string(), "route".to_string(), "show".to_string()],
    })
}

pub fn ip_rule_command_spec_for_os(os: &str) -> Option<OwnedCommandSpec> {
    (os == "linux").then(|| OwnedCommandSpec {
        display: "ip rule".to_string(),
        program: "ip".to_string(),
        args: vec!["rule".to_string()],
    })
}

pub fn run_owned_command_capture(command: &OwnedCommandSpec) -> Result<CommandResult, String> {
    let args: Vec<&str> = command.args.iter().map(String::as_str).collect();
    run_command_capture(command.program.as_str(), &args)
}
```

- [ ] **Step 7: Update existing route construction**

Update all direct `RouteEntry { destination, gateway, interface }` literals in tests and production code to include the new fields, or replace simple cases with `RouteEntry::new(destination, gateway, interface)`.

- [ ] **Step 8: Run tests**

Run:

```bash
cargo test command::tests
```

Expected: PASS.

- [ ] **Step 9: Commit**

```bash
git add Cargo.toml Cargo.lock src/lib.rs src/model.rs src/command.rs
git commit -m "feat: add route inspector model foundations"
```

---

### Task 2: Route Table And Path Parsers

**Files:**
- Modify: `src/collector/routes.rs`
- Test: `src/collector/routes.rs`
- Add: `tests/route_inspector.rs`

- [ ] **Step 1: Add parser tests**

Create `tests/route_inspector.rs` with these tests.

```rust
use lazyifconfig::collector::routes::{
    parse_linux_route_path,
    parse_macos_route_path,
    parse_routes,
};
use lazyifconfig::model::RouteFamily;

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
```

- [ ] **Step 2: Run parser tests and verify they fail**

Run:

```bash
cargo test --test route_inspector
```

Expected: FAIL because route path parser functions and metadata parsing are not implemented.

- [ ] **Step 3: Implement richer parsers**

In `src/collector/routes.rs`, keep the public `parse_routes` API and add these public route path parsers.

```rust
use crate::model::{RouteEntry, RouteFamily, RoutePathResult};

pub fn parse_linux_route_path(destination: &str, output: &str) -> Result<RoutePathResult, String> {
    let first_line = output.lines().next().unwrap_or("").trim();
    if first_line.is_empty() {
        return Err("route path output is empty".to_string());
    }

    let parts: Vec<&str> = first_line.split_whitespace().collect();
    let resolved_destination = parts.first().map(|value| (*value).to_string());
    let gateway = value_after(&parts, "via").map(str::to_string);
    let interface = value_after(&parts, "dev").map(str::to_string);
    let source_ip = value_after(&parts, "src").map(str::to_string);

    Ok(RoutePathResult {
        destination: destination.to_string(),
        resolved_destination,
        source_ip,
        interface,
        gateway,
        is_vpn: false,
        raw_output: output.to_string(),
    })
}

pub fn parse_macos_route_path(destination: &str, output: &str) -> Result<RoutePathResult, String> {
    if output.trim().is_empty() {
        return Err("route path output is empty".to_string());
    }

    let mut resolved_destination = None;
    let mut gateway = None;
    let mut interface = None;

    for line in output.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("route to:") {
            resolved_destination = Some(value.trim().to_string());
        } else if let Some(value) = trimmed.strip_prefix("gateway:") {
            gateway = Some(value.trim().to_string());
        } else if let Some(value) = trimmed.strip_prefix("interface:") {
            interface = Some(value.trim().to_string());
        }
    }

    Ok(RoutePathResult {
        destination: destination.to_string(),
        resolved_destination,
        source_ip: None,
        interface,
        gateway,
        is_vpn: false,
        raw_output: output.to_string(),
    })
}
```

Update the macOS and Linux table parsing functions so each emitted route uses the extended fields.

```rust
RouteEntry {
    destination: destination.to_string(),
    gateway: gateway.to_string(),
    interface: interface.to_string(),
    metric,
    protocol,
    flags,
    family,
}
```

For Linux parsing:

```rust
let metric = value_after(&parts, "metric").and_then(|value| value.parse::<u32>().ok());
let protocol = value_after(&parts, "proto").map(str::to_string);
let family = if destination.contains(':') || gateway.contains(':') {
    RouteFamily::Ipv6
} else {
    RouteFamily::Ipv4
};
```

For macOS parsing:

```rust
let flags = parts.get(2).map(|value| (*value).to_string());
let family = if parsing_ipv6 { RouteFamily::Ipv6 } else { RouteFamily::Ipv4 };
```

- [ ] **Step 4: Run parser tests**

Run:

```bash
cargo test --test route_inspector
```

Expected: PASS.

- [ ] **Step 5: Run existing route parser unit tests**

Run:

```bash
cargo test collector::routes::tests
```

Expected: PASS after updating assertions to account for extended route fields.

- [ ] **Step 6: Commit**

```bash
git add src/collector/routes.rs tests/route_inspector.rs
git commit -m "feat: parse route inspector data"
```

---

### Task 3: VPN Detection, Diagnostics, And Graph Builder

**Files:**
- Create: `src/route_inspector/mod.rs`
- Create: `src/route_inspector/vpn.rs`
- Create: `src/route_inspector/diagnostics.rs`
- Create: `src/route_inspector/graph.rs`
- Modify: `tests/route_inspector.rs`

- [ ] **Step 1: Add failing tests**

Append these tests to `tests/route_inspector.rs`.

```rust
use lazyifconfig::model::{
    InterfaceAddress, InterfaceStatus, InterfaceType, NetworkInterface, NetworkKind,
    RouteDiagnosticSeverity, RouteEntry, RouteFamily, RoutePathResult,
};
use lazyifconfig::route_inspector::diagnostics::build_route_diagnostics;
use lazyifconfig::route_inspector::graph::{build_route_graph, render_route_graph_lines};
use lazyifconfig::route_inspector::vpn::is_vpn_interface_name;

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
    assert!(no_default.iter().any(|item| item.title == "No default route"));

    let routes = vec![
        route("default", "192.168.0.1", "en0", None),
        route("0.0.0.0/0", "10.8.0.1", "utun4", Some(100)),
    ];
    let diagnostics = build_route_diagnostics(&routes, &[]);

    assert!(diagnostics.iter().any(|item| item.title == "Multiple default routes"));
}

#[test]
fn diagnostics_find_down_and_missing_interfaces() {
    let routes = vec![
        route("default", "192.168.0.1", "en0", None),
        route("10.8.0.0/24", "link", "utun4", None),
    ];
    let interfaces = vec![interface("en0", InterfaceStatus::Down)];

    let diagnostics = build_route_diagnostics(&routes, &interfaces);

    assert!(diagnostics.iter().any(|item| item.title == "Route interface is down"));
    assert!(diagnostics.iter().any(|item| item.title == "Route references missing interface"));
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
```

- [ ] **Step 2: Run tests and verify they fail**

Run:

```bash
cargo test --test route_inspector
```

Expected: FAIL because `route_inspector` modules do not exist.

- [ ] **Step 3: Create module exports**

Create `src/route_inspector/mod.rs`.

```rust
pub mod diagnostics;
pub mod graph;
pub mod vpn;
```

- [ ] **Step 4: Implement VPN detector**

Create `src/route_inspector/vpn.rs`.

```rust
pub fn is_vpn_interface_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with("tun")
        || lower.starts_with("tap")
        || lower.starts_with("utun")
        || lower.starts_with("wg")
        || lower.starts_with("tailscale")
        || lower.starts_with("zt")
}
```

- [ ] **Step 5: Implement diagnostics**

Create `src/route_inspector/diagnostics.rs`.

```rust
use std::collections::{HashMap, HashSet};

use crate::model::{
    InterfaceStatus, NetworkInterface, RouteDiagnostic, RouteDiagnosticSeverity, RouteEntry,
};
use crate::route_inspector::vpn::is_vpn_interface_name;

pub fn build_route_diagnostics(
    routes: &[RouteEntry],
    interfaces: &[NetworkInterface],
) -> Vec<RouteDiagnostic> {
    let mut diagnostics = Vec::new();
    let default_routes: Vec<&RouteEntry> = routes.iter().filter(|route| is_default_route(route)).collect();

    if default_routes.is_empty() {
        diagnostics.push(RouteDiagnostic {
            severity: RouteDiagnosticSeverity::Warning,
            title: "No default route".to_string(),
            description: "No IPv4 default route was found.".to_string(),
            affected_route: None,
            recommendation: "Check whether the active network interface has a gateway assigned.".to_string(),
        });
    } else if default_routes.len() > 1 {
        diagnostics.push(RouteDiagnostic {
            severity: RouteDiagnosticSeverity::Warning,
            title: "Multiple default routes".to_string(),
            description: format!("Found {} IPv4 default routes.", default_routes.len()),
            affected_route: default_routes.first().map(|route| (*route).clone()),
            recommendation: "Verify route priorities and metrics so the intended default route wins.".to_string(),
        });
    }

    if let Some(route) = default_routes.iter().find(|route| is_vpn_interface_name(&route.interface)) {
        diagnostics.push(RouteDiagnostic {
            severity: RouteDiagnosticSeverity::Info,
            title: "VPN overrides default route".to_string(),
            description: format!("Default traffic currently uses {}.", route.interface),
            affected_route: Some((*route).clone()),
            recommendation: "If this is unexpected, disconnect the VPN or inspect split-tunnel settings.".to_string(),
        });
    }

    let interfaces_by_name: HashMap<&str, &NetworkInterface> =
        interfaces.iter().map(|interface| (interface.name.as_str(), interface)).collect();
    let mut reported_missing = HashSet::new();

    for route in routes {
        match interfaces_by_name.get(route.interface.as_str()) {
            Some(interface) if interface.status == InterfaceStatus::Down => diagnostics.push(RouteDiagnostic {
                severity: RouteDiagnosticSeverity::Warning,
                title: "Route interface is down".to_string(),
                description: format!("Route {} references down interface {}.", route.destination, route.interface),
                affected_route: Some(route.clone()),
                recommendation: "Bring the interface up or remove the stale route.".to_string(),
            }),
            Some(_) => {}
            None if reported_missing.insert(route.interface.clone()) => diagnostics.push(RouteDiagnostic {
                severity: RouteDiagnosticSeverity::Warning,
                title: "Route references missing interface".to_string(),
                description: format!("A route references interface {}, which is not in the current interface snapshot.", route.interface),
                affected_route: Some(route.clone()),
                recommendation: "Refresh network state and check whether the route is stale.".to_string(),
            }),
            None => {}
        }
    }

    let mut metrics_by_destination: HashMap<(&str, Option<u32>), usize> = HashMap::new();
    for route in routes {
        *metrics_by_destination.entry((route.destination.as_str(), route.metric)).or_default() += 1;
    }
    for ((destination, metric), count) in metrics_by_destination {
        if count > 1 && metric.is_some() {
            diagnostics.push(RouteDiagnostic {
                severity: RouteDiagnosticSeverity::Warning,
                title: "Route metric conflict".to_string(),
                description: format!("{count} routes for {destination} share metric {}.", metric.unwrap()),
                affected_route: routes.iter().find(|route| route.destination == destination && route.metric == metric).cloned(),
                recommendation: "Adjust metrics so route priority is unambiguous.".to_string(),
            });
        }
    }

    diagnostics
}

pub fn is_default_route(route: &RouteEntry) -> bool {
    matches!(route.destination.as_str(), "default" | "0.0.0.0/0")
}
```

- [ ] **Step 6: Implement graph builder and renderer**

Create `src/route_inspector/graph.rs`.

```rust
use petgraph::graph::Graph;

use crate::model::{
    RouteGraph, RouteGraphNode, RouteGraphNodeKind, RoutePathResult,
};
use crate::route_inspector::vpn::is_vpn_interface_name;

pub fn build_route_graph(result: &RoutePathResult) -> RouteGraph {
    let mut internal = Graph::<RouteGraphNode, ()>::new();
    let mut nodes = Vec::new();

    let host = RouteGraphNode {
        kind: RouteGraphNodeKind::Host,
        label: "This Host".to_string(),
        detail: result.source_ip.clone(),
    };
    internal.add_node(host.clone());
    nodes.push(host);

    if let Some(interface) = &result.interface {
        let node = RouteGraphNode {
            kind: RouteGraphNodeKind::Interface,
            label: "Interface".to_string(),
            detail: Some(interface.clone()),
        };
        internal.add_node(node.clone());
        nodes.push(node);
    }

    if let Some(gateway) = &result.gateway {
        if !is_link_gateway(gateway) {
            let node = RouteGraphNode {
                kind: RouteGraphNodeKind::Gateway,
                label: "Gateway".to_string(),
                detail: Some(gateway.clone()),
            };
            internal.add_node(node.clone());
            nodes.push(node);
        }
    }

    let uses_vpn = result.is_vpn
        || result
            .interface
            .as_deref()
            .is_some_and(is_vpn_interface_name);
    if uses_vpn {
        let node = RouteGraphNode {
            kind: RouteGraphNodeKind::VpnTunnel,
            label: "VPN Tunnel".to_string(),
            detail: result.interface.clone(),
        };
        internal.add_node(node.clone());
        nodes.push(node);
    } else {
        let node = RouteGraphNode {
            kind: RouteGraphNodeKind::Internet,
            label: "Internet".to_string(),
            detail: None,
        };
        internal.add_node(node.clone());
        nodes.push(node);
    }

    let node = RouteGraphNode {
        kind: RouteGraphNodeKind::Destination,
        label: "Destination".to_string(),
        detail: result.resolved_destination.clone().or_else(|| Some(result.destination.clone())),
    };
    internal.add_node(node.clone());
    nodes.push(node);

    RouteGraph { nodes }
}

pub fn render_route_graph_lines(graph: &RouteGraph) -> Vec<String> {
    let mut lines = Vec::new();
    for (index, node) in graph.nodes.iter().enumerate() {
        push_box(&mut lines, node);
        if index + 1 < graph.nodes.len() {
            lines.push("       │".to_string());
            lines.push("       ▼".to_string());
        }
    }
    lines
}

fn push_box(lines: &mut Vec<String>, node: &RouteGraphNode) {
    lines.push("┌──────────────┐".to_string());
    lines.push(format_box_line(&node.label));
    if let Some(detail) = &node.detail {
        lines.push(format_box_line(detail));
    }
    lines.push("└──────────────┘".to_string());
}

fn format_box_line(value: &str) -> String {
    let text: String = value.chars().take(12).collect();
    format!("│{text:<14}│")
}

fn is_link_gateway(gateway: &str) -> bool {
    gateway == "link" || gateway == "local" || gateway.starts_with("link#")
}
```

- [ ] **Step 7: Run tests**

Run:

```bash
cargo test --test route_inspector
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add src/route_inspector tests/route_inspector.rs
git commit -m "feat: add route inspector analysis"
```

---

### Task 4: App State For Route Inspector

**Files:**
- Modify: `src/app.rs`
- Modify: `tests/app_state.rs`

- [ ] **Step 1: Add failing app state tests**

Append these tests to `tests/app_state.rs`.

```rust
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
        lazyifconfig::app::NavigationItem::Route { interface, .. } => assert_eq!(interface, "utun4"),
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
```

- [ ] **Step 2: Run tests and verify they fail**

Run:

```bash
cargo test --test app_state route_
```

Expected: FAIL because `route_inspector` app state and section methods do not exist.

- [ ] **Step 3: Add route inspector state**

In `src/app.rs`, import the new model types.

```rust
use crate::model::{
    CommandOutput, CommandSourceId, NetworkEvent, NetworkInterface, NetworkSnapshot, PublicIpInfo,
    RouteDiagnostic, RouteInspectorSection, RoutePathResult, RouteSortColumn, Subnet,
};
```

Add this state struct near `RawViewerState`.

```rust
#[derive(Clone, Debug)]
pub struct RouteInspectorState {
    pub active_section: RouteInspectorSection,
    pub destination_input: String,
    pub destination_input_active: bool,
    pub latest_path_result: Option<RoutePathResult>,
    pub latest_path_error: Option<String>,
    pub diagnostics: Vec<RouteDiagnostic>,
    pub route_filter: String,
    pub route_filter_active: bool,
    pub sort_column: RouteSortColumn,
}

impl Default for RouteInspectorState {
    fn default() -> Self {
        Self {
            active_section: RouteInspectorSection::Summary,
            destination_input: "8.8.8.8".to_string(),
            destination_input_active: false,
            latest_path_result: None,
            latest_path_error: None,
            diagnostics: Vec::new(),
            route_filter: String::new(),
            route_filter_active: false,
            sort_column: RouteSortColumn::Destination,
        }
    }
}
```

Add this field to `App`.

```rust
pub route_inspector: RouteInspectorState,
```

Initialize it in `Default for App`.

```rust
route_inspector: RouteInspectorState::default(),
```

- [ ] **Step 4: Refresh diagnostics on snapshots**

In `replace_snapshot`, after `push_generated_events()` and before `update_navigation_items()`, add:

```rust
self.refresh_route_diagnostics();
```

Add this method to `impl App`.

```rust
pub fn refresh_route_diagnostics(&mut self) {
    let Some(snapshot) = &self.current_snapshot else {
        self.route_inspector.diagnostics.clear();
        return;
    };

    self.route_inspector.diagnostics =
        crate::route_inspector::diagnostics::build_route_diagnostics(
            &snapshot.routes,
            &snapshot.interfaces,
        );
}
```

- [ ] **Step 5: Add route section cycling**

Add these methods to `impl App`.

```rust
pub fn select_next_route_section(&mut self) {
    self.route_inspector.active_section = match self.route_inspector.active_section {
        RouteInspectorSection::Summary => RouteInspectorSection::PathViewer,
        RouteInspectorSection::PathViewer => RouteInspectorSection::RouteTable,
        RouteInspectorSection::RouteTable => RouteInspectorSection::VpnRoutes,
        RouteInspectorSection::VpnRoutes => RouteInspectorSection::Diagnostics,
        RouteInspectorSection::Diagnostics => RouteInspectorSection::Summary,
    };
    self.details_scroll = 0;
}

pub fn select_previous_route_section(&mut self) {
    self.route_inspector.active_section = match self.route_inspector.active_section {
        RouteInspectorSection::Summary => RouteInspectorSection::Diagnostics,
        RouteInspectorSection::PathViewer => RouteInspectorSection::Summary,
        RouteInspectorSection::RouteTable => RouteInspectorSection::PathViewer,
        RouteInspectorSection::VpnRoutes => RouteInspectorSection::RouteTable,
        RouteInspectorSection::Diagnostics => RouteInspectorSection::VpnRoutes,
    };
    self.details_scroll = 0;
}
```

- [ ] **Step 6: Filter route navigation items**

Update the `ViewMode::Routes` branch in `update_navigation_items`.

```rust
ViewMode::Routes => {
    let snapshot = snapshot.unwrap();
    let query = self.route_inspector.route_filter.to_lowercase();
    let mut routes: Vec<(usize, &crate::model::RouteEntry)> = snapshot
        .routes
        .iter()
        .enumerate()
        .filter(|(_, route)| {
            query.is_empty()
                || route.destination.to_lowercase().contains(&query)
                || route.gateway.to_lowercase().contains(&query)
                || route.interface.to_lowercase().contains(&query)
        })
        .collect();

    match self.route_inspector.sort_column {
        RouteSortColumn::Destination => routes.sort_by(|(_, a), (_, b)| a.destination.cmp(&b.destination)),
        RouteSortColumn::Gateway => routes.sort_by(|(_, a), (_, b)| a.gateway.cmp(&b.gateway)),
        RouteSortColumn::Interface => routes.sort_by(|(_, a), (_, b)| a.interface.cmp(&b.interface)),
        RouteSortColumn::Metric => routes.sort_by(|(_, a), (_, b)| a.metric.cmp(&b.metric)),
    }

    self.navigation_items = routes
        .into_iter()
        .map(|(idx, r)| NavigationItem::Route {
            destination: r.destination.clone(),
            gateway: r.gateway.clone(),
            interface: r.interface.clone(),
            index: idx,
        })
        .collect();
}
```

- [ ] **Step 7: Run app state tests**

Run:

```bash
cargo test --test app_state route_
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add src/app.rs tests/app_state.rs
git commit -m "feat: add route inspector state"
```

---

### Task 5: Route Path Lookup And Raw Output Capture

**Files:**
- Modify: `src/main.rs`
- Modify: `src/command.rs`
- Modify: `src/app.rs`

- [ ] **Step 1: Add a reusable owned capture helper**

In `src/main.rs`, add this helper near `capture_command_output`.

```rust
fn capture_owned_command_output(
    app: &mut App,
    source_id: CommandSourceId,
    command: &lazyifconfig::command::OwnedCommandSpec,
) -> Result<String, String> {
    let args: Vec<&str> = command.args.iter().map(String::as_str).collect();
    let captured = run_command_capture(command.program.as_str(), &args)?;
    let result = command_stdout(&captured);
    app.command_outputs.insert(source_id, CommandOutput {
        command: command.display.clone(),
        stdout: captured.stdout,
        stderr: captured.stderr,
        executed_at: std::time::SystemTime::now(),
        exit_code: captured.exit_code,
    });
    result
}
```

- [ ] **Step 2: Capture Linux route support raw outputs during tick**

In `tick_update`, after the default route command capture, add:

```rust
if let Some(command) = lazyifconfig::command::ipv6_route_table_command_spec() {
    let _ = capture_owned_command_output(app, CommandSourceId::Ipv6Routes, &command);
}

if let Some(command) = lazyifconfig::command::ip_rule_command_spec() {
    let _ = capture_owned_command_output(app, CommandSourceId::IpRules, &command);
}
```

- [ ] **Step 3: Add route path execution function**

In `src/main.rs`, add:

```rust
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
                        .is_some_and(lazyifconfig::route_inspector::vpn::is_vpn_interface_name);
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
            app.route_inspector.latest_path_error =
                Some(format!("destination could not be resolved by route command: {error}"));
        }
    }
}
```

Update imports at the top of `src/main.rs`.

```rust
use lazyifconfig::collector::routes::{parse_linux_route_path, parse_macos_route_path, parse_routes};
```

- [ ] **Step 4: Wire route input keys**

Before the port filter input branch in the event loop, add a route destination input branch.

```rust
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
```

In normal mode, add these route-specific keys.

```rust
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
        app.route_inspector.active_section = lazyifconfig::model::RouteInspectorSection::PathViewer;
    }
}
```

Update `/` handling so Routes activates `route_filter_active` and Ports still activates `port_filter_active`.

```rust
KeyCode::Char('/') => {
    app.help_visible = false;
    match app.view_mode {
        ViewMode::Ports => app.port_filter_active = true,
        ViewMode::Routes => app.route_inspector.route_filter_active = true,
        _ => {}
    }
}
```

Add a route filter input branch similar to the port filter branch.

```rust
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
```

- [ ] **Step 5: Include route raw sources**

Update the `ViewMode::Routes` raw source list.

```rust
ViewMode::Routes => {
    let mut sources = vec![CommandSourceId::NetstatRoutes, CommandSourceId::DefaultRoute];
    if app.command_outputs.contains_key(&CommandSourceId::Ipv6Routes) {
        sources.push(CommandSourceId::Ipv6Routes);
    }
    if app.command_outputs.contains_key(&CommandSourceId::IpRules) {
        sources.push(CommandSourceId::IpRules);
    }
    if app.command_outputs.contains_key(&CommandSourceId::RoutePath) {
        sources.push(CommandSourceId::RoutePath);
    }
    if app.command_outputs.contains_key(&CommandSourceId::PublicIp) {
        sources.push(CommandSourceId::PublicIp);
    }
    sources
}
```

- [ ] **Step 6: Run focused tests**

Run:

```bash
cargo test command::tests
cargo test --test app_state route_
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/main.rs src/command.rs src/app.rs
git commit -m "feat: wire route path lookup"
```

---

### Task 6: Route Inspector UI

**Files:**
- Modify: `src/ui.rs`
- Modify: `tests/route_inspector.rs`

- [ ] **Step 1: Add graph rendering width test**

Append this test to `tests/route_inspector.rs`.

```rust
#[test]
fn graph_lines_stay_within_compact_width() {
    let result = RoutePathResult {
        destination: "github.com".to_string(),
        resolved_destination: Some("140.82.112.4".to_string()),
        source_ip: Some("192.168.0.25".to_string()),
        interface: Some("en0".to_string()),
        gateway: Some("192.168.0.1".to_string()),
        is_vpn: false,
        raw_output: String::new(),
    };

    let graph = build_route_graph(&result);
    let lines = render_route_graph_lines(&graph);

    assert!(lines.iter().all(|line| line.chars().count() <= 16));
}
```

- [ ] **Step 2: Run test**

Run:

```bash
cargo test --test route_inspector graph_lines_stay_within_compact_width
```

Expected: PASS if Task 3 used the compact graph renderer.

- [ ] **Step 3: Add route UI helpers**

In `src/ui.rs`, add imports.

```rust
use crate::model::{InterfaceStatus, NetworkKind, RouteDiagnosticSeverity, RouteFamily, RouteInspectorSection};
use crate::route_inspector::diagnostics::is_default_route;
use crate::route_inspector::graph::{build_route_graph, render_route_graph_lines};
use crate::route_inspector::vpn::is_vpn_interface_name;
```

Add helper functions near existing private UI helpers.

```rust
fn route_family_label(family: RouteFamily) -> &'static str {
    match family {
        RouteFamily::Ipv4 => "IPv4",
        RouteFamily::Ipv6 => "IPv6",
        RouteFamily::Unknown => "?",
    }
}

fn diagnostic_color(severity: RouteDiagnosticSeverity) -> Color {
    match severity {
        RouteDiagnosticSeverity::Info => Color::Blue,
        RouteDiagnosticSeverity::Warning => Color::Yellow,
        RouteDiagnosticSeverity::Error => Color::Red,
    }
}
```

- [ ] **Step 4: Replace Route details branch with section-aware rendering**

Replace the `NavigationItem::Route { ... }` details rendering branch with logic that delegates to `render_route_inspector_details(app, details_inner)`.

Add:

```rust
fn render_route_inspector_details(frame: &mut Frame, app: &App, area: Rect) {
    let lines = match app.route_inspector.active_section {
        RouteInspectorSection::Summary => route_summary_lines(app),
        RouteInspectorSection::PathViewer => route_path_lines(app),
        RouteInspectorSection::RouteTable => route_table_detail_lines(app),
        RouteInspectorSection::VpnRoutes => vpn_route_lines(app),
        RouteInspectorSection::Diagnostics => route_diagnostic_lines(app),
    };

    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .scroll((app.details_scroll, 0));
    frame.render_widget(paragraph, area);
}
```

Add these line builders.

```rust
fn route_summary_lines(app: &App) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        "=== Route Summary ===",
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    if let Some(snapshot) = &app.current_snapshot {
        let default_route = snapshot.routes.iter().find(|route| is_default_route(route));
        let ipv4_count = snapshot.routes.iter().filter(|route| route.family == RouteFamily::Ipv4).count();
        let ipv6_count = snapshot.routes.iter().filter(|route| route.family == RouteFamily::Ipv6).count();
        let vpn_routes: Vec<_> = snapshot
            .routes
            .iter()
            .filter(|route| is_vpn_interface_name(&route.interface))
            .collect();
        let warning_count = app.route_inspector.diagnostics.iter().filter(|item| item.severity == RouteDiagnosticSeverity::Warning).count();

        if let Some(route) = default_route {
            lines.push(Line::from(vec![Span::styled("Gateway:   ", Style::default().add_modifier(Modifier::BOLD)), Span::styled(route.gateway.clone(), Style::default().fg(Color::Blue))]));
            lines.push(Line::from(vec![Span::styled("Interface: ", Style::default().add_modifier(Modifier::BOLD)), Span::styled(route.interface.clone(), Style::default().fg(Color::Green))]));
        } else {
            lines.push(Line::from(Span::styled("No default route", Style::default().fg(Color::Red))));
        }
        lines.push(Line::from(format!("IPv4 Routes: {}", ipv4_count)));
        lines.push(Line::from(format!("IPv6 Routes: {}", ipv6_count)));
        lines.push(Line::from(format!("VPN: {}", if vpn_routes.is_empty() { "Disconnected" } else { "Connected" })));
        if let Some(route) = vpn_routes.first() {
            lines.push(Line::from(format!("VPN Interface: {}", route.interface)));
        }
        lines.push(Line::from(format!("Warnings: {}", warning_count)));
    }

    lines
}

fn route_path_lines(app: &App) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        "=== Path Viewer ===",
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(format!(
        "Destination: {}{}",
        app.route_inspector.destination_input,
        if app.route_inspector.destination_input_active { "█" } else { "" }
    )));
    lines.push(Line::from(""));

    if let Some(result) = &app.route_inspector.latest_path_result {
        let graph = build_route_graph(result);
        for line in render_route_graph_lines(&graph) {
            lines.push(Line::from(line));
        }
    } else if let Some(error) = &app.route_inspector.latest_path_error {
        lines.push(Line::from(Span::styled(error.clone(), Style::default().fg(Color::Red))));
    } else {
        lines.push(Line::from("Press Enter in Routes view, type a destination, then press Enter again."));
    }

    lines
}
```

Add `route_table_detail_lines`, `vpn_route_lines`, and `route_diagnostic_lines` using the same pattern:

```rust
fn route_table_detail_lines(app: &App) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled("=== Route Table ===", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))));
    lines.push(Line::from("Destination        Gateway          Interface  Metric  Proto  Flags"));
    if let Some(snapshot) = &app.current_snapshot {
        for route in &snapshot.routes {
            let metric = route.metric.map(|value| value.to_string()).unwrap_or_else(|| "-".to_string());
            let protocol = route.protocol.clone().unwrap_or_else(|| "-".to_string());
            let flags = route.flags.clone().unwrap_or_else(|| "-".to_string());
            let style = if is_default_route(route) {
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
            } else if is_vpn_interface_name(&route.interface) {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            };
            lines.push(Line::from(Span::styled(
                format!("{:<18} {:<16} {:<10} {:<7} {:<6} {}", route.destination, route.gateway, route.interface, metric, protocol, flags),
                style,
            )));
        }
    }
    lines
}

fn vpn_route_lines(app: &App) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled("=== VPN Routes ===", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))));
    if let Some(snapshot) = &app.current_snapshot {
        let vpn_routes: Vec<_> = snapshot.routes.iter().filter(|route| is_vpn_interface_name(&route.interface)).collect();
        if vpn_routes.is_empty() {
            lines.push(Line::from("No VPN routes detected."));
        } else {
            for route in vpn_routes {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(route.destination.clone(), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))));
                lines.push(Line::from(format!("Interface: {}", route.interface)));
                lines.push(Line::from(format!("Gateway: {}", route.gateway)));
            }
        }
    }
    lines
}

fn route_diagnostic_lines(app: &App) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled("=== Diagnostics ===", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))));
    if app.route_inspector.diagnostics.is_empty() {
        lines.push(Line::from(Span::styled("No routing warnings detected.", Style::default().fg(Color::Green))));
    } else {
        for item in &app.route_inspector.diagnostics {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(item.title.clone(), Style::default().fg(diagnostic_color(item.severity)).add_modifier(Modifier::BOLD))));
            lines.push(Line::from(item.description.clone()));
            if let Some(route) = &item.affected_route {
                lines.push(Line::from(format!("Affected: {} -> {} ({})", route.destination, route.gateway, route.interface)));
            }
            lines.push(Line::from(format!("Recommendation: {}", item.recommendation)));
        }
    }
    lines
}
```

- [ ] **Step 5: Improve route list rows**

In the `NavigationItem::Route` list item branch, include gateway and highlight route kinds.

```rust
NavigationItem::Route { destination, gateway, interface, .. } => {
    let text = format!("{:<18} {:<16} {}", destination, gateway, interface);
    let mut route_style = style;
    if destination == "default" || destination == "0.0.0.0/0" {
        route_style = route_style.fg(Color::Green).add_modifier(Modifier::BOLD);
    } else if is_vpn_interface_name(interface) {
        route_style = route_style.fg(Color::Yellow);
    }
    list_items.push(ListItem::new(text).style(route_style));
}
```

- [ ] **Step 6: Update footer text**

Update `get_status_text` for `ViewMode::Routes`.

```rust
ViewMode::Routes => {
    if app.route_inspector.route_filter_active {
        " filter routes: type | Enter apply | Esc clear | Backspace delete ".to_string()
    } else if app.route_inspector.destination_input_active {
        " destination: type | Enter lookup | Esc cancel | Backspace delete ".to_string()
    } else {
        " q | u check | U update | R notes | Enter path | Tab section | / filter | o raw | i/n/c/p/e ".to_string()
    }
}
```

- [ ] **Step 7: Run UI-adjacent tests**

Run:

```bash
cargo test --test route_inspector
cargo test --test app_state route_
cargo test ui::tests
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add src/ui.rs tests/route_inspector.rs
git commit -m "feat: render route inspector"
```

---

### Task 7: Full Verification And Documentation

**Files:**
- Modify: `README.md`
- Modify: `docs/superpowers/specs/2026-06-11-route-inspector-design.md`

- [ ] **Step 1: Update README feature bullets**

In `README.md`, replace the route feature bullet with:

```markdown
- Route Inspector with default route summary, destination path lookup, VPN route detection, diagnostics, and raw route output
```

Update Controls with route-specific notes:

```markdown
- `g`: Route Inspector
- In Route Inspector: `Enter` starts destination path lookup, `Tab` switches inspector sections, `/` filters routes, `o` opens raw route output
```

- [ ] **Step 2: Mark spec as implementation planned**

In `docs/superpowers/specs/2026-06-11-route-inspector-design.md`, update the status line to:

```markdown
- **상태**: 구현 계획 완료
```

- [ ] **Step 3: Run formatter**

Run:

```bash
cargo fmt
```

Expected: command exits 0 and formats Rust files.

- [ ] **Step 4: Run full test suite**

Run:

```bash
cargo test
```

Expected: PASS.

- [ ] **Step 5: Run clippy if available**

Run:

```bash
cargo clippy --all-targets --all-features
```

Expected: PASS. If clippy is not installed, record the missing component in the final implementation summary and continue with `cargo test` as the required verification.

- [ ] **Step 6: Manual smoke test**

Run:

```bash
cargo run
```

Manual checks:

- Press `g` and confirm Route Inspector opens.
- Confirm Summary shows default gateway/interface and warning count.
- Press `Enter`, type `8.8.8.8`, press `Enter`, and confirm Path Viewer shows a topology.
- Press `Tab` repeatedly and confirm sections cycle.
- Press `/`, type an interface name, press `Enter`, and confirm route rows filter.
- Press `o` and confirm raw route sources are available.

- [ ] **Step 7: Commit**

```bash
git add README.md docs/superpowers/specs/2026-06-11-route-inspector-design.md
git commit -m "docs: document route inspector"
```

---

## Self-Review

- Spec coverage: Summary, Path Viewer, route table metadata, VPN route detection, diagnostics, raw output, macOS/Linux command support, small-terminal route graph constraints, and documentation are covered by Tasks 1-7.
- Out of scope items remain out of implementation: Windows support, DNS resolver integration, traceroute visualization, network discovery, live traffic overlays, path comparison, and route change history.
- Type consistency: `RouteEntry`, `RoutePathResult`, `RouteGraph`, `RouteDiagnostic`, `RouteInspectorState`, and `CommandSourceId` names are consistent across tasks.
- Execution safety: commits are scoped by task and explicitly exclude the pre-existing unstaged `src/collector/ports.rs` change.
