# simple-incus — Design

Date: 2026-10-18
Status: Approved 2026-10-18

## Intent

A self-hosted web UI for a single Incus host in a homelab, written in Rust.
The official Incus UI requires TLS client certificates / trust tokens before
you can do anything, which is friction the owner doesn't want on a home LAN.
simple-incus removes that entirely: it runs on the Incus host, talks to Incus
over the local Unix socket, and serves a no-login UI on port 5000.

Despite "simple" in the name, it should look and feel like high-end
enterprise infrastructure software (think Proxmox/vSphere/Grafana-level
polish), not a hobby dashboard.

## Target environment

- Host: `jemma@192.168.77.150`, Ubuntu 24.04 x86_64.
- Incus 7.5.x from the Zabbly stable repo, already initialised:
  storage pool `default` (dir), bridge `incusbr0`, `default` profile.
- Docker and LXD (snap) are also installed on the host and must keep working.
  A systemd unit `incus-docker-fwd.service` already allows `incusbr0` traffic
  through Docker's `FORWARD DROP` policy.
- Bare metal (no hypervisor), `/dev/kvm` present, Incus drivers `qemu | lxc`
  — VMs fully supported, so VM features (VGA, serial console, agent shell)
  can be tested on this host.
- Only the `default` Incus project exists. *(Claude's call: v1 is
  default-project only; all URLs/IDs use instance name. No project picker.)*
- No instances exist on the host today, so end-to-end tests may create and
  destroy instances. *(Claude's call: test instances use the `si-test-`
  prefix and tests only touch instances with that prefix.)*
- Development machine: Windows 11, project at `D:\Dev\simple-incus`,
  repo `git@github.com:awillfox/simple-incus.git`, work on `main`.
  Commits carry no AI co-author/attribution lines.

## Goals (v1)

1. **Instances**: list containers and VMs with state, type, IPv4/IPv6, CPU,
   memory, disk, network usage; start / stop / restart / delete (force option).
2. **Create instance (advanced)**: name; image (from local images or the
   remote catalog); container or VM; CPU, memory, root disk size limits;
   profiles; network; storage pool; cloud-init user-data. Show progress
   (image download %) until running.
3. **Web shell**: in-browser terminal (exec a login shell) for containers and
   VMs (VMs require `incus-agent`), with resize support.
4. **Text console**: attach to the instance console (serial for VMs, `/dev/console`
   for containers) — useful when a VM has no agent or won't boot.
5. **VGA console (VMs)**: graphical SPICE console in the browser.
6. **Snapshots**: list, create, restore, delete per instance.
7. **Resource stats**: per-instance CPU %, memory, disk, network; host
   overview (CPU, memory, storage pool usage).
8. **Images**: list and delete local images; browse/search the remote
   catalog at images.linuxcontainers.org and launch from it.
9. **Live updates**: instance state changes and operation progress pushed
   to the browser from the Incus event stream; stats polled.

## Non-goals

- Authentication, users, roles, TLS. (Owner's explicit decision: no login.)
- Multiple Incus hosts, clustering, remotes other than the local socket.
- Managing networks, storage pools, profiles, projects beyond *selecting*
  existing ones in the create form (read-only lists are fine).
- Instance config editing after creation beyond what snapshots/actions cover.
- Migration, backups/export, file browser.

## Anti-goals (would make it a failure even if it works)

- Requiring any certificate, trust token, or `incus remote add` step to use it.
- Needing the CLI for any of the v1 tasks above.
- Looking cheap: default browser widgets, unstyled tables, layout jank.
- Browser `alert()/confirm()` dialogs — use in-app modals.
- Loading anything from the internet at runtime (fonts, CDNs). Everything
  is embedded/served by the binary. (The remote image catalog fetch is the
  one exception and is done server-side.)
- Breaking Docker or LXD on the host.

## Decisions made by the owner

| Topic | Decision |
|---|---|
| Where it runs | On the Incus host, as a systemd service |
| Incus access | Unix socket `/var/lib/incus/unix.socket` — no certs |
| Port | simple-incus on `0.0.0.0:5000` (and `[::]:5000`), plain HTTP |
| Official Incus HTTPS listener | Turned **off** (`core.https_address` unset) |
| Auth | None |
| Language / UI | Rust, Leptos with SSR + hydration via `cargo-leptos` (axum server) |
| Build | Cross-compile on Windows, copy binary + site assets to the server |
| Console | Shell + text console + VGA (SPICE) |
| Images | Local + browse remote catalog |
| Create form | Advanced (profiles, network, pool, cloud-init) |
| Live updates | Incus event stream; stats still polled |
| Look | High-end, enterprise-grade |
| Layout priority | Desktop-first (1280–1440 px+); mobile best-effort |

## Architecture (Claude's proposal — builder may refine)

```
Browser ──HTTP/WS :5000──► simple-incus (axum + Leptos SSR)
                              │
                              ├─ Incus client ──HTTP over Unix socket──► incusd
                              ├─ WS proxy (exec / console / vga) ──► incusd operation websockets
                              ├─ Event hub (1 upstream /1.0/events ws → broadcast → browsers)
                              └─ Catalog cache (simplestreams JSON from images.linuxcontainers.org)
```

- **Incus client**: thin typed wrapper over the REST API using hyper over a
  `tokio::net::UnixStream` (e.g. `hyperlocal` or hand-rolled connector).
  Async operations are awaited via `/1.0/operations/{id}/wait` or tracked via
  events.
- **Server functions** (Leptos `#[server]`) for all reads and actions, so
  the client never needs raw Incus access.
- **WebSocket proxy**: browser opens `ws://host:5000/ws/exec/{name}` (or
  `/console/{name}?type=console|vga`). The server POSTs to Incus
  (`/1.0/instances/{name}/exec` with `interactive: true`,
  `wait-for-websocket: true`, or `/console` with `type`), gets the operation
  + fd secrets, connects to `/1.0/operations/{id}/websocket?secret=…` over the
  Unix socket, and pipes frames both ways. Control channel carries terminal
  resize.
- **Terminal**: xterm.js (vendored, served from the binary) wrapped in a
  small Leptos component via JS interop.
- **VGA**: vendored spice-html5 client fed by the proxied SPICE websocket.
- **Events**: one long-lived upstream connection to `/1.0/events?type=lifecycle,operation`
  with reconnect; fan out via `tokio::sync::broadcast` to browsers over a
  single `/ws/events` (or SSE) connection; client updates reactive state.
- **Stats**: `GET /1.0/instances?recursion=2` (state included) polled every
  ~5 s while a page is visible; CPU % derived from usage deltas server-side.
- **Remote catalog**: fetch simplestreams index from images.linuxcontainers.org,
  cache in memory (~1 h), filter to host arch (amd64), group by
  distro/release/variant, mark container/VM availability. Launch uses source
  `{type: image, server: https://images.linuxcontainers.org, protocol: simplestreams, alias}`.

## UI structure

- Left sidebar: **Overview**, **Instances**, **Images**, (Host details in Overview).
- **Overview**: host CPU/memory, storage pool usage, instance counts by state,
  recent operations/events feed.
- **Instances**: dense sortable/filterable table, status pills, bulk-free
  row actions; detail page with tabs: Summary · Terminal · Console · Snapshots · Metrics.
- **Images**: tabs Local / Catalog; catalog search with distro facets;
  "Launch" from any image opens the create drawer pre-filled.
- **Create**: right-side drawer or full page with sections (Basics, Resources,
  Network & Storage, Cloud-init), validation, then live progress toast.
- Global: toast notifications for operation results, in-app confirm modals
  for destructive actions, dark theme default with light toggle, embedded
  Inter (or similar) + monospace fonts, keyboard-friendly.

## Deployment

- Build on Windows targeting `x86_64-unknown-linux-gnu` (or `-musl`) — the
  exact toolchain (cargo-zigbuild, cross, etc.) is the builder's call, but it
  must be scripted (`deploy.ps1` or similar) so one command builds and deploys.
- Install on server: binary at `/usr/local/bin/simple-incus`, site assets
  under `/usr/local/share/simple-incus/` (or embedded), systemd unit
  `simple-incus.service`, running as a dedicated system user in the
  `incus-admin` group (socket access) — restart on failure.
- Deploy auth: SSH key (`~/.ssh/id_ed25519` on the Windows PC) for `jemma`,
  and passwordless sudo for `jemma` (owner's decision), so the deploy script
  runs unattended.
- Deployment step also runs `incus config unset core.https_address`.
- Config via env vars / small TOML: listen address (default `0.0.0.0:5000`),
  socket path (default `/var/lib/incus/unix.socket`).

## Security note (acknowledged by owner)

No login + Incus admin socket = anyone who can reach port 5000 has
root-equivalent control of the host. Acceptable on the owner's LAN; must not
be port-forwarded to the internet. README should state this plainly.

## Left to the builder

- Crate choices (hyper/hyperlocal vs reqwest-unix, tungstenite flavour, etc.).
- Exact cross-compile toolchain, as long as it's one scripted command.
- Visual design details within the "enterprise, high-end" brief; component
  library vs hand-rolled CSS (Tailwind via cargo-leptos is fine).
- Test strategy (unit tests for the Incus client against recorded responses
  are encouraged; end-to-end check against the real host).
- Delivery order of features. Owner wants v1 delivered all at once (one
  deploy at the end), not phased releases.
