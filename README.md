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
- Route view from `netstat -rn` on macOS and `ip route show` on Linux
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
- `g`: routes view
- `/` and `[` : scroll in list-heavy views

Some views expose additional actions in the footer, including filtering ports, copying values, WHOIS lookup, and raw output inspection.

When a newer GitHub Release is found, `lazyifconfig` will attempt to install the matching release artifact automatically. After the binary is replaced, restart the app to run the new version.

## Testing

```bash
cargo test
```

## Release

GitHub Actions creates a release when a tag matching `v*` is pushed.

```bash
git tag v0.2.2
git push origin v0.2.2
```

You can also trigger the `Create Release Tag` workflow from GitHub Actions.
Enter `0.2.2` or `v0.2.2` as the input, and it will:

- verify the version matches `Cargo.toml`
- create an annotated `v*` tag
- push the tag so the `Release` workflow builds artifacts and publishes the GitHub Release

For crates.io publishing, trigger the `Publish Crate` workflow from GitHub Actions.
Enter `0.2.2` or `v0.2.2`, and it will:

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
