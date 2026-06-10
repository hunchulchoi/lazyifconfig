# lazyifconfig

`lazyifconfig` is a terminal UI for inspecting local network state on macOS.
It combines `ifconfig`, `netstat`, `route`, `lsof`, and a periodic public IP lookup into a single view for interfaces, subnets, routes, connections, ports, and recent network events.

## Features

- Interface view with IPv4 and IPv6 details
- Network grouping by subnet
- Active connection list from `netstat -an`
- Listening port list from `lsof`
- Route view from `netstat -rn`
- Event timeline for interface and public IP changes
- Raw command output capture inside the app

## Requirements

- macOS
- Rust toolchain
- System commands available in `PATH`:
  - `ifconfig`
  - `netstat`
  - `route`
  - `lsof`
  - `curl`

## Install

From GitHub:

```bash
cargo install --git https://github.com/hunchulchoi/lazyifconfig.git
```

From a local checkout:

```bash
cargo install --path .
```

## Build

```bash
cargo build --release
```

## Run

```bash
cargo run --release
```

## Controls

- `q`: quit
- `r`: refresh
- `j` / `k`: move selection
- `i`: interface view
- `n`: network view
- `c`: connections view
- `p`: ports view
- `e`: timeline view
- `g`: routes view
- `/` and `[` : scroll in list-heavy views

Some views expose additional actions in the footer, including filtering ports, copying values, WHOIS lookup, and raw output inspection.

## Testing

```bash
cargo test
```

## Release

GitHub Actions creates a release when a tag matching `v*` is pushed.

```bash
git tag v0.1.0
git push origin v0.1.0
```

The release workflow builds and uploads artifacts for:

- Linux `x86_64-unknown-linux-gnu`
- macOS `x86_64-apple-darwin`
- macOS `aarch64-apple-darwin`
- Windows `x86_64-pc-windows-msvc`

## Notes

- The app is built around macOS networking command output, so behavior on other platforms is not expected to be reliable.
- Public IP information is fetched from `https://ipinfo.io/json`.
