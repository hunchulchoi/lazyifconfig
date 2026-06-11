# lazyifconfig

`lazyifconfig` is a terminal UI for inspecting local network state.
It combines local interface, route, connection, port, and public IP data into a single view for interfaces, subnets, routes, connections, ports, and recent network events.

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
- Listening port list from `lsof` on macOS and `ss` on Linux
- Port and connection detail tabs with focused summaries, process/WHOIS drilldowns, and shared keyboard navigation
- Route Inspector with default route summary, destination path lookup, VPN route detection, diagnostics, raw route output, and a sortable/filterable route table
- Tools input modal with muted placeholders, focused-field highlighting, and empty-input warnings
- Event timeline for interface and public IP changes
- Raw command output capture inside the app
- Background GitHub Release check with self-update support

## Requirements

- macOS or Linux
- Rust toolchain
- System commands available in `PATH`:
  - macOS: `ifconfig`, `netstat`, `route`
  - Linux: `ip`, `netstat`, `ss`
  - macOS: `lsof`
  - `curl`

## Install

From Homebrew tap:

```bash
brew tap choihunchul/homebrew-tap
brew install lazyifconfig
```

From crates.io:

```bash
cargo install lazyifconfig
```

From GitHub:

```bash
cargo install --git https://github.com/choihunchul/lazyifconfig.git
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
- `S`: save timeline to a timestamped file in current directory (e.g. `lazyifconfig-timeline-YYYYMMDD-HHMMSS.txt`)
- `g`: Route Inspector
- `/` and `[` : scroll in list-heavy views
- In Route Inspector: `Enter` starts destination path lookup, `Tab` switches inspector sections, `Home`/`End` or `1`-`4` jumps between sections, `/` filters routes, `o` opens raw route output
- In Ports and Connections: `Tab` switches the detail tabs
- In Tools: `Tab` moves between input fields and the first field is focused when the modal opens

Some views expose additional actions in the footer, including filtering ports, copying values, WHOIS lookup, and raw output inspection.

When a newer GitHub Release is found, `lazyifconfig` will attempt to install the matching release artifact automatically. After the binary is replaced, restart the app to run the new version.

## Testing

```bash
cargo test
```

## Release

GitHub Actions creates a release when a tag matching `v*` is pushed.

```bash
git tag v0.2.4
git push origin v0.2.4
```

You can also trigger the `Create Release Tag` workflow from GitHub Actions.
Enter `0.2.4` or `v0.2.4` as the input, and it will:

- verify the version matches `Cargo.toml`
- create an annotated `v*` tag
- push the tag so the `Release` workflow builds artifacts and publishes the GitHub Release

For crates.io publishing, trigger the `Publish Crate` workflow from GitHub Actions.
Enter `0.2.4` or `v0.2.4`, and it will:

- verify the version matches `Cargo.toml`
- run `cargo publish --dry-run --locked`
- optionally publish to crates.io with the `CARGO_REGISTRY_TOKEN` secret

For Homebrew publishing, create a tap repository such as `choihunchul/homebrew-tap`,
add a `HOMEBREW_TAP_TOKEN` secret with push access to that repo, then trigger
the `Publish Homebrew Tap` workflow. It will:

- download the macOS release tarballs for the selected tag
- compute SHA-256 checksums
- write `Formula/lazyifconfig.rb` into the tap repository
- push the formula update so `brew tap ... && brew install lazyifconfig` works

The release workflow builds and uploads artifacts for:

- Linux `x86_64-unknown-linux-gnu`
- macOS `x86_64-apple-darwin`
- macOS `aarch64-apple-darwin`
- Windows `x86_64-pc-windows-msvc`

## Notes

- Linux interface and route views use `ip`, and the port view uses `ss`; the connection view still relies on `netstat`.
- Public IP information is fetched from `https://ipinfo.io/json`.

## Project Rules

- [Project Rules](PROJECT_RULES.md): release workflows and checkpoint commit policy for development.
