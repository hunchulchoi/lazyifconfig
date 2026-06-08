# lazyifconfig Design Spec

Date: 2026-06-08
Status: Draft for user review

## Summary

`lazyifconfig` is a keyboard-first terminal UI for inspecting network configuration on macOS, inspired by LazyGit, LazyDocker, and htop-style observability tools.

This first version is intentionally scoped as a macOS-first MVP:

- Terminal-only application written in Rust
- Read-only by default
- Fast startup and simple interaction model
- Data collection through macOS `ifconfig` output
- Internal structure split by domain so Linux support can be added later without rewriting the UI
- Lightweight network change detection to surface meaningful events

## MVP Scope

The first implementation cycle includes:

- Interface list in the left pane
- Interface details in the right pane
- Small event panel for recent network changes
- Status bar at the bottom
- Automatic refresh every 2 seconds
- Manual refresh with `r`
- Keyboard navigation with `j/k`, arrow keys, `Tab` or `Shift+Tab` if needed, `r` to refresh, and `q` to quit
- Per-interface RX/TX byte display
- Derived RX/s and TX/s text values based on consecutive snapshots

The first implementation cycle does not include:

- Port inspector
- Active connections view
- Packet graphs or sparkline-style charts
- Docker integration
- Windows support
- Mutating network state

## Platform and Collection Strategy

The MVP targets macOS first.

Data is collected from `ifconfig` rather than OS-specific Rust APIs or multiple command sources. This keeps the first version straightforward and aligns the UI with the same operational data a developer would inspect manually in the terminal.

The implementation should use a "1 shaped like 2" approach:

- One system-command-based collection strategy across the app
- But code organized as separate domain collectors instead of a single monolithic parser

This allows fast delivery now while preserving clean seams for future Linux support or selective replacement with native libraries.

## Architecture

The application is divided into five layers.

### 1. Command Layer

Responsibility:

- Execute macOS commands
- Apply timeout and error handling
- Return raw stdout/stderr text

Expected initial commands:

- `ifconfig`

This layer should not know anything about UI or app state.

### 2. Collector Layer

Responsibility:

- Parse raw command output
- Convert output into typed internal models
- Keep parsing logic separated by domain

Initial collectors:

- `interface_collector`
- `stats_collector`
- `event_collector` or equivalent change-detection logic at the state boundary

Collectors should be independently testable from stored command output samples.

### 3. Model Layer

Responsibility:

- Define UI-independent domain types
- Represent the normalized network state used by the rest of the app

Expected core models:

- `NetworkInterface`
- `InterfaceType`
- `InterfaceAddress`
- `InterfaceStats`
- `NetworkSnapshot`
- `NetworkEvent`

The model layer should not depend on ratatui rendering concerns.

### 4. App State Layer

Responsibility:

- Hold the latest snapshot
- Track current selection
- Preserve selected interface across refreshes
- Track refresh timing and lightweight error state
- Compute derived values such as RX/s and TX/s

This layer is the boundary between data collection and presentation.

### 5. UI Layer

Responsibility:

- Render panes and status bar with ratatui
- Translate app state into a keyboard-driven interface
- Avoid direct dependency on command output or parsing details

The UI should only consume stable internal models and app state.

## Initial Screen Layout

The MVP uses a single main screen instead of tabs or modal-heavy navigation.

### Left Pane: Interfaces

Display a compact list of interfaces including:

- Interface name
- UP or DOWN state
- Primary IPv4 address when available

The selected row is highlighted. The list should stay stable during refreshes so users do not lose context.

### Right Pane: Details

Display details for the selected interface:

- Name
- Inferred interface type
- Status
- IPv4 addresses
- IPv6 addresses
- MAC address
- MTU
- RX bytes
- TX bytes
- RX/s
- TX/s

The right pane is optimized for quick troubleshooting, not exhaustive protocol analysis.

### Event Panel

Display a short rolling list of recent changes detected between snapshots.

Examples:

- `utun4 appeared`
- `bridge0 appeared`
- `en0 IPv4 changed`
- `en0 status changed`

This gives the MVP a monitoring-oriented workflow rather than acting only as a static interface viewer.

Interface type inference is heuristic and intentionally lightweight in v1.

Initial mapping rules:

- `utun*` -> `VPN`
- `lo0` -> `Loopback`
- `bridge*` -> `Bridge`
- `awdl*` -> `AirDrop`
- `en*` -> `Wi-Fi/Ethernet`
- everything else -> `Unknown`

### Bottom Status Bar

Display:

- Last refresh time
- Auto-refresh state
- Basic key hints such as `q`, `j`, and `k`
- Short warning summary if part of collection failed

## Refresh and Data Flow

The app refreshes every 2 seconds by default, with manual refresh available through `r`.

Flow:

1. App starts and collects an initial snapshot
2. App renders using that snapshot
3. Background refresh collects a new snapshot every 2 seconds
4. App state merges the snapshot while preserving the current selection
5. App state derives human-readable events from snapshot differences
6. UI rerenders from updated state

Traffic rate calculation:

- The app stores at least the previous and current stats snapshot
- `RX/s` and `TX/s` are computed from byte deltas over elapsed time
- The MVP shows numeric throughput text only
- The design should leave room for a later graph component without changing the data model boundary

Change detection:

- The app compares the current snapshot against the previous snapshot
- It records a bounded list of the 50 most recent events
- Event generation is best-effort and should prefer clear, high-signal changes over noisy churn
- Initial event scope should include interface appearance, disappearance, link state changes, and IPv4 changes

## Error Handling

The app should degrade gracefully rather than failing entirely when one collector has trouble.

Examples:

- If interface parsing succeeds but throughput derivation fails, the interface list still renders
- If traffic stats are unavailable, detail fields show `unavailable` rather than crashing
- If a command times out, the previous snapshot can remain visible while the error is surfaced in lightweight form

Error categories to preserve internally:

- Command execution failure
- Timeout
- Parsing failure
- Partial data unavailable

User-facing behavior should remain simple for the MVP:

- Continue rendering as much data as possible
- Show concise failure hints in the status bar
- Avoid a dedicated debug or logs panel in v1

## Testing Strategy

Implementation follows test-driven development.

The first testing focus is on the layers with the highest behavioral risk.

### Parser Tests

Use captured or representative macOS command outputs as fixtures and verify they parse into the expected internal models.

Suggested fixture set:

- `fixtures/macos14.txt`
- `fixtures/macos15.txt`
- `fixtures/vpn.txt`
- `fixtures/docker.txt`

Important cases:

- Multiple interfaces
- Interfaces with IPv4 and IPv6
- Loopback and tunnel-style interfaces
- Missing or partial fields
- Command output variations that still occur on supported macOS versions

### App State Tests

Verify:

- Selection is preserved across refresh
- Updated snapshots replace stale values correctly
- Missing fields do not break rendering state
- RX/s and TX/s calculations use the correct deltas
- Interface type inference follows the expected name-based mapping rules
- Network events are emitted only for meaningful changes
- Event history stays bounded to 50 items and ordered

### UI Tests

Keep UI testing minimal in the first cycle.

The MVP should favor:

- Small rendering-oriented checks where practical
- Stronger confidence in parser and state behavior

The main reliability target for v1 is the conversion pipeline from command output to normalized model state.

## Non-Goals

The following are explicitly out of scope for this design cycle:

- Building every feature in the original product brief
- Creating a multi-view or tabbed network toolbox
- Adding Docker-specific views
- Parsing route or gateway information in the first cycle
- Implementing privileged actions
- Designing a plugin architecture

## Implementation Notes

Suggested crate and library direction:

- `ratatui` for rendering
- `crossterm` for input and terminal integration
- `tokio` for refresh scheduling and command execution orchestration
- `serde` and `serde_json` only where serialization support is helpful for fixtures or debugging

The codebase should prefer small modules with clear single-purpose responsibilities. If a file starts to combine command execution, parsing, state transitions, and rendering, that is a signal to split responsibilities earlier rather than later.

## Open Decision Record

Decisions already made for this spec:

- Scope: MVP first
- Platform: macOS first
- Data source: `ifconfig` parsing first
- Internal organization: domain collectors instead of one giant parser
- UI structure: single main view with list, detail, event panel, and status bar

## Ready-to-Plan Boundary

This spec is ready for implementation planning once the user confirms it.

The first implementation plan should likely cover:

- Project scaffolding
- Domain model definitions
- Command execution abstraction
- Initial parser tests
- Basic app state
- First renderable TUI shell
