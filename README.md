# lazyifconfig

`lazyifconfig` is a terminal UI for inspecting local network state on macOS.
It combines `ifconfig`, `netstat`, `route`, `lsof`, and a periodic public IP lookup into a single view for interfaces, subnets, routes, connections, ports, and recent network events.

## Screenshots

![Interface view](docs/screenshots/raw/screenshot-interface-raw.png)

<p>
  <img src="docs/screenshots/raw/screenshot-network-raw.png" alt="Network view" width="49%" />
  <img src="docs/screenshots/raw/screenshot-ports-raw.png" alt="Ports view" width="49%" />
</p>
<p>
  <img src="docs/screenshots/raw/screenshot-timeline-raw.png" alt="Timeline view" width="49%" />
</p>

## Features

- Interface view with IPv4 and IPv6 details
- Network grouping by subnet
- Active connection list from `netstat -an`
- Listening port list from `lsof`
- Route view from `netstat -rn`
- Event timeline for interface and public IP changes
- Raw command output capture inside the app
- Background GitHub Release check with self-update support

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

From crates.io:

```bash
cargo install lazyifconfig
```

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
- `u`: check GitHub Release now
- `U`: apply pending update now
- `j` / `k`: move selection
- `i`: interface view
- `n`: network view
- `c`: connections view
- `p`: ports view
- `e`: timeline view
- `g`: routes view
- `/` and `[` : scroll in list-heavy views

Some views expose additional actions in the footer, including filtering ports, copying values, WHOIS lookup, and raw output inspection.

When a newer GitHub Release is found, `lazyifconfig` will attempt to install the matching macOS release artifact automatically. After the binary is replaced, restart the app to run the new version.

## Testing

```bash
cargo test
```

## Release

GitHub Actions creates a release when a tag matching `v*` is pushed.

```bash
git tag v0.2.0
git push origin v0.2.0
```

You can also trigger the `Create Release Tag` workflow from GitHub Actions.
Enter `0.2.0` or `v0.2.0` as the input, and it will:

- verify the version matches `Cargo.toml`
- create an annotated `v*` tag
- push the tag so the `Release` workflow builds artifacts and publishes the GitHub Release

For crates.io publishing, trigger the `Publish Crate` workflow from GitHub Actions.
Enter `0.2.0` or `v0.2.0`, and it will:

- verify the version matches `Cargo.toml`
- run `cargo publish --dry-run --locked`
- optionally publish to crates.io with the `CARGO_REGISTRY_TOKEN` secret

The release workflow builds and uploads artifacts for:

- Linux `x86_64-unknown-linux-gnu`
- macOS `x86_64-apple-darwin`
- macOS `aarch64-apple-darwin`
- Windows `x86_64-pc-windows-msvc`

## Notes

- The app is built around macOS networking command output, so behavior on other platforms is not expected to be reliable.
- Public IP information is fetched from `https://ipinfo.io/json`.
