# Windows Tool Support Design

## Goal

Make `lazyifconfig` usable on Windows without failing on Unix-only command names, and make Tools Hub behavior graceful when `whois` or `openssl` are not installed.

## Scope

This change adds Windows-aware command selection and parsers for the local network views:

- Interface collection uses `ipconfig /all` on Windows.
- Route table collection uses `route PRINT` on Windows.
- Default route collection uses `route PRINT 0.0.0.0` on Windows.
- Listening ports use `netstat -ano -p tcp` on Windows.
- Ping uses `ping -n 4` on Windows.
- Traceroute uses `tracert -h 8` on Windows.

Tools Hub changes:

- TLS Inspector stops depending on the external `openssl` command for normal operation. It uses a native Rust TLS implementation to connect, collect certificate metadata, and report handshake diagnostics.
- Whois Lookup keeps using a local `whois` command when present. If the command is missing, it falls back to RDAP over HTTPS and presents structured ownership metadata when available.
- If neither a native/fallback path can produce useful data, the tool returns a clear diagnostic instead of an opaque process-spawn error.

## Non-Goals

- No full Windows UI redesign.
- No Windows service/process-name enrichment for ports in this slice.
- No guarantee that all third-party Whois/RDAP registries return complete metadata.
- No packet capture or admin-only route APIs.
- No replacement of existing macOS/Linux command behavior.

## Architecture

Command specs become explicitly three-way where needed: Linux, Windows, and macOS/other Unix. Existing `*_for_os(os: &str)` helpers stay as testable seams.

Collectors gain parser support for Windows command output:

- `collector::interface` recognizes `ipconfig /all` adapter sections, extracts adapter name, IPv4/IPv6 addresses, subnet masks when available, gateway, and MAC address.
- `collector::routes` recognizes `route PRINT` active route rows and maps them into `RouteEntry`.
- `collector::ports` recognizes `netstat -ano -p tcp` rows and maps listening TCP sockets into `ListeningPort`, using PID as both `pid` and limited process identity when command/user cannot be resolved.

TLS Inspector uses a native Rust path. The command-spec function remains for tests and CLI display compatibility, but `tls::run` no longer shells out to `openssl` on any platform. The output sections preserve the current shape: Summary, Certificate, Diagnostics, Raw Output.

Whois Lookup uses this order:

1. If `whois` exists in `PATH`, run current command path.
2. If process spawn fails with command-not-found, run RDAP fallback.
3. If RDAP fails, return a ToolResult with diagnostics explaining both failures.

## Data Flow

The runtime refresh loop calls the same high-level command spec functions. On Windows those specs point to Windows-native command names. Existing parser entrypoints keep their names, so `tick_update` stays unchanged.

Tools Hub still calls `tools::run_tool`. Individual tool modules own fallback behavior and return `ToolResult` rather than leaking missing-command errors where a fallback exists.

## Error Handling

Windows collectors tolerate partial output. Missing fields become empty strings or `None` rather than panics.

TLS native errors become diagnostics like DNS failure, TCP connect failure, TLS handshake failure, or certificate parse failure.

Whois fallback distinguishes:

- local `whois` missing
- local `whois` exited non-zero
- RDAP endpoint returned no useful fields
- network failure

## Tests

Add fixture-style tests for Windows output parsing:

- `ipconfig /all` with Wi-Fi, loopback, and virtual adapter sections.
- `route PRINT` with default route, on-link route, IPv6 rows.
- `netstat -ano -p tcp` with listening and established rows.

Add command spec tests:

- Windows interface command is `ipconfig /all`.
- Windows route command is `route PRINT`.
- Windows default route command is `route PRINT 0.0.0.0`.
- Windows listening ports command is `netstat -ano -p tcp`.
- Windows ping command is `ping -n 4`.
- Windows traceroute command is `tracert -h 8`.

Add tool behavior tests:

- TLS target parsing still accepts `host` and `host:port`.
- Whois missing-command path can return an RDAP fallback result via a testable parser/helper.

Verification commands:

```bash
cargo test --no-run
cargo test
```

The target outcome on Windows is a fully passing test suite.
