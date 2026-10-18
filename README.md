# simple-incus

A clean, no-login web UI for a single [Incus](https://linuxcontainers.org/incus/) host, written in Rust.

The official Incus UI needs TLS client certificates and trust tokens before you
can do anything. On a home LAN that's friction you don't need. simple-incus runs
**on the Incus host itself**, talks to Incus over its local Unix socket, and serves
the UI on plain HTTP port **5000**. No certificates, no tokens, no `incus remote add`.

> **Status:** under active development (v1). See the
> [design](docs/superpowers/specs/2026-10-18-simple-incus-design.md) and the
> [implementation plan](docs/superpowers/plans/2026-10-18-simple-incus-v1.md).

## ⚠️ Security

**No login: anyone who can reach port 5000 controls Incus, which is root on the host.
Use only on a trusted LAN. Never port-forward it to the internet.**

## Features (v1)

- **Instances**: containers and VMs with live state, IPs, CPU, memory, disk and network.
  Start, stop, restart, freeze and delete them (force optional).
- **Create**: pick an image from local images or the online catalog
  (images.linuxcontainers.org), choose container or VM, set CPU/memory/disk limits,
  profiles, network, storage pool and cloud-init.
- **Web shell**: an in-browser terminal (xterm.js) into any running instance.
- **Consoles**: a text console (serial / `/dev/console`) and a graphical VGA console
  (SPICE) for VMs.
- **Snapshots**: create, restore and delete.
- **Images**: manage local images and browse/search the remote catalog.
- **Live updates**: state changes and operation progress come in through the Incus event stream.
- Dark/light themes, with all assets embedded and nothing loaded from the internet in the browser.

## Architecture

```
Browser ──HTTP/WS :5000──► simple-incus (axum + Leptos SSR/hydrate)
                              │
                              ├─ Incus client ──HTTP over /var/lib/incus/unix.socket──► incusd
                              ├─ WS proxy (exec / console / VGA) ──► incusd operation websockets
                              ├─ Event hub (/1.0/events → broadcast → browsers)
                              └─ Image catalog cache (simplestreams)
```

- **Rust**: [Leptos](https://leptos.dev) 0.8 with SSR and hydration, built with `cargo-leptos`, served by axum.
- **Incus access**: the local Unix socket only. Incus's own HTTPS listener (`core.https_address`) is turned off.

## Configuration

| Env var | Default | Notes |
|---|---|---|
| `SI_LISTEN` | `0.0.0.0:5000` | With the default, it also listens on `[::]:5000` |
| `SI_INCUS` | `unix:///var/lib/incus/unix.socket` | `tcp://host:port` is also accepted (for development) |

## Development (Windows or Linux)

Prerequisites:

```sh
rustup target add wasm32-unknown-unknown x86_64-unknown-linux-gnu
cargo install cargo-leptos cargo-zigbuild --locked
# plus zig (Windows: winget install zig.zig)
```

On Windows there's no local Incus socket, so forward the server's socket over SSH
and point the app at it:

```sh
ssh -N -L 127.0.0.1:8444:/var/lib/incus/unix.socket jemma@192.168.77.150
```

```powershell
$env:SI_INCUS = "tcp://127.0.0.1:8444"
cargo leptos watch
```

Then open http://localhost:5000.

Tests:

```sh
cargo test --features ssr
```

## Build for Linux

```powershell
./deploy/build.ps1
```

This builds the site (wasm/css/js) with `cargo leptos build --release --frontend-only`
and the server binary with `cargo zigbuild` for `x86_64-unknown-linux-gnu`.

## Deploy

A one-command deploy script (`deploy/deploy.ps1`) and a systemd unit are coming in a later
v1 task. It will install the binary as `simple-incus.service`, running as a dedicated user in
the `incus-admin` group, and turn off Incus's HTTPS listener.

## License

[MIT](LICENSE)
