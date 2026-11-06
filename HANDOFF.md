# HANDOFF — simple-incus v1

Snapshot rechecked 2026-11-06, replacing the 2026-10-18 version. Read this file first when picking the work back up.

## Where we are

**Tasks 1–9 are done and pushed. Task 10 (create instance) is written but uncommitted, unreviewed, and never tried in a real browser.** Tasks 11–13 and the final review haven't started.

No code has changed since the last session stopped on 2026-10-18: the newest source edit is from 23:42 that night.

| Check on 2026-11-06 | Result |
|---|---|
| `main` vs `origin/main` | In sync at `0d73b04`; no stashes |
| `cargo test --features ssr` | ✅ 100 passed, 0 failed, including 8 in `tests/create.rs` |
| Wasm check (hydrate) | ✅ Passes |
| `cargo clippy --features ssr --all-targets` | 5 warnings, none in Task 10 files (`data_table.rs`, `server/images.rs`, `server/snapshots.rs`) |
| Task 10 implementer report | ❌ Missing: no `task-10-report.md` in the ledger |
| Task 10 review | ❌ Not run |
| Task 10 manual test (plan Step 3) | ❌ Not done: no screenshots, no instances on the host |

The Task 10 implementer's last state isn't recorded and it can't be resumed. Treat the working tree as a draft that compiles and passes tests.

## Next steps

1. **Finish Task 10** using the checklist below, then commit as `feat: advanced create instance` and push.
2. Run **Task 11** (web shell) and **Task 12** (consoles) through the same implement, review and push loop.
3. Run **Task 13** (deploy). It runs on the real host and turns off the official Incus HTTPS listener.
4. Run the final whole-branch review on the most capable model, then do one fix wave.
5. Report the rulings to the owner, then use `superpowers:finishing-a-development-branch`.

## Finishing Task 10

The plan is at `docs/superpowers/plans/2026-10-18-simple-incus-v1.md`, line 236. Steps 1 and 2 (tests, implementation) look done. Steps 3 and 4 (manual test, commit) are open.

### Uncommitted work

- **New:** `src/server/create.rs`, `src/ui/pages/create.rs`, `src/ui/pages/create_image.rs`, `tests/create.rs`.
- **Changed to support it:**
  - `toast.rs`: busy and updatable toasts.
  - `drawer.rs`: width prop.
  - `incus/client.rs`, `incus/raw.rs`: profiles, networks, create.
  - `model.rs`: `CreateOptions`, `validate_name`, `UiEvent.status`.
  - `incus/events.rs`: maps `status`.
  - `server/snapshots.rs`: uses the shared `validate_name` (R8).
  - `app.rs`: mounts `CreateDrawer`.
- The `UiEvent` field addition also touched `ui/events.rs`, `ui/pages/overview.rs` and `tests/events.rs`.
- Git prints "CRLF will be replaced by LF" warnings. They're harmless (`.gitattributes` sets `eol=lf`).

### Problems found by reading the code

None of these has been reproduced yet. Confirm each one, then fix it or rule on it.

1. **Stale preselected image (likely bug).** `src/ui/pages/instances.rs:420` calls `create.open.set(true)` and nothing clears `prefill`. So Images "Launch", then Cancel, then Instances "Create instance" probably opens with the old image selected. Fix: use `create.open_with(None)`.
2. **Toast can spin forever.** `CreateDrawer` doesn't react to `use_incus_connected()`, and events aren't replayed after an outage. If the socket drops while a create runs, the final event is lost and the busy toast never ends. The other live pages refetch on reconnect; this one should too.
3. **No test for the new status mapping.** `maps_operation_progress` in `tests/events.rs` has `"status":"Running"` in its input but never asserts `ev.status`. The create toast's Success and Failure handling depends on this field.
4. **Deselecting all profiles is ignored.** The chips allow an empty selection, but `create_body` then sends `["default"]`. Either block the empty selection or send it as is.
5. **Disk size with no root pool.** If no selected profile has a root device, the body gets a root device without `pool`, so Incus will presumably reject it. Check that the error reads clearly.

### Steps to close it out

1. Confirm and fix the problems above, adding tests where the logic is testable.
2. Run the live dev setup (see "Dev environment"). From the UI, create `si-test-c` (Alpine container) and `si-test-vm` (`ubuntu/24.04` VM). Progress must show and both must reach Running.
3. Also check: the failure path (sticky error toast), Images "Launch" prefill, type toggle disabled per image, and the layout at 1440 and 1280 px, dark and light. Save screenshots to the ledger's `screens/`.
4. Delete the `si-test-*` instances and the images they pulled.
5. Re-run the tests and the wasm check, get the review done, then commit and push.

## What this project is

A no-login, no-certificate web UI for a single Incus host, written in Rust (Leptos 0.8 SSR + hydrate, axum).
It runs on the Incus host, talks to incusd over `/var/lib/incus/unix.socket`, and serves plain HTTP on port 5000.

- **Spec (binding):** `docs/superpowers/specs/2026-10-18-simple-incus-design.md`
- **Plan (13 tasks):** `docs/superpowers/plans/2026-10-18-simple-incus-v1.md`
- **Repo:** `git@github.com:awillfox/simple-incus.git`. All work happens on `main`.
- **Git identity:** set for this repo only, as `awillfox <natee.singkong@gmail.com>`.

## Owner rules (follow these)

- **Brainstorming:** always give the owner multiple-choice options, not open questions.
- **Commits:** conventional style, with **no AI co-author or "Generated with" lines**. This is also set globally in `~/.claude/settings.json` (`attribution`).
- **Push to GitHub after every task** once it passes review.
- **Desktop-first web UI.** Verify at 1440 and 1280 px. Mobile is best-effort only.
- **Look:** high-end, enterprise-grade, dark by default with a light toggle.
- **No login, no TLS, port 5000.** The official Incus HTTPS listener gets switched off during deploy.

## How the work is being run

The plan is executed with **subagent-driven development**:
- A fresh implementer does each task, then a reviewer checks spec and quality.
- Fix rounds resume the implementer, and a scoped re-review follows.
- A whole-branch review on the most capable model runs at the end.

The local ledger is `.superpowers/sdd/2026-10-18-simple-incus-v1/progress.md`. It is **git-ignored and only on the dev PC**. It holds per-task briefs, reports, review packages and screenshots (`screens/`). Its last line is still "Task 10: dispatched". If it's gone, use this file plus `git log`.

## Progress

| Task | Status | Commits |
|---|---|---|
| 1 Toolchain + scaffold + Linux cross-build | ✅ done | 84aa4c2 |
| 2 Incus transport + client core | ✅ done | 5baeed9..1ded3e7 |
| 3 Shared model + mapping + CPU math | ✅ done | fcea1ba |
| 4 Instance list/action server fns | ✅ done | 865a9f5..2366ea1 |
| 5 Event hub + `/ws/events` | ✅ done | 17361cd..3a70988 |
| 6 App shell + design system | ✅ done | f47221b |
| 7 Instances page (live, actions) | ✅ done | 62212dd..5a0b7d1 |
| 8 Instance detail: summary, snapshots, metrics | ✅ done | 91e420e |
| 9 Images (local + catalog) + Overview | ✅ done | a1bcf77..929dc8b |
| 10 Create instance (advanced) | 🔄 **uncommitted draft**; needs fixes, manual test, review | — |
| 11 WS proxy + web shell (xterm.js) | ⏳ todo | — |
| 12 Text console + VGA (SPICE) | ⏳ todo | — |
| 13 Deploy (systemd, deploy.ps1), README update, end-to-end check | ⏳ todo | — |
| Final whole-branch review + fix wave | ⏳ todo | — |

## Rulings made during execution (binding unless the owner overrides)

- **R1:** the Unix-socket transport is `#[cfg(unix)]`. On Windows, use `SI_INCUS=tcp://127.0.0.1:8444` over an SSH forward.
- **R2:** `CreateDrawerState { open, prefill }` context lives in `app.rs`. The Instances "Create" and Images "Launch" buttons set it, and Task 10 mounts `CreateDrawer`.
- **R3:** Task 13 does **not reboot** the host, since it also runs Docker and LXD. Use `systemctl restart incus simple-incus` and `systemctl is-enabled` instead.
- **R4:** CPU % must use `CpuTracker::sample_with_limit(name, usage, at, cpu_limit(&raw))`.
- **R5:** server logic lives in ssr-only `*_impl(&AppState)` functions. `#[server]` functions are thin wrappers via `expect_context`, and errors are mapped by `human()`.
- **R6:** UI pages live in `src/ui/pages/*.rs`, and shared row actions are in `src/ui/pages/actions.rs`.
- **R7:** in the catalog, an image is a container if it has `squashfs` or `root.tar.xz`, and a VM if it has `disk-kvm.img`. The plan's `incus.tar.xz` rule was wrong for the real data.
- **R8:** one shared, wasm-safe `validate_name` (1–63 chars, `[a-zA-Z0-9-]`, starts with a letter, no trailing `-`, not all digits). Create and snapshots both use it. Snapshot names ending in `-` are now rejected.

## Things Task 13 must handle (from earlier reviews)

- **Release mode:** `Cargo.toml` has `env = "DEV"`, and `AutoReload` injects a reload websocket. Release must run as PROD (`LEPTOS_ENV=PROD`).
- **Leptos config at runtime:** `get_configuration(None)` reads `Cargo.toml` when the server starts. The systemd unit must set `LEPTOS_SITE_ROOT`, `LEPTOS_OUTPUT_NAME`, `LEPTOS_SITE_ADDR` and related variables.
- **License field:** set `license = "MIT"` in `Cargo.toml`. The `LICENSE` file already exists.
- **README:** it already exists. **Update** it with the real deploy steps; don't recreate it.
- **Incus listener:** run `incus config unset core.https_address` on deploy. It's still `[::]:5000`, so the official UI is still live.
- **Dead config:** remove the `end2end-*` entries from `Cargo.toml`.

## Deferred minor findings (triage these in the final review)

- **DataTable / dialogs:** no focus trap in modal or drawer, Escape only works while the dialog has focus, and the closed mobile sidebar is still reachable with Tab.
- **Event stream:** wasm closures leak with `.forget()` on each reconnect. A server restart shows the "Incus unreachable" banner.
- **Instances page:** no "stale" indicator when a refetch fails with rows already loaded. `format_bytes(1048575)` prints "1024.0 KiB".
- **Instance detail:** `instance_detail.rs` is about 940 lines; split out `SnapshotsTab`. The constants `Busy`, `POLL_MS` and `DEBOUNCE_MS` are duplicated with `instances.rs`; move them to `actions.rs`.
- **Catalog:** no backoff or single-flight tests. `get()` holds its lock during a fetch of up to 20 s.
- **CPU tracking:** the host thread count is fixed at startup, and concurrent list calls share the `CpuTracker`.
- **Dependencies:** serde, thiserror and futures-util are non-optional, so they're compiled into wasm.
- **Clippy:** the 5 warnings listed in "Where we are".

The ledger has more small items, listed per task.

## Dev environment (Windows PC)

- **Toolchain (confirmed 2026-11-06):** Rust 1.99, cargo-leptos 0.3.11, and the wasm32 and x86_64-unknown-linux-gnu targets. Also cargo-zigbuild and zig 0.17 (from winget; `deploy/build.ps1` finds it), not rechecked.
- **Tests:** `cargo test --features ssr`. A plain `cargo test` fails with dozens of "unresolved crate" errors; that isn't a real failure.
- **Wasm check:** `cargo check --lib --target wasm32-unknown-unknown --no-default-features --features hydrate`.
- **Linux build:** `./deploy/build.ps1`.
- **Live dev:**
  ```sh
  ssh -N -L 127.0.0.1:8444:/var/lib/incus/unix.socket jemma@192.168.77.150
  SI_INCUS=tcp://127.0.0.1:8444 cargo leptos watch   # → http://localhost:5000
  ```
- **Screenshots:** headless Chrome at `"/c/Program Files/Google/Chrome/Application/chrome.exe" --headless=new --screenshot=out.png --window-size=1440,900 URL`.

## Target host

- **Host:** `jemma@192.168.77.150`, Ubuntu 24.04, bare metal with KVM.
- **Incus:** 7.5.1 from Zabbly (`incus` + `incus-ui-canonical`). Storage pool `default` (dir), bridge `incusbr0`, default project only.
- **Access:** SSH key login works from the dev PC, and `jemma` has passwordless sudo (`/etc/sudoers.d/90-jemma-nopasswd`).
- **Also running:** Docker and the LXD snap. The `incus-docker-fwd.service` unit lets `incusbr0` traffic through Docker's `FORWARD DROP`.
- **Test rule:** tests may only create or delete `si-test-*` instances.

State on 2026-11-06, checked read-only:

- **Instances:** none, so there are no `si-test-*` leftovers to clean up.
- **Images:** none. The cached Alpine 3.21 image `f17d529e9e66` the old handoff said to keep is gone, probably through Incus's automatic cache expiry (not confirmed). The first Alpine create will download it again.
- **Operations:** none. The old official-UI trust-token operation is no longer pending, so Task 13 has nothing to clean up there.
- **`simple-incus` service:** not installed yet, as expected before Task 13.
