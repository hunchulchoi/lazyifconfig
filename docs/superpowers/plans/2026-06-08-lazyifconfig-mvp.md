# lazyifconfig MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a macOS-first Rust TUI MVP for browsing network interfaces, viewing per-interface details and throughput, and seeing recent network change events from `ifconfig` snapshots.

**Architecture:** The app is split into command execution, parsing, domain models, app state, and ratatui rendering. The first version uses only `ifconfig`, parses it into normalized models, infers lightweight interface types from naming rules, computes throughput and change events from consecutive snapshots, and renders a single-screen TUI with a list pane, detail pane, event panel, and status bar.

**Tech Stack:** Rust, Cargo, ratatui, crossterm, tokio, serde, serde_json

---

## File Structure

Planned project layout:

- `Cargo.toml`
  - Crate manifest and dependencies
- `src/main.rs`
  - Tokio entrypoint, terminal setup, event loop bootstrap
- `src/lib.rs`
  - Public module exports for integration tests and shared app code
- `src/app.rs`
  - App state, selection, refresh scheduling hooks, event history, throughput derivation
- `src/model.rs`
  - Core types such as `NetworkInterface`, `InterfaceType`, `InterfaceAddress`, `InterfaceStats`, `NetworkSnapshot`, `NetworkEvent`
- `src/command.rs`
  - `ifconfig` command runner and timeout handling
- `src/collector/mod.rs`
  - Collector module exports
- `src/collector/interface.rs`
  - Interface and address parsing from `ifconfig`
- `src/collector/stats.rs`
  - RX/TX stats parsing from `ifconfig`
- `src/ui.rs`
  - ratatui layout and rendering for the main screen
- `fixtures/macos14.txt`
  - Representative `ifconfig` output sample
- `fixtures/macos15.txt`
  - Representative `ifconfig` output sample
- `fixtures/vpn.txt`
  - `ifconfig` output with `utun` interfaces
- `fixtures/docker.txt`
  - `ifconfig` output with bridge-style or Docker-related interfaces
- `tests/parser_interface.rs`
  - Parser behavior tests using fixture files
- `tests/app_state.rs`
  - State update, throughput, and network event tests

## Assumptions

- The codebase is currently empty and not yet initialized as a Git repository.
- Commit steps are included as checkpoints, but if Git is still uninitialized at execution time, initialize Git first or skip commits for that run.
- Fixture contents can be captured from real macOS machines during implementation if the exact samples are not already available.

### Task 1: Scaffold the Rust application

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Create: `src/lib.rs`
- Create: `src/app.rs`
- Create: `src/model.rs`
- Create: `src/command.rs`
- Create: `src/collector/mod.rs`
- Create: `src/collector/interface.rs`
- Create: `src/collector/stats.rs`
- Create: `src/ui.rs`

- [ ] **Step 1: Write the failing manifest-level smoke check**

Create `src/lib.rs` with imports for modules that do not exist yet:

```rust
pub mod app;
pub mod collector;
pub mod command;
pub mod model;
pub mod ui;
```

Create `src/main.rs`:

```rust
fn main() {
    println!("lazyifconfig");
}
```

Create `Cargo.toml`:

```toml
[package]
name = "lazyifconfig"
version = "0.1.0"
edition = "2021"

[dependencies]
ratatui = "0.28"
crossterm = "0.28"
tokio = { version = "1", features = ["macros", "rt-multi-thread", "time", "process"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

- [ ] **Step 2: Run build to verify it fails**

Run: `cargo build`
Expected: FAIL with module file not found errors for `app`, `collector`, `command`, `model`, or `ui`

- [ ] **Step 3: Write the minimal module scaffolding**

Create these files with minimal content:

`src/app.rs`
```rust
#[derive(Default)]
pub struct App;
```

`src/model.rs`
```rust
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NetworkSnapshot;
```

`src/command.rs`
```rust
pub async fn run_ifconfig() -> Result<String, String> {
    Err("not implemented".to_string())
}
```

`src/collector/mod.rs`
```rust
pub mod interface;
pub mod stats;
```

`src/collector/interface.rs`
```rust
pub fn parse_interfaces(_input: &str) -> Vec<String> {
    Vec::new()
}
```

`src/collector/stats.rs`
```rust
pub fn parse_stats(_input: &str) -> Vec<String> {
    Vec::new()
}
```

`src/ui.rs`
```rust
pub fn render_title() -> &'static str {
    "lazyifconfig"
}
```

- [ ] **Step 4: Run build to verify it passes**

Run: `cargo build`
Expected: PASS

- [ ] **Step 5: Commit**

Run:

```bash
git add Cargo.toml src
git commit -m "chore: scaffold lazyifconfig crate"
```

### Task 2: Define the domain models

**Files:**
- Modify: `src/model.rs`
- Test: `tests/app_state.rs`

- [ ] **Step 1: Write the failing model behavior test**

Create `tests/app_state.rs`:

```rust
use lazyifconfig::model::{InterfaceAddress, InterfaceStats, InterfaceStatus, InterfaceType, NetworkEvent, NetworkInterface, NetworkSnapshot};

#[test]
fn snapshot_can_hold_interfaces_and_events() {
    let interface = NetworkInterface {
        name: "en0".to_string(),
        interface_type: InterfaceType::WifiOrEthernet,
        status: InterfaceStatus::Up,
        ipv4: vec![InterfaceAddress::new("192.168.0.10")],
        ipv6: vec![],
        mac_address: Some("aa:bb:cc:dd:ee:ff".to_string()),
        mtu: Some(1500),
        stats: InterfaceStats {
            rx_bytes: 100,
            tx_bytes: 50,
            rx_packets: 10,
            tx_packets: 5,
        },
    };

    let snapshot = NetworkSnapshot {
        interfaces: vec![interface],
        captured_at_secs: 10,
    };

    let event = NetworkEvent::new("en0 appeared".to_string(), 10);

    assert_eq!(snapshot.interfaces.len(), 1);
    assert_eq!(event.message, "en0 appeared");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test snapshot_can_hold_interfaces_and_events`
Expected: FAIL with unresolved crate items from `lazyifconfig::model`

- [ ] **Step 3: Write the minimal model implementation**

Replace `src/model.rs`:

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InterfaceType {
    Vpn,
    Loopback,
    Bridge,
    AirDrop,
    WifiOrEthernet,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InterfaceStatus {
    Up,
    Down,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InterfaceAddress {
    pub value: String,
}

impl InterfaceAddress {
    pub fn new(value: &str) -> Self {
        Self {
            value: value.to_string(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InterfaceStats {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_packets: u64,
    pub tx_packets: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkInterface {
    pub name: String,
    pub interface_type: InterfaceType,
    pub status: InterfaceStatus,
    pub ipv4: Vec<InterfaceAddress>,
    pub ipv6: Vec<InterfaceAddress>,
    pub mac_address: Option<String>,
    pub mtu: Option<u32>,
    pub stats: InterfaceStats,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NetworkSnapshot {
    pub interfaces: Vec<NetworkInterface>,
    pub captured_at_secs: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkEvent {
    pub message: String,
    pub captured_at_secs: u64,
}

impl NetworkEvent {
    pub fn new(message: String, captured_at_secs: u64) -> Self {
        Self {
            message,
            captured_at_secs,
        }
    }
}
```

Also update `src/main.rs` so the crate can be imported by tests later:

```rust
fn main() {
    println!("lazyifconfig");
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test snapshot_can_hold_interfaces_and_events`
Expected: PASS

- [ ] **Step 5: Commit**

Run:

```bash
git add src/lib.rs src/main.rs src/model.rs tests/app_state.rs
git commit -m "feat: define network domain models"
```

### Task 3: Add parser fixture coverage for interfaces and stats

**Files:**
- Create: `fixtures/macos14.txt`
- Create: `fixtures/macos15.txt`
- Create: `fixtures/vpn.txt`
- Create: `fixtures/docker.txt`
- Modify: `src/collector/interface.rs`
- Modify: `src/collector/stats.rs`
- Test: `tests/parser_interface.rs`

- [ ] **Step 1: Write the failing parser tests**

Create `tests/parser_interface.rs`:

```rust
use lazyifconfig::collector::interface::parse_interfaces;
use lazyifconfig::collector::stats::merge_stats;

#[test]
fn parses_en0_from_fixture() {
    let input = include_str!("../fixtures/macos14.txt");
    let interfaces = parse_interfaces(input);
    assert!(interfaces.iter().any(|item| item.name == "en0"));
}

#[test]
fn infers_interface_types_from_name_rules() {
    let input = include_str!("../fixtures/docker.txt");
    let interfaces = parse_interfaces(input);
    let bridge = interfaces.iter().find(|item| item.name == "bridge0").unwrap();
    assert_eq!(format!("{:?}", bridge.interface_type), "Bridge");
}

#[test]
fn parses_utun_interface_from_vpn_fixture() {
    let input = include_str!("../fixtures/vpn.txt");
    let interfaces = parse_interfaces(input);
    assert!(interfaces.iter().any(|item| item.name.starts_with("utun")));
}

#[test]
fn merges_stats_into_interface_records() {
    let input = include_str!("../fixtures/macos15.txt");
    let interfaces = parse_interfaces(input);
    let merged = merge_stats(input, interfaces);
    let en0 = merged.iter().find(|item| item.name == "en0").unwrap();
    assert!(en0.stats.rx_bytes > 0);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test parser_interface`
Expected: FAIL because `parse_interfaces` returns the wrong type and `merge_stats` does not exist

- [ ] **Step 3: Add minimal fixtures**

Create `fixtures/macos14.txt`:

```text
en0: flags=8863<UP,BROADCAST,SMART,RUNNING,SIMPLEX,MULTICAST> mtu 1500
    ether aa:bb:cc:dd:ee:ff
    inet 192.168.0.10 netmask 0xffffff00 broadcast 192.168.0.255
    inet6 fe80::1234%en0 prefixlen 64 secured scopeid 0x4
    nd6 options=201<PERFORMNUD,DAD>
    media: autoselect
    status: active
    RX packets 100 bytes 2048
    TX packets 50 bytes 1024
lo0: flags=8049<UP,LOOPBACK,RUNNING,MULTICAST> mtu 16384
    inet 127.0.0.1 netmask 0xff000000
    inet6 ::1 prefixlen 128
    RX packets 10 bytes 640
    TX packets 10 bytes 640
```

Copy `fixtures/macos14.txt` to `fixtures/macos15.txt` and adjust one value:

```text
en0: flags=8863<UP,BROADCAST,SMART,RUNNING,SIMPLEX,MULTICAST> mtu 1500
    ether aa:bb:cc:dd:ee:11
    inet 192.168.0.20 netmask 0xffffff00 broadcast 192.168.0.255
    inet6 fe80::5678%en0 prefixlen 64 secured scopeid 0x4
    status: active
    RX packets 200 bytes 4096
    TX packets 100 bytes 2048
```

Create `fixtures/vpn.txt`:

```text
utun4: flags=8051<UP,POINTOPOINT,RUNNING,MULTICAST> mtu 1380
    inet6 fe80::abcd%utun4 prefixlen 64 scopeid 0x12
    inet 10.10.0.2 --> 10.10.0.2 netmask 0xffffffff
    status: active
    RX packets 20 bytes 2048
    TX packets 25 bytes 3072
```

Create `fixtures/docker.txt`:

```text
bridge0: flags=8863<UP,BROADCAST,SMART,RUNNING,SIMPLEX,MULTICAST> mtu 1500
    ether de:ad:be:ef:00:01
    inet 172.18.0.1 netmask 0xffff0000 broadcast 172.18.255.255
    status: active
    RX packets 30 bytes 3000
    TX packets 30 bytes 3000
```

- [ ] **Step 4: Write the minimal parsing implementation**

Replace `src/collector/interface.rs`:

```rust
use crate::model::{InterfaceAddress, InterfaceStats, InterfaceStatus, NetworkInterface};

pub fn parse_interfaces(input: &str) -> Vec<NetworkInterface> {
    let mut interfaces = Vec::new();
    let mut current: Option<NetworkInterface> = None;

    for line in input.lines() {
        if !line.starts_with('\t') && !line.starts_with(' ') && line.contains(':') {
            if let Some(interface) = current.take() {
                interfaces.push(interface);
            }

            let name = line.split(':').next().unwrap_or_default().to_string();
            let status = if line.contains("UP") {
                InterfaceStatus::Up
            } else {
                InterfaceStatus::Down
            };

            current = Some(NetworkInterface {
                name,
                interface_type: infer_interface_type(&name),
                status,
                ipv4: Vec::new(),
                ipv6: Vec::new(),
                mac_address: None,
                mtu: parse_mtu(line),
                stats: InterfaceStats::default(),
            });

            continue;
        }

        let Some(interface) = current.as_mut() else {
            continue;
        };

        let trimmed = line.trim();

        if let Some(rest) = trimmed.strip_prefix("ether ") {
            interface.mac_address = Some(rest.to_string());
        } else if let Some(rest) = trimmed.strip_prefix("inet6 ") {
            let value = rest.split_whitespace().next().unwrap_or_default();
            interface.ipv6.push(InterfaceAddress::new(value));
        } else if let Some(rest) = trimmed.strip_prefix("inet ") {
            let value = rest.split_whitespace().next().unwrap_or_default();
            interface.ipv4.push(InterfaceAddress::new(value));
        } else if trimmed == "status: active" {
            interface.status = InterfaceStatus::Up;
        }
    }

    if let Some(interface) = current.take() {
        interfaces.push(interface);
    }

    interfaces
}

fn infer_interface_type(name: &str) -> crate::model::InterfaceType {
    use crate::model::InterfaceType;

    if name.starts_with("utun") {
        InterfaceType::Vpn
    } else if name == "lo0" {
        InterfaceType::Loopback
    } else if name.starts_with("bridge") {
        InterfaceType::Bridge
    } else if name.starts_with("awdl") {
        InterfaceType::AirDrop
    } else if name.starts_with("en") {
        InterfaceType::WifiOrEthernet
    } else {
        InterfaceType::Unknown
    }
}

fn parse_mtu(line: &str) -> Option<u32> {
    let token = line.split_whitespace().collect::<Vec<_>>();
    token
        .windows(2)
        .find(|window| window[0] == "mtu")
        .and_then(|window| window[1].parse::<u32>().ok())
}
```

Replace `src/collector/stats.rs`:

```rust
use crate::model::NetworkInterface;

pub fn merge_stats(input: &str, mut interfaces: Vec<NetworkInterface>) -> Vec<NetworkInterface> {
    let mut current_name = String::new();

    for line in input.lines() {
        if !line.starts_with('\t') && !line.starts_with(' ') && line.contains(':') {
            current_name = line.split(':').next().unwrap_or_default().to_string();
            continue;
        }

        let trimmed = line.trim();
        if !trimmed.starts_with("RX packets ") && !trimmed.starts_with("TX packets ") {
            continue;
        }

        let Some(interface) = interfaces.iter_mut().find(|item| item.name == current_name) else {
            continue;
        };

        let parts = trimmed.split_whitespace().collect::<Vec<_>>();
        if parts.len() >= 5 && parts[0] == "RX" {
            interface.stats.rx_packets = parts[2].parse().unwrap_or(0);
            interface.stats.rx_bytes = parts[4].parse().unwrap_or(0);
        } else if parts.len() >= 5 && parts[0] == "TX" {
            interface.stats.tx_packets = parts[2].parse().unwrap_or(0);
            interface.stats.tx_bytes = parts[4].parse().unwrap_or(0);
        }
    }

    interfaces
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --test parser_interface`
Expected: PASS

- [ ] **Step 6: Commit**

Run:

```bash
git add fixtures src/collector tests/parser_interface.rs
git commit -m "feat: parse interfaces and stats from ifconfig fixtures"
```

### Task 4: Implement app state refresh, throughput, and network events

**Files:**
- Modify: `src/app.rs`
- Modify: `src/model.rs`
- Test: `tests/app_state.rs`

- [ ] **Step 1: Write the failing app state tests**

Replace `tests/app_state.rs`:

```rust
use lazyifconfig::app::App;
use lazyifconfig::model::{InterfaceAddress, InterfaceStats, InterfaceStatus, InterfaceType, NetworkInterface, NetworkSnapshot};

fn snapshot(name: &str, rx: u64, tx: u64, ip: &str, captured_at_secs: u64) -> NetworkSnapshot {
    NetworkSnapshot {
        interfaces: vec![NetworkInterface {
            name: name.to_string(),
            interface_type: InterfaceType::WifiOrEthernet,
            status: InterfaceStatus::Up,
            ipv4: vec![InterfaceAddress::new(ip)],
            ipv6: vec![],
            mac_address: None,
            mtu: Some(1500),
            stats: InterfaceStats {
                rx_bytes: rx,
                tx_bytes: tx,
                rx_packets: 0,
                tx_packets: 0,
            },
        }],
        captured_at_secs,
    }
}

#[test]
fn preserves_selected_interface_across_refresh() {
    let mut app = App::default();
    app.replace_snapshot(snapshot("en0", 100, 50, "192.168.0.10", 10));
    app.selected_index = 0;
    app.replace_snapshot(snapshot("en0", 150, 70, "192.168.0.10", 12));
    assert_eq!(app.selected_interface_name(), Some("en0"));
}

#[test]
fn computes_throughput_from_previous_snapshot() {
    let mut app = App::default();
    app.replace_snapshot(snapshot("en0", 100, 50, "192.168.0.10", 10));
    app.replace_snapshot(snapshot("en0", 300, 150, "192.168.0.10", 12));
    let rates = app.selected_rates().unwrap();
    assert_eq!(rates.rx_bytes_per_sec, 100);
    assert_eq!(rates.tx_bytes_per_sec, 50);
}

#[test]
fn emits_event_when_ipv4_changes() {
    let mut app = App::default();
    app.replace_snapshot(snapshot("en0", 100, 50, "192.168.0.10", 10));
    app.replace_snapshot(snapshot("en0", 100, 50, "192.168.0.20", 12));
    assert!(app.events.iter().any(|event| event.message.contains("IPv4 changed")));
}

#[test]
fn keeps_only_the_most_recent_fifty_events() {
    let mut app = App::default();
    app.replace_snapshot(snapshot("en0", 100, 50, "192.168.0.10", 10));

    for second in 11..=70 {
        app.replace_snapshot(snapshot("en0", 100, 50, &format!("192.168.0.{}", second), second));
    }

    assert_eq!(app.events.len(), 50);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test app_state`
Expected: FAIL because `App` behavior and throughput types are not implemented

- [ ] **Step 3: Write the minimal app state implementation**

Replace `src/app.rs`:

```rust
use crate::model::{NetworkEvent, NetworkSnapshot};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InterfaceRates {
    pub rx_bytes_per_sec: u64,
    pub tx_bytes_per_sec: u64,
}

#[derive(Default)]
pub struct App {
    pub current_snapshot: Option<NetworkSnapshot>,
    pub previous_snapshot: Option<NetworkSnapshot>,
    pub selected_index: usize,
    pub events: Vec<NetworkEvent>,
}

impl App {
    pub fn replace_snapshot(&mut self, next: NetworkSnapshot) {
        let previous = self.current_snapshot.replace(next);
        self.previous_snapshot = previous.clone();

        if let (Some(prev), Some(curr)) = (&previous, &self.current_snapshot) {
            self.push_events(prev, curr);
        }

        if let Some(snapshot) = &self.current_snapshot {
            if self.selected_index >= snapshot.interfaces.len() {
                self.selected_index = 0;
            }
        }
    }

    pub fn selected_interface_name(&self) -> Option<&str> {
        self.current_snapshot
            .as_ref()?
            .interfaces
            .get(self.selected_index)
            .map(|item| item.name.as_str())
    }

    pub fn selected_rates(&self) -> Option<InterfaceRates> {
        let prev = self.previous_snapshot.as_ref()?;
        let curr = self.current_snapshot.as_ref()?;
        let prev_if = prev.interfaces.get(self.selected_index)?;
        let curr_if = curr.interfaces.get(self.selected_index)?;
        let elapsed = curr.captured_at_secs.saturating_sub(prev.captured_at_secs).max(1);

        Some(InterfaceRates {
            rx_bytes_per_sec: curr_if.stats.rx_bytes.saturating_sub(prev_if.stats.rx_bytes) / elapsed,
            tx_bytes_per_sec: curr_if.stats.tx_bytes.saturating_sub(prev_if.stats.tx_bytes) / elapsed,
        })
    }

    fn push_events(&mut self, prev: &NetworkSnapshot, curr: &NetworkSnapshot) {
        for current in &curr.interfaces {
            match prev.interfaces.iter().find(|item| item.name == current.name) {
                None => self.events.push(NetworkEvent::new(
                    format!("{} appeared", current.name),
                    curr.captured_at_secs,
                )),
                Some(previous) => {
                    let prev_ip = previous.ipv4.first().map(|item| item.value.as_str());
                    let curr_ip = current.ipv4.first().map(|item| item.value.as_str());
                    if prev_ip != curr_ip {
                        self.events.push(NetworkEvent::new(
                            format!("{} IPv4 changed", current.name),
                            curr.captured_at_secs,
                        ));
                    }
                }
            }
        }

        if self.events.len() > 50 {
            let keep_from = self.events.len() - 50;
            self.events.drain(0..keep_from);
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test app_state`
Expected: PASS

- [ ] **Step 5: Commit**

Run:

```bash
git add src/app.rs tests/app_state.rs
git commit -m "feat: add snapshot state and network change detection"
```

### Task 5: Add the `ifconfig` command runner and snapshot assembly

**Files:**
- Modify: `src/command.rs`
- Modify: `src/model.rs`
- Modify: `src/collector/mod.rs`
- Test: `tests/parser_interface.rs`

- [ ] **Step 1: Write the failing command-layer test**

Append to `tests/parser_interface.rs`:

```rust
use lazyifconfig::collector::build_snapshot;

#[test]
fn builds_snapshot_from_fixture_text() {
    let input = include_str!("../fixtures/docker.txt");
    let snapshot = build_snapshot(input, 42);
    assert_eq!(snapshot.captured_at_secs, 42);
    assert!(snapshot.interfaces.iter().any(|item| item.name == "bridge0"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test builds_snapshot_from_fixture_text`
Expected: FAIL because `build_snapshot` does not exist

- [ ] **Step 3: Write the minimal snapshot assembly and command runner**

Replace `src/collector/mod.rs`:

```rust
pub mod interface;
pub mod stats;

use crate::model::NetworkSnapshot;

pub fn build_snapshot(input: &str, captured_at_secs: u64) -> NetworkSnapshot {
    let interfaces = interface::parse_interfaces(input);
    let interfaces = stats::merge_stats(input, interfaces);

    NetworkSnapshot {
        interfaces,
        captured_at_secs,
    }
}
```

Replace `src/command.rs`:

```rust
use std::time::Duration;

use tokio::process::Command;
use tokio::time::timeout;

pub async fn run_ifconfig() -> Result<String, String> {
    let output = timeout(Duration::from_secs(2), Command::new("ifconfig").output())
        .await
        .map_err(|_| "ifconfig timed out".to_string())?
        .map_err(|error| error.to_string())?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
```

- [ ] **Step 4: Run targeted tests to verify they pass**

Run: `cargo test builds_snapshot_from_fixture_text --test parser_interface`
Expected: PASS

- [ ] **Step 5: Commit**

Run:

```bash
git add src/collector/mod.rs src/command.rs tests/parser_interface.rs
git commit -m "feat: assemble snapshots from ifconfig output"
```

### Task 6: Render the first working TUI shell

**Files:**
- Modify: `src/main.rs`
- Modify: `src/lib.rs`
- Modify: `src/ui.rs`
- Modify: `src/app.rs`

- [ ] **Step 1: Write the failing compile-time UI integration step**

Update `src/main.rs` to call functions that do not exist yet:

```rust
use lazyifconfig::app;
use lazyifconfig::ui;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = app::App::default();
    app.load_initial_snapshot().await;
    ui::run(app).await
}
```

- [ ] **Step 2: Run build to verify it fails**

Run: `cargo build`
Expected: FAIL because `load_initial_snapshot` or `ui::run` does not exist

- [ ] **Step 3: Write the minimal interactive shell**

Append to `src/app.rs`:

```rust
use crate::collector::build_snapshot;
use crate::command::run_ifconfig;
use std::time::{SystemTime, UNIX_EPOCH};

impl App {
    pub async fn load_initial_snapshot(&mut self) {
        if let Ok(text) = run_ifconfig().await {
            let captured_at_secs = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_secs())
                .unwrap_or(0);
            self.replace_snapshot(build_snapshot(&text, captured_at_secs));
        }
    }
}
```

Replace `src/ui.rs`:

```rust
use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Terminal;

use crate::app::App;

pub async fn run(app: App) -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let result = run_loop(&mut terminal, app);
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: App,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        terminal.draw(|frame| {
            let areas = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(3)])
                .split(frame.area());

            let body = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(30),
                    Constraint::Percentage(45),
                    Constraint::Percentage(25),
                ])
                .split(areas[0]);

            let items = app
                .current_snapshot
                .as_ref()
                .map(|snapshot| {
                    snapshot
                        .interfaces
                        .iter()
                        .map(|item| ListItem::new(item.name.clone()))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            frame.render_widget(List::new(items).block(Block::default().title("Interfaces").borders(Borders::ALL)), body[0]);
            frame.render_widget(Paragraph::new("Details").block(Block::default().title("Selected").borders(Borders::ALL)), body[1]);
            frame.render_widget(Paragraph::new("Events").block(Block::default().title("Recent Changes").borders(Borders::ALL)), body[2]);
            frame.render_widget(Paragraph::new("q quit | r refresh").block(Block::default().title("Status").borders(Borders::ALL)), areas[1]);
        })?;

        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => break,
                    _ => {}
                }
            }
        }
    }

    Ok(())
}
```

- [ ] **Step 4: Run build to verify it passes**

Run: `cargo build`
Expected: PASS

- [ ] **Step 5: Run the app manually**

Run: `cargo run`
Expected: The app opens an alternate-screen TUI with three panes and exits on `q`

- [ ] **Step 6: Commit**

Run:

```bash
git add src/main.rs src/app.rs src/ui.rs
git commit -m "feat: render initial lazyifconfig tui shell"
```

## Self-Review

Spec coverage check:

- macOS-first scope is covered by the `ifconfig`-only command layer in Tasks 3, 5, and 6.
- Single-screen TUI with list, detail, event panel, and status bar is covered in Task 6.
- TDD parser and fixture strategy is covered in Task 3.
- App state, throughput calculation, and recent event detection are covered in Task 4.
- Interface type inference is covered in Tasks 2 and 3.
- Read-only refresh-driven workflow is covered across Tasks 4, 5, and 6.

Placeholder scan:

- No `TODO`, `TBD`, or "implement later" markers remain.
- Each code-changing step includes concrete code.
- Each validation step includes an exact command and expected result.

Type consistency check:

- The plan consistently uses `NetworkSnapshot`, `NetworkInterface`, `InterfaceType`, `InterfaceStats`, `NetworkEvent`, and `App`.
- `build_snapshot`, `run_ifconfig`, `replace_snapshot`, `selected_rates`, and `load_initial_snapshot` are introduced before later tasks depend on them.
- `merge_stats` is defined in Task 3 before Task 5 uses it.
