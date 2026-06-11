# Windows Tool Support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Windows use native command names where available and remove hard failures from missing `whois`/`openssl` tools.

**Architecture:** Keep existing command-spec seams and parser entrypoints, adding Windows branches plus fixture tests. Replace TLS command execution with a native Rust TLS connection path. Add Whois RDAP fallback when the local `whois` command is unavailable.

**Tech Stack:** Rust 2021, Tokio, Ratatui, `rustls`/`webpki-roots`/`x509-parser` for TLS, existing `curl` command helper for RDAP HTTP fallback.

---

## File Structure

- Modify `Cargo.toml`: add TLS parsing/client dependencies.
- Modify `src/command.rs`: Windows command specs and command tests.
- Modify `src/tools/ping.rs`: Windows ping args.
- Modify `src/tools/traceroute.rs`: Windows tracert args.
- Modify `src/collector/interface.rs`: parse `ipconfig /all`.
- Modify `src/collector/routes.rs`: parse `route PRINT`.
- Modify `src/collector/ports.rs`: parse `netstat -ano -p tcp`.
- Modify `src/tools/tls.rs`: use native Rust TLS instead of `openssl` process execution.
- Modify `src/tools/whois.rs`: add RDAP fallback for missing `whois`.
- Modify `tests/tools_runner.rs`: Windows command-spec tests and tool helper tests.

---

### Task 1: Windows Command Specs

**Files:**
- Modify: `src/command.rs`
- Modify: `src/tools/ping.rs`
- Modify: `src/tools/traceroute.rs`
- Test: existing unit tests and `tests/tools_runner.rs`

- [ ] **Step 1: Add Windows command branches**

Update `*_command_spec_for_os` helpers:

```rust
interface_command_spec_for_os("windows") => ipconfig /all
route_table_command_spec_for_os("windows") => route PRINT
default_route_command_spec_for_os("windows") => route PRINT 0.0.0.0
listening_ports_command_spec_for_os("windows") => netstat -ano -p tcp
route_path_command_spec_for_os("windows", destination) => route PRINT destination
```

- [ ] **Step 2: Add Windows tool command specs**

Update:

```rust
ping::command_spec_for_os("windows", target) => ping -n 4 target
traceroute::command_spec_for_os("windows", target) => tracert -h 8 target
```

- [ ] **Step 3: Add tests**

Add assertions to existing command/tool spec tests for Windows command output.

- [ ] **Step 4: Verify**

Run:

```bash
cargo test command::tests:: tests::tools_runner
```

Expected: command and tool runner tests pass.

---

### Task 2: Windows Parsers

**Files:**
- Modify: `src/collector/interface.rs`
- Modify: `src/collector/routes.rs`
- Modify: `src/collector/ports.rs`

- [ ] **Step 1: Add parser detection**

Add lightweight detection at parser entrypoints:

```rust
parse_interfaces(input): if input contains "Windows IP Configuration", use Windows parser.
parse_routes(input): if input contains "IPv4 Route Table" or "Active Routes:", use Windows parser.
parse_listening_ports(input): if input contains "Proto" and "PID", use Windows netstat parser.
```

- [ ] **Step 2: Add Windows interface parser**

Parse adapter headers ending with `:` and address fields:

```text
Ethernet adapter Ethernet:
   Physical Address. . . . . . . . . : AA-BB-CC-DD-EE-FF
   IPv4 Address. . . . . . . . . . . : 192.168.0.10(Preferred)
   Subnet Mask . . . . . . . . . . . : 255.255.255.0
   Default Gateway . . . . . . . . . : 192.168.0.1
```

- [ ] **Step 3: Add Windows route parser**

Parse route rows with five columns:

```text
Network Destination        Netmask          Gateway       Interface  Metric
          0.0.0.0          0.0.0.0      192.168.0.1     192.168.0.96     35
```

- [ ] **Step 4: Add Windows port parser**

Parse TCP listening rows:

```text
TCP    0.0.0.0:135    0.0.0.0:0    LISTENING    1234
```

- [ ] **Step 5: Add fixture tests**

Add one focused test per parser for Windows sample output.

- [ ] **Step 6: Verify**

Run:

```bash
cargo test collector::
```

Expected: collector tests pass.

---

### Task 3: TLS Native Path

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/tools/tls.rs`
- Test: `tests/tools_runner.rs`

- [ ] **Step 1: Add deps**

Add:

```toml
rustls = "0.23"
webpki-roots = "0.26"
x509-parser = "0.17"
```

- [ ] **Step 2: Keep command spec for display compatibility**

Leave `tls::command_spec` available, but stop using it from `tls::run`.

- [ ] **Step 3: Implement native TLS**

Use `TcpStream`, `rustls::ClientConnection`, webpki roots, and `peer_certificates()` to collect:

```text
Target
Status
Protocol
Cipher
Subject
Issuer
Validity
SAN count
```

- [ ] **Step 4: Parse certificate metadata**

Use `x509-parser` on first peer certificate DER. If parse fails, return handshake summary plus diagnostic.

- [ ] **Step 5: Add tests**

Keep existing command-spec test. Add pure parse/format helper tests for invalid target and certificate-section fallback.

- [ ] **Step 6: Verify**

Run:

```bash
cargo test tls
```

Expected: TLS tests pass.

---

### Task 4: Whois RDAP Fallback

**Files:**
- Modify: `src/tools/whois.rs`
- Test: `tests/tools_runner.rs`

- [ ] **Step 1: Detect command-not-found**

If `run_command(&whois_spec).await` returns a spawn error containing command-not-found markers, run RDAP fallback.

- [ ] **Step 2: Add RDAP command helper**

Build RDAP URL:

```rust
if target.parse::<std::net::IpAddr>().is_ok() {
    https://rdap.org/ip/{target}
} else {
    https://rdap.org/domain/{target}
}
```

Fetch with existing async command runner using:

```text
curl -sS -L -m 10 <url>
```

- [ ] **Step 3: Add RDAP parser helper**

Parse JSON fields:

```text
handle
name
country
startAddress/endAddress
ldhName
entities[].vcardArray
events[]
```

- [ ] **Step 4: Return useful diagnostics**

Raw output includes local missing-command error and RDAP JSON. Diagnostics says RDAP fallback used.

- [ ] **Step 5: Add parser tests**

Add a pure test for RDAP JSON parsing into Summary/Diagnostics sections.

- [ ] **Step 6: Verify**

Run:

```bash
cargo test whois
```

Expected: Whois tests pass.

---

### Task 5: Full Verification and Commit

**Files:**
- All files above

- [ ] **Step 1: Compile**

Run:

```bash
cargo test --no-run
```

Expected: compile passes.

- [ ] **Step 2: Full tests**

Run:

```bash
cargo test
```

Expected: all tests pass on Windows.

- [ ] **Step 3: Diff review**

Run:

```bash
git diff --stat
git diff -- Cargo.toml src/command.rs src/collector/interface.rs src/collector/routes.rs src/collector/ports.rs src/tools/ping.rs src/tools/traceroute.rs src/tools/tls.rs src/tools/whois.rs tests/tools_runner.rs
```

- [ ] **Step 4: Commit**

Run:

```bash
git add -- Cargo.toml Cargo.lock src/command.rs src/collector/interface.rs src/collector/routes.rs src/collector/ports.rs src/tools/ping.rs src/tools/traceroute.rs src/tools/tls.rs src/tools/whois.rs tests/tools_runner.rs docs/superpowers/plans/2026-06-11-windows-tool-support.md
git commit -m "feat: add windows tool support"
```

---

## Self-Review

- Spec coverage: Windows command selection, parser support, TLS native path, Whois RDAP fallback, and full Windows test verification are covered.
- Placeholder scan: no TBD or fill-in items; each task names exact files and expected command behavior.
- Type consistency: command spec functions keep existing signatures, parser entrypoints keep existing signatures, and tool modules continue returning `ToolResult`.
