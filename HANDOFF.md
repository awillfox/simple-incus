# HANDOFF — simple-incus v1

Snapshot taken 2026-10-18. Read this file first when picking the work back up.

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

The local ledger is `.superpowers/sdd/2026-10-18-simple-incus-v1/progress.md`. It is **git-ignored and only on the dev PC**. It holds per-task briefs, reports, review packages and screenshots (`screens/`). If it's gone, use this file plus `git log`.

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
| 10 Create instance (advanced) | 🔄 **in progress** (uncommitted work in tree) | — |
| 11 WS proxy + web shell (xterm.js) | ⏳ todo | — |
| 12 Text console + VGA (SPICE) | ⏳ todo | — |
| 13 Deploy (systemd, deploy.ps1), README update, end-to-end check | ⏳ todo | — |
| Final whole-branch review + fix wave | ⏳ todo | — |

The last pushed commit is `929dc8b`.

### Resuming Task 10

When this handoff was written, a Task 10 implementer was mid-work, with uncommitted changes in `src/` (drawer, create form, validation, events).

1. Run `git status`. If Task 10 is committed (`feat: advanced create instance`), review it against `docs/superpowers/plans/...` Task 10 plus the rulings below. Otherwise either finish it from the working tree or `git stash` it and redo the task.
2. Check the server for leftovers: `ssh jemma@192.168.77.150 'incus list; incus image list'`. Delete any `si-test-*` instance and any image those tests pulled, but **keep** the cached Alpine 3.21 image `f17d529e9e66`.

## Rulings made during execution (binding unless the owner overrides)

- **R1:** the Unix-socket transport is `#[cfg(unix)]`. On Windows, use `SI_INCUS=tcp://127.0.0.1:8444` over an SSH forward.
- **R2:** `CreateDrawerState { open, prefill }` context lives in `app.rs`. The Instances "Create" and Images "Launch" buttons set it, and Task 10 mounts `CreateDrawer`.
- **R3:** Task 13 does **not reboot** the host, since it also runs Docker and LXD. Use `systemctl restart incus simple-incus` and `systemctl is-enabled` instead.
- **R4:** CPU % must use `CpuTracker::sample_with_limit(name, usage, at, cpu_limit(&raw))`.
- **R5:** server logic lives in ssr-only `*_impl(&AppState)` functions. `#[server]` functions are thin wrappers via `expect_context`, and errors are mapped by `human()`.
- **R6:** UI pages live in `src/ui/pages/*.rs`, and shared row actions are in `src/ui/pages/actions.rs`.
- **R7:** in the catalog, an image is a container if it has `squashfs` or `root.tar.xz`, and a VM if it has `disk-kvm.img`. The plan's `incus.tar.xz` rule was wrong for the real data.
- **R8:** one shared, wasm-safe `validate_name` (1–63 chars, `[a-zA-Z0-9-]`, starts with a letter, no trailing `-`, not all digits). Create and snapshots both use it.

## Things Task 13 must handle (from earlier reviews)

- **Release mode:** `Cargo.toml` has `env = "DEV"`, and `AutoReload` injects a reload websocket. Release must run as PROD (`LEPTOS_ENV=PROD`).
- **Leptos config at runtime:** `get_configuration(None)` reads `Cargo.toml` when the server starts. The systemd unit must set `LEPTOS_SITE_ROOT`, `LEPTOS_OUTPUT_NAME`, `LEPTOS_SITE_ADDR` and related variables.
- **License field:** set `license = "MIT"` in `Cargo.toml`. The `LICENSE` file already exists.
- **README:** it already exists. **Update** it with the real deploy steps; don't recreate it.
- **Incus listener:** run `incus config unset core.https_address` on deploy. It's currently `[::]:5000`, so the official UI is still live. An old official-UI trust-token operation is also still pending on the host.
- **Dead config:** remove the `end2end-*` entries from `Cargo.toml`.

## Deferred minor findings (triage these in the final review)

- **DataTable / dialogs:** no focus trap in modal or drawer, Escape only works while the dialog has focus, and the closed mobile sidebar is still reachable with Tab.
- **Event stream:** wasm closures leak with `.forget()` on each reconnect. A server restart shows the "Incus unreachable" banner.
- **Instances page:** no "stale" indicator when a refetch fails with rows already loaded. `format_bytes(1048575)` prints "1024.0 KiB".
- **Instance detail:** `instance_detail.rs` is about 940 lines; split out `SnapshotsTab`. The constants `Busy`, `POLL_MS` and `DEBOUNCE_MS` are duplicated with `instances.rs`; move them to `actions.rs`.
- **Catalog:** no backoff or single-flight tests. `get()` holds its lock during a fetch of up to 20 s.
- **CPU tracking:** the host thread count is fixed at startup, and concurrent list calls share the `CpuTracker`.
- **Dependencies:** serde, thiserror and futures-util are non-optional, so they're compiled into wasm.

## Dev environment (Windows PC)

- **Toolchain:** Rust 1.99, cargo-leptos 0.3.11, cargo-zigbuild, and zig 0.17 (from winget; `deploy/build.ps1` finds it), plus the wasm32 and x86_64-unknown-linux-gnu targets.
- **Tests:** `cargo test --features ssr`.
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

## Next steps

1. Finish or review Task 10, then push.
2. Run Task 11 (web shell) and Task 12 (consoles) through the same implement, review and push loop.
3. Run Task 13 (deploy). The deploy runs on the real host and turns off the official Incus HTTPS listener.
4. Run the final whole-branch review on the most capable model, then do one fix wave.
5. Report the rulings to the owner, then use `superpowers:finishing-a-development-branch`.
