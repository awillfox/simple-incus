# simple-incus v1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A no-login, no-certificate, enterprise-grade web UI for a single Incus host, written in Rust (Leptos SSR + hydration), served on port 5000 from the host itself.

**Architecture:** One `cargo-leptos` crate (features `ssr` / `hydrate`). The server talks to incusd over its Unix socket through a small typed client; Leptos server functions expose reads/actions; axum websocket routes proxy exec/console/VGA streams and fan out the Incus event stream. Built on Windows, cross-compiled to Linux with cargo-zigbuild, deployed by one PowerShell script.

**Tech Stack:** Rust 1.99 stable, Leptos (version from the current `leptos-rs/start-axum` template), axum, tokio, hyper 1 + hyper-util, tokio-tungstenite, serde/serde_json, thiserror, tracing; Tailwind CSS via cargo-leptos; vendored xterm.js and spice-html5; cargo-zigbuild + zig for `x86_64-unknown-linux-gnu`.

**Spec:** `docs/superpowers/specs/2026-10-18-simple-incus-design.md`

## Global Constraints

- Listen on `0.0.0.0:5000` and `[::]:5000`, plain HTTP. No auth, no TLS, no certificates anywhere.
- Incus access only via socket `/var/lib/incus/unix.socket` in production; `core.https_address` is unset on deploy.
- Config via env: `SI_LISTEN` (default `0.0.0.0:5000`), `SI_INCUS` (default `unix:///var/lib/incus/unix.socket`; also accepts `tcp://host:port` for dev over an SSH-forwarded socket).
- Default Incus project only; instances addressed by name.
- No runtime internet loads in the browser: fonts, xterm.js, spice-html5 are vendored under `public/` and served by the binary. Only server-side fetch allowed: `https://images.linuxcontainers.org/streams/v1/` (catalog).
- No `alert()` / `confirm()` / `prompt()` — in-app modals only.
- Look: high-end enterprise; dark theme default with light toggle; dense tables; status pills; toasts.
- Test instances are named with prefix `si-test-`; e2e tests touch nothing else.
- Must not break Docker or LXD on the host.
- Commits: conventional style, **no AI co-author / attribution lines**. Work on `main`, push to `origin` (`git@github.com:awillfox/simple-incus.git`).
- Dev loop on Windows: `ssh -N -L 127.0.0.1:8444:/var/lib/incus/unix.socket jemma@192.168.77.150` then `SI_INCUS=tcp://127.0.0.1:8444 cargo leptos watch`.

## Review Focus

1. **incusd unavailable or restarting** — every page renders with a "Incus unreachable" banner instead of a 500/panic; the event hub reconnects with backoff and the banner clears by itself. (Test in Task 2 and Task 5.)
2. **Long operations (image download during create)** — the create call returns immediately with an operation id; progress streams via events; no HTTP request waits on the download. (Test in Task 10.)
3. **Invalid instance names** — Incus rules (1–63 chars, `[a-zA-Z0-9-]`, starts with a letter, no trailing `-`, not all digits) rejected inline before submit with a clear message. (Test in Task 10.)
4. **VM shell without `incus-agent` / instance stopped** — Terminal tab shows a clear message ("agent not running — use Console") within ~5 s instead of hanging. (Test in Task 11.)
5. **Browser closes a terminal/console tab** — the proxied Incus websockets and the exec operation are closed (no leaked shells); instance stopping mid-session closes the browser terminal with a message. (Test in Task 11.)

---

## File Structure

```
Cargo.toml                    # cargo-leptos metadata, features ssr/hydrate, bin-target-triple
tailwind.config.js, style/tailwind.css
public/fonts/                 # Inter + JetBrains Mono (woff2)
public/vendor/xterm/          # xterm.js + addon-fit (pinned version)
public/vendor/spice/          # spice-html5
public/js/term.js, spice.js   # tiny glue exposing window.siTerm / window.siSpice
src/main.rs                   # ssr entry: config, router, ws routes, event hub start
src/lib.rs                    # hydrate entry
src/config.rs                 # Config::from_env
src/app.rs                    # <App/>, router, shell layout
src/model.rs                  # shared serde types used by server fns and UI
src/incus/mod.rs              # pub use; ssr only
src/incus/transport.rs        # Unix/TCP connector, request(), websocket connect
src/incus/client.rs           # IncusClient typed methods
src/incus/raw.rs              # serde structs mirroring Incus JSON
src/incus/map.rs              # raw -> model mapping, CPU % calc
src/incus/events.rs           # EventHub
src/incus/catalog.rs          # simplestreams fetch/parse/cache
src/server/*.rs               # #[server] fns grouped: instances, snapshots, images, host, create
src/ws/events.rs              # GET /ws/events
src/ws/proxy.rs               # bidirectional ws pump
src/ws/exec.rs, console.rs    # GET /ws/exec/{name}, /ws/console/{name}?type=console|vga
src/ui/components/*.rs        # sidebar, topbar, data_table, status_pill, modal, toast, drawer, tabs, meter, banner
src/ui/pages/*.rs             # overview, instances, instance_detail, images, create
tests/fixtures/*.json         # recorded Incus responses
tests/fake_incus.rs           # in-process fake incusd over TCP (shared test helper)
deploy/simple-incus.service
deploy/deploy.ps1
README.md
```

---

### Task 1: Toolchain + scaffold that cross-builds

**Files:**
- Create: whole template via `cargo leptos new --git https://github.com/leptos-rs/start-axum` into repo root (keep `docs/`), `src/config.rs`, `.gitignore` (`target/`, `node_modules/`)
- Modify: `Cargo.toml` (`[package.metadata.leptos]`: `site-addr = "0.0.0.0:5000"`, `bin-target-triple = "x86_64-unknown-linux-gnu"` only under a `release-linux` profile/env, `tailwind-input-file = "style/tailwind.css"`)

**Interfaces:**
- Produces: `Config { listen: SocketAddr, incus: IncusEndpoint }`, `enum IncusEndpoint { Unix(PathBuf), Tcp(SocketAddr) }`, `Config::from_env() -> anyhow::Result<Config>`, `IncusEndpoint::parse(&str) -> Result<IncusEndpoint, String>`.

- [ ] **Step 1:** Install tooling: `rustup target add wasm32-unknown-unknown x86_64-unknown-linux-gnu`, `cargo install cargo-leptos cargo-generate cargo-zigbuild --locked`, zig (`winget install zig.zig`). Verify `cargo leptos --version`, `zig version` print versions.
- [ ] **Step 2: Failing test** in `src/config.rs`:
```rust
#[test] fn parses_endpoints() {
    assert_eq!(IncusEndpoint::parse("unix:///var/lib/incus/unix.socket").unwrap(),
               IncusEndpoint::Unix("/var/lib/incus/unix.socket".into()));
    assert_eq!(IncusEndpoint::parse("tcp://127.0.0.1:8444").unwrap(),
               IncusEndpoint::Tcp("127.0.0.1:8444".parse().unwrap()));
    assert!(IncusEndpoint::parse("http://x").is_err());
}
#[test] fn defaults() { /* with SI_LISTEN/SI_INCUS unset: listen 0.0.0.0:5000, Unix default socket */ }
```
- [ ] **Step 3:** Run `cargo test --features ssr config` → FAIL; implement; → PASS.
- [ ] **Step 4:** `main.rs` binds both `0.0.0.0:5000` and `[::]:5000` (`IPV6_V6ONLY` on the v6 socket) when `SI_LISTEN` is default.
- [ ] **Step 5:** Linux cross build: `$env:LEPTOS_BIN_TARGET_TRIPLE="x86_64-unknown-linux-gnu"; $env:LEPTOS_BIN_CARGO_COMMAND="cargo-zigbuild"; cargo leptos build --release`. Expected: `target/x86_64-unknown-linux-gnu/release/simple-incus` exists and `target/site/` contains pkg wasm/css. Record the working incantation in `deploy/build.ps1`.
- [ ] **Step 6:** Commit `chore: scaffold leptos app with linux cross-build`.

### Task 2: Incus transport + client core

**Files:** Create `src/incus/{mod,transport,client,raw}.rs`, `tests/fake_incus.rs`, `tests/fixtures/{server.json,instances_recursion2.json,operation_running.json,error_404.json}`

**Interfaces:**
- Produces:
  - `IncusError` (thiserror): `Unreachable(String)`, `Api { status: u16, message: String }`, `Decode(String)`.
  - `IncusClient::new(IncusEndpoint) -> IncusClient` (Clone, cheap).
  - `async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, IncusError>` — unwraps Incus envelope `{type, metadata, error, error_code}`.
  - `async fn send<B: Serialize, T: DeserializeOwned>(&self, method: Method, path: &str, body: Option<&B>) -> Result<Response<T>, IncusError>` where `enum Response<T> { Sync(T), Async(Operation) }`.
  - `async fn wait(&self, op_id: &str, timeout_s: u32) -> Result<Operation, IncusError>` (`/1.0/operations/{id}/wait?timeout=`).
  - `async fn ws_connect(&self, path: &str) -> Result<WebSocketStream<BoxedIo>, IncusError>` (used by Tasks 5, 11).
  - `async fn ping(&self) -> bool` (`GET /1.0`).
  - raw `Operation { id, status, status_code, metadata: Value, err, resources }`.
- Test helper `fake_incus::spawn(routes: Vec<(Method, &str, u16, Value)>) -> SocketAddr` (hyper server on 127.0.0.1:0; runs on Windows).

- [ ] **Step 1: Failing tests** (`#[tokio::test]`, client pointed at `IncusEndpoint::Tcp(fake)`):
  - `get_unwraps_sync_metadata` — fixture `server.json` → `api_version == "1.0"`.
  - `api_error_maps` — 404 fixture → `Err(Api{status:404, message:"Instance not found"})`.
  - `async_returns_operation` — 202 + `operation_running.json` → `Response::Async(op)` with `op.id`.
  - `unreachable_is_error_not_panic` — endpoint `Tcp(127.0.0.1:1)` → `Err(Unreachable(_))`, `ping() == false`.
- [ ] **Step 2:** Run `cargo test --features ssr incus::` → FAIL.
- [ ] **Step 3:** Implement transport with hyper-util legacy client over a custom connector: `#[cfg(unix)] tokio::net::UnixStream` for `Unix`, `TcpStream` for `Tcp`; Host header `incus`. Websocket via `tokio_tungstenite::client_async` on the same connector. 10 s connect timeout.
- [ ] **Step 4:** Tests PASS. Also verify live via SSH forward: tiny `examples/ping.rs` prints server version against real host.
- [ ] **Step 5:** Commit `feat(incus): typed client over unix/tcp socket`.

### Task 3: Shared model + mapping + stats math

**Files:** Create `src/model.rs`, `src/incus/map.rs`

**Interfaces:**
- Produces (in `model.rs`, `Serialize + Deserialize + Clone + PartialEq`):
  - `enum InstanceKind { Container, Vm }`, `enum InstanceStatus { Running, Stopped, Frozen, Error, Other(String) }`
  - `InstanceSummary { name, kind, status, image_desc: String, ipv4: Vec<String>, ipv6: Vec<String>, cpu_pct: Option<f32>, mem_used: u64, mem_limit: Option<u64>, disk_used: u64, net_rx: u64, net_tx: u64, created_at: String, profiles: Vec<String> }`
  - `SnapshotInfo { name, created_at, stateful: bool }`, `HostInfo { cpu_model, cpu_threads, mem_total, mem_used, pools: Vec<PoolUsage>, incus_version }`, `PoolUsage { name, driver, used, total }`
  - `ImageInfo { fingerprint, aliases: Vec<String>, description, kind, size, uploaded_at }`, `CatalogImage { alias: String /* "ubuntu/24.04" */, distro, release, variant, container: bool, vm: bool, description }`
  - `CreateRequest { name, image: ImageSource, kind, cpu: Option<u32>, memory_mib: Option<u64>, disk_gib: Option<u64>, profiles: Vec<String>, network: Option<String>, pool: Option<String>, cloud_init: Option<String>, start: bool }`, `enum ImageSource { Local { fingerprint }, Catalog { alias } }`
  - `UiEvent { kind: UiEventKind, instance: Option<String>, op_id: Option<String>, progress: Option<String>, message: String }`, `enum UiEventKind { Lifecycle, Operation }`
  - `enum Action { Start, Stop { force: bool }, Restart { force: bool }, Freeze, Unfreeze }`
- In `map.rs`: `fn to_summary(raw: &raw::Instance) -> InstanceSummary`, `struct CpuTracker` with `fn sample(&mut self, name: &str, usage_ns: u64, at: Instant) -> Option<f32>` (percent of one core × 100 / cpu limit-or-host-threads, `None` on first sample).

- [ ] **Step 1: Failing tests** using `instances_recursion2.json` (one running container with eth0 10.178.204.x + lo, one stopped VM):
  - `ips_exclude_loopback_and_link_local` — container `ipv4 == ["10.178.204.23"]`, no `127.0.0.1`, no `fe80::`.
  - `stopped_vm_maps` — `kind == Vm`, `status == Stopped`, `cpu_pct == None`.
  - `cpu_pct_from_deltas` — samples 0 ns then 500_000_000 ns 1 s later with 1 thread → `Some(50.0)`.
- [ ] **Step 2:** FAIL → implement → PASS (`cargo test --features ssr map`).
- [ ] **Step 3:** Commit `feat: shared model and incus mapping`.

### Task 4: Instance read + action server functions

**Files:** Create `src/server/instances.rs`; Modify `src/incus/client.rs`

**Interfaces:**
- Consumes: `IncusClient`, `to_summary`, `CpuTracker` (held in axum state `AppState { incus: IncusClient, cpu: Arc<Mutex<CpuTracker>> }` — define `AppState` here in `src/state.rs`, provided to server fns via `provide_context`; Task 5 adds field `hub: EventHub`, Task 9 adds `catalog: Catalog`).
- Produces client methods: `list_instances() -> Vec<raw::Instance>` (`/1.0/instances?recursion=2`), `instance_state_put(name, action: &Action) -> Operation`, `delete_instance(name) -> Operation`.
- Server fns: `#[server] list_instances() -> Result<Vec<InstanceSummary>, ServerFnError>`, `#[server] instance_action(name: String, action: Action) -> Result<String /*op id*/, ServerFnError>`, `#[server] delete_instance(name: String, force: bool) -> Result<(), ServerFnError>` — if running and `force`, stop(force) + wait 30 s, then delete + wait; if running and `!force` → error `"Instance is running; enable force to stop and delete"`.

- [ ] **Step 1: Failing tests** against fake incus: `action_puts_state` (asserts body `{"action":"stop","force":true,"timeout":-1}`), `delete_running_without_force_errors`, `delete_running_with_force_stops_then_deletes` (fake records call order).
- [ ] **Step 2:** FAIL → implement → PASS.
- [ ] **Step 3:** Commit `feat: instance list and lifecycle server fns`.

### Task 5: Event hub + `/ws/events`

**Files:** Create `src/incus/events.rs`, `src/ws/events.rs`

**Interfaces:**
- Produces: `EventHub::start(client: IncusClient) -> EventHub` (spawns task), `EventHub::subscribe(&self) -> broadcast::Receiver<UiEvent>`, `EventHub::connected(&self) -> watch::Receiver<bool>`. Upstream `/1.0/events?type=lifecycle,operation`; reconnect backoff 1 s → 2 s → … max 30 s; maps Incus event JSON to `UiEvent` (instance name from `metadata.source` `/1.0/instances/<name>` or operation `resources.instances`).
- Route `GET /ws/events` → JSON-serialised `UiEvent` text frames, plus `{"kind":"Status","connected":bool}` on change.
- UI side (used by Tasks 7–10): `fn use_events() -> ReadSignal<Option<UiEvent>>` and `fn use_incus_connected() -> ReadSignal<bool>` in `src/ui/events.rs` (hydrate: browser WebSocket with auto-reconnect; ssr: inert signal).

- [ ] **Step 1: Failing tests:** `maps_lifecycle_event` (fixture `instance-started` → `UiEvent{kind:Lifecycle, instance:Some("c1")}`), `maps_operation_progress` (download progress string carried), `reconnects_after_drop` (fake ws server closes; hub reconnects; `connected` goes false→true; next event delivered).
- [ ] **Step 2:** FAIL → implement → PASS.
- [ ] **Step 3:** Commit `feat: incus event hub and browser event stream`.

### Task 6: App shell + design system

**Files:** Create `src/ui/components/{sidebar,topbar,data_table,status_pill,modal,toast,drawer,tabs,meter,banner}.rs`, `style/tailwind.css`, `public/fonts/*`; Modify `src/app.rs`

**Interfaces:**
- Produces components (props exact):
  - `StatusPill(status: InstanceStatus)`; `Meter(value: f64, max: f64, label: String)`
  - `DataTable<T>(rows: Signal<Vec<T>>, columns: Vec<Column<T>>, row_key: fn(&T)->String, on_row_click: Option<Callback<T>>)` with `Column<T>{ title, sort: Option<fn(&T,&T)->Ordering>, render: fn(&T)->AnyView }`, client-side sort + text filter.
  - `ConfirmModal(open: RwSignal<bool>, title, body: String, confirm_label: String, danger: bool, on_confirm: Callback<()>, extra: Option<ChildrenFn>)`
  - `provide_toasts()`, `fn toast(kind: ToastKind, msg: impl Into<String>)` (`Success|Error|Info`, auto-dismiss 5 s, errors sticky until closed).
  - `Drawer(open: RwSignal<bool>, title: String, children)`, `Tabs(tabs: Vec<(String /*slug*/, String /*label*/)>, active: Signal<String>)`
  - `IncusBanner()` — reads `use_incus_connected()`; red top banner "Incus is unreachable — retrying…".
- Routes: `/` Overview, `/instances`, `/instances/:name/:tab?`, `/images`. Sidebar with icons, active state, collapsible; topbar with theme toggle (persisted in `localStorage` key `si-theme`, wrapped in try/catch-equivalent) and live connection dot.
- Design tokens as CSS variables (`--bg`, `--surface`, `--border`, `--text`, `--muted`, `--accent`, `--ok`, `--warn`, `--danger`), dark default + `[data-theme=light]`. Inter for UI, JetBrains Mono for IDs/IPs/terminal. 13–14 px base, tight table rows.

- [ ] **Step 1: Failing test** (ssr render): `shell_renders_nav` — render `<App/>` at `/` to string, assert contains `Overview`, `Instances`, `Images`, and no `<script src="http` (no external loads).
- [ ] **Step 2:** FAIL → implement → PASS.
- [ ] **Step 3:** Visual check with `cargo leptos watch` + SSH forward; screenshot dark and light at 1440 px and 390 px widths; no horizontal scroll.
- [ ] **Step 4:** Commit `feat(ui): app shell and design system`.

### Task 7: Instances page + live updates + actions

**Files:** Create `src/ui/pages/instances.rs`

**Interfaces:** Consumes `list_instances`, `instance_action`, `delete_instance`, `use_events`, `DataTable`, `ConfirmModal`, `toast`.

- Columns: Name (mono, link), Status pill, Type, IPv4, CPU %, Memory used/limit meter, Disk, Created. Row action menu: Start/Stop/Restart/Freeze/Delete (contextual by status). Stop/Restart offer "Force" in modal; Delete modal has "Force (stop first)" checkbox, defaulting off.
- Refetch on any `Lifecycle` event for any instance (debounced 300 ms) and every 5 s while `document.visibilityState == "visible"`.
- Header: counts (Running / Stopped / Total), search box, "Create instance" button (opens Task 10 drawer).

- [ ] **Step 1: Failing test** (ssr render with fake incus): `instances_page_lists_rows` — HTML contains both fixture names and `Running` pill.
- [ ] **Step 2:** FAIL → implement → PASS.
- [ ] **Step 3:** Manual on real host: launch `si-test-a` via CLI, it appears within 1 s without reload; stop from UI → pill turns Stopped live; delete with force works; toasts shown.
- [ ] **Step 4:** Commit `feat(ui): instances page with live state`.

### Task 8: Instance detail — Summary, Snapshots, Metrics

**Files:** Create `src/server/snapshots.rs`, `src/ui/pages/instance_detail.rs`

**Interfaces:**
- Server fns: `get_instance(name) -> InstanceDetail` (`InstanceSummary` + `config: BTreeMap<String,String>` filtered to `limits.*`, `image.*`, `cloud-init.*` keys + `devices` summary), `list_snapshots(name) -> Vec<SnapshotInfo>`, `create_snapshot(name, snap: String) -> ()`, `restore_snapshot(name, snap) -> ()` (`PUT /1.0/instances/{name}` with `{"restore": snap}`), `delete_snapshot(name, snap) -> ()` — each waits on its operation (60 s).
- Tabs: `summary | terminal | console | snapshots | metrics` (terminal/console bodies come from Tasks 11–12; render placeholder component names `TerminalTab` / `ConsoleTab` to be filled there). Metrics tab: CPU/mem/disk/net current values + 5-min in-browser sparkline from 5 s polls.

- [ ] **Step 1: Failing tests:** `restore_sends_restore_body`, `snapshot_name_default` (empty name → server generates `snap-YYYYMMDD-HHMMSS`).
- [ ] **Step 2:** FAIL → implement → PASS.
- [ ] **Step 3:** Manual: create/restore/delete snapshot on `si-test-a`; restore requires confirm modal.
- [ ] **Step 4:** Commit `feat: instance detail with snapshots and metrics`.

### Task 9: Images (local + catalog) + Overview

**Files:** Create `src/incus/catalog.rs`, `src/server/{images,host}.rs`, `src/ui/pages/{images,overview}.rs`, `tests/fixtures/simplestreams_images.json` (trimmed real sample)

**Interfaces:**
- `Catalog::new() -> Catalog`; `async fn get(&self) -> Result<Arc<Vec<CatalogImage>>, String>` — fetches `https://images.linuxcontainers.org/streams/v1/images.json` (reqwest, rustls), caches 1 h, on fetch failure returns last good copy or `Err("Catalog unavailable: …")`. `fn parse(json: &Value, arch: &str) -> Vec<CatalogImage>` keeps only `arch == "amd64"`, latest version per product, `container` true if any item ftype `incus.tar.xz`/`squashfs`, `vm` true if `disk-kvm.img`.
- Server fns: `list_images() -> Vec<ImageInfo>`, `delete_image(fingerprint) -> ()`, `catalog(query: String) -> Vec<CatalogImage>` (case-insensitive match on alias/description, max 200), `host_info() -> HostInfo` (`/1.0/resources`, `/1.0/storage-pools?recursion=1` + `/resources` per pool, `/1.0` version).
- Images page tabs Local / Catalog; catalog grouped by distro with facets (distro, container/VM); "Launch" opens create drawer pre-filled `ImageSource::Catalog{alias}`.
- Overview: host CPU/mem meters, pool usage meters, instance counts by status, live "Recent activity" list (last 20 `UiEvent`s since page load).

- [ ] **Step 1: Failing tests:** `catalog_parse_filters_arch` (no arm64 entries), `catalog_flags_vm_and_container` (ubuntu/24.04 → both true), `catalog_serves_stale_on_failure`.
- [ ] **Step 2:** FAIL → implement → PASS.
- [ ] **Step 3:** Commit `feat: images, catalog browser and overview`.

### Task 10: Create instance (advanced)

**Files:** Create `src/server/create.rs`, `src/ui/pages/create.rs` (drawer component `CreateDrawer(open, prefill: Option<ImageSource>)`)

**Interfaces:**
- `fn validate_name(name: &str) -> Result<(), String>` (shared, both features) — rules in Review Focus #3.
- Server fns: `create_options() -> CreateOptions { profiles: Vec<String>, networks: Vec<String> /*managed only*/, pools: Vec<String> }`, `create_instance(req: CreateRequest) -> Result<String /*op id*/, ServerFnError>` — returns as soon as Incus accepts (no wait). Body: `source` = `{type:"image", alias, server:"https://images.linuxcontainers.org", protocol:"simplestreams"}` or `{type:"image", fingerprint}`; `type` = `container|virtual-machine`; config `limits.cpu`, `limits.memory` = `"{n}MiB"`, `cloud-init.user-data`; devices `root` (`pool`, `size` = `"{n}GiB"`) and `eth0` (`network`) only when overridden; `start: req.start`.
- UI: sections Basics (name, image picker with local + catalog search, type toggle disabled per image capability), Resources, Network & Storage, Cloud-init (monospace textarea, starts with `#cloud-config` template). On submit: toast with live progress from `UiEvent.progress` for that `op_id`; on success navigate to `/instances/{name}`; on failure sticky error toast with Incus message.

- [ ] **Step 1: Failing tests:** `validate_name_cases` — ok: `"web1"`, `"si-test-a"`; err: `""`, `"1abc"`, `"abc-"`, `"a_b"`, `"123"`, 64×`"a"`. `create_body_catalog_vm` — asserts exact JSON for a VM from `ubuntu/24.04` with cpu 2, memory 2048, disk 20, cloud-init. `create_returns_without_waiting` — fake returns 202 and never completes op; server fn returns op id in < 1 s.
- [ ] **Step 2:** FAIL → implement → PASS.
- [ ] **Step 3:** Manual: create `si-test-c` (alpine container) and `si-test-vm` (ubuntu/24.04 VM) from the UI; progress visible; both reach Running.
- [ ] **Step 4:** Commit `feat: advanced create instance`.

### Task 11: WS proxy + web shell (xterm.js)

**Files:** Create `src/ws/{proxy,exec}.rs`, `public/vendor/xterm/*`, `public/js/term.js`, `src/ui/terminal.rs` (`TerminalTab(name: String)`)

**Interfaces:**
- `async fn pump(browser: axum::extract::ws::WebSocket, upstream: WebSocketStream<BoxedIo>, on_browser_text: impl Fn(String) -> Option<ControlMsg>)` — copies binary both ways; closing either side closes the other; returns when both closed.
- Route `GET /ws/exec/{name}?cols=&rows=` → POST `/1.0/instances/{name}/exec` `{command:["/bin/sh","-c","exec su -l 2>/dev/null || exec bash -l 2>/dev/null || exec sh -l"], interactive:true, "wait-for-websocket":true, width, height, environment:{TERM:"xterm-256color"}}` → connect `fds["0"]` (data) and `fds["control"]`; browser text frames `{"type":"resize","cols":N,"rows":N}` → control `{"command":"window-resize","args":{"width":"N","height":"N"}}`. On close: close control ws, which ends the op.
- Pre-check: instance not Running → send text `{"type":"error","message":"Instance is not running"}` and close. VM exec failing / no data within 5 s → `{"type":"error","message":"incus-agent is not running in this VM — use the Console tab"}`.
- `window.siTerm.open(el, wsUrl) -> handle` / `handle.dispose()` in `term.js`; Leptos calls via `wasm_bindgen` extern; fit addon on resize.

- [ ] **Step 1: Failing tests** (fake incus ws): `pump_closes_upstream_when_browser_closes`, `resize_maps_to_window_resize`, `not_running_sends_error_frame`.
- [ ] **Step 2:** FAIL → implement → PASS.
- [ ] **Step 3:** Manual: shell into `si-test-c` and `si-test-vm`; `htop` renders; resize works; close tab → `incus operation list` shows no lingering exec op within 5 s.
- [ ] **Step 4:** Commit `feat: browser shell via exec websocket proxy`.

### Task 12: Text console + VGA (SPICE)

**Files:** Create `src/ws/console.rs`, `public/vendor/spice/*`, `public/js/spice.js`, `src/ui/console.rs` (`ConsoleTab(name: String, kind: InstanceKind)`)

**Interfaces:**
- Route `GET /ws/console/{name}?type=console|vga&cols=&rows=` → POST `/1.0/instances/{name}/console` `{type, width, height}` → connect `fds["0"]` + `fds["control"]`, reuse `pump`. `vga` rejected for containers with error frame.
- ConsoleTab: sub-toggle **Text | Graphical** (Graphical only for VMs). Text reuses `siTerm` against `/ws/console/...?type=console`; Graphical uses `window.siSpice.open(el, wsUrl)` (spice-html5 `SpiceMainConn` with ws URL), with Ctrl-Alt-Del button and fullscreen.
- Text console note above terminal: "Press Enter if the screen is blank".

- [ ] **Step 1: Failing tests:** `vga_rejected_for_container`, `console_posts_type` (body `{"type":"vga",...}`).
- [ ] **Step 2:** FAIL → implement → PASS.
- [ ] **Step 3:** Manual on `si-test-vm`: text console shows login prompt; graphical shows VM framebuffer and accepts keyboard.
- [ ] **Step 4:** Commit `feat: text and VGA consoles`.

### Task 13: Deploy + README + end-to-end check

**Files:** Create `deploy/simple-incus.service`, `deploy/deploy.ps1`, `README.md`

**Interfaces:** Consumes `deploy/build.ps1` (Task 1).

- `simple-incus.service`: `User=simple-incus`, `Group=simple-incus`, `SupplementaryGroups=incus-admin`, `ExecStart=/usr/local/bin/simple-incus`, `Environment=LEPTOS_SITE_ROOT=/usr/local/share/simple-incus/site`, `Restart=on-failure`, `After=incus.service`, `NoNewPrivileges=yes`, `ProtectSystem=strict`, `ProtectHome=yes`.
- `deploy.ps1` (one command, uses SSH key + passwordless sudo, host param default `jemma@192.168.77.150`): run `build.ps1`; `scp` binary + `target/site` tarball + unit to `/tmp`; remote: create system user if missing, install files, `systemctl daemon-reload && enable --now && restart simple-incus`, `incus config unset core.https_address`, then `curl -fsS http://127.0.0.1:5000/ >/dev/null` and print `OK`.
- README: what it is, dev loop (SSH forward + `SI_INCUS`), deploy, config env vars, and the security warning verbatim: "No login: anyone who can reach port 5000 controls Incus, which is root on the host. Use only on a trusted LAN. Never port-forward it to the internet."

- [ ] **Step 1:** Run `./deploy/deploy.ps1` → prints `OK`.
- [ ] **Step 2:** Verify from Windows: `curl http://192.168.77.150:5000/` 200; `ss -ltnp` on host shows `simple-incus` on :5000 and **no** incusd TCP listener; `docker ps` and `lxc list` still work.
- [ ] **Step 3:** Full manual pass of spec Goals 1–9 against the host using only `si-test-*` instances; then delete all `si-test-*` instances and any images they pulled.
- [ ] **Step 4:** Reboot test: `sudo reboot`; after boot UI is up and banner is clear.
- [ ] **Step 5:** Commit `feat: deploy script, systemd unit, README` and push `main`.
