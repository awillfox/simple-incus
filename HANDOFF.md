# HANDOFF — simple-incus v1

Snapshot rechecked 2026-11-06 after finishing the Task 10 fixes and live tests. Read this file first when picking the work back up.

## Where we are

**Tasks 1–9 are done. Task 10 (create instance) is fixed, reviewed, passes live tests on the real host, and is pushed to `feat/resume_task`, but not yet merged to `main`.** Tasks 11–13 and the final review haven't started.

### Git state: read this first

- On 2026-11-06 at 12:39, commit `76f059c` "some work" landed with the *original* Task 10 draft and an earlier `HANDOFF.md`. `origin/main` points at it, so **the unreviewed draft is already on `main`**.
- The checkout then moved to a new branch, **`feat/resume_task`**, at the same commit. The Task 10 fixes are committed on top of it as `feat: advanced create instance` and pushed to that branch.
- This replaces "all work happens on `main`" until the owner says otherwise. The owner merges to `main`; **`origin/main` lacks the fixes until then**. Don't push to `main`.
- A Task 10 review must cover `git diff 0d73b04`, not plain `git diff`, or it misses the draft.

### Verified on 2026-11-06

| Check | Result |
|---|---|
| `cargo test --features ssr` | ✅ 109 passed, 0 failed |
| Wasm check (hydrate) | ✅ Passes |
| `cargo clippy --features ssr --all-targets` | 5 warnings, none in Task 10 code (`data_table.rs`, `server/images.rs`, `server/snapshots.rs`) |
| Live create: container (`alpine/3.24`) | ✅ Live progress, reached Running in about 8 s |
| Live create: VM (`ubuntu/24.04`, 2 CPU, 2048 MiB, 20 GiB) | ✅ Reached Running; limits and disk size confirmed on the host |
| Live create: network, pool, cloud-init, start unticked | ✅ Stayed Stopped; config and devices confirmed on the host |
| Operation failure (VM on a 1 GiB disk) | ✅ Sticky error toast with the Incus message; nothing left behind |
| Duplicate and invalid names, no profile selected | ✅ Clear message shown in the form |
| Images "Launch" with a local image | ✅ Image preselected; type locked to what it supports |
| Tunnel cut for 25 s mid-create | ✅ Toast showed "Created" 0.6 s after the tunnel returned |
| Final event dropped, socket healthy (simulated in the browser) | ✅ Polling finished the create |
| Busy toast closed, then the create fails | ✅ Error toast still appears |
| Cancel during a slow request, then reopen | ✅ Second form stays open with its input |
| Options fail to load (tunnel down) | ✅ Error shown in the form |
| Layout: 1440 and 1280 px, dark and light | ✅ Screenshots in the ledger's `screens/task10-*` |

Local images weren't used for a full create, only for the prefill and type lock. Live tests ran in headless Chrome driven by a throwaway script, since the Chrome extension wasn't connected.

### What changed in the fix round

- **Stale image:** Instances "Create instance" now clears the preselected image. Reproduced in the browser before the fix.
- **Lost final event:** new `create_status` server fn; `CreateDrawer` polls it every 5 s while a create is tracked, because events can be missed through an outage or a lagging browser. If Incus has forgotten the operation, the instance settles it: it must exist and, if a start was requested, be Running.
- **Profiles:** selecting none is now blocked with a visible message. This was a judgment call; the alternative is sending an empty list.
- **Disk size with no root pool:** now rejected with a clear 400 before Incus is called.
- **Footer:** the sticky footer left a 20 px gap where content showed through. Found only in the browser.
- **Toast:** `toast_set_text` read its signal with tracking on, so the event Effect subscribed by accident. Found through a console warning.
- **Tests:** `ev.status` is now asserted.
- **From review:** closing the busy toast no longer hides the result; a late response only closes the form that sent it; a failure to load form options is now shown.

### Review findings not acted on

The review ran on `git diff 0d73b04`.
- **`default` profile could be missing:** rejected. Incus doesn't allow deleting or renaming it, as far as I know; not tested on the host.
- **Stricter snapshot names:** rejected. This is ruling R8.
- **Deferred:** see "Deferred minor findings".

## Next steps

1. Ask the owner whether `feat/resume_task` has been merged, and which branch later tasks should use (see "Git state").
2. Update the ledger's last line, which still says "Task 10: dispatched".
3. Run **Task 11** (web shell) and **Task 12** (consoles) through the same implement, review and push loop.
4. Run **Task 13** (deploy). It runs on the real host and turns off the official Incus HTTPS listener.
5. Run the final whole-branch review on the most capable model, then do one fix wave.
6. Report the rulings to the owner, then use `superpowers:finishing-a-development-branch`.

## What this project is

A no-login, no-certificate web UI for a single Incus host, written in Rust (Leptos 0.8 SSR + hydrate, axum).
It runs on the Incus host, talks to incusd over `/var/lib/incus/unix.socket`, and serves plain HTTP on port 5000.

- **Spec (binding):** `docs/superpowers/specs/2026-10-18-simple-incus-design.md`
- **Plan (13 tasks):** `docs/superpowers/plans/2026-10-18-simple-incus-v1.md`
- **Repo:** `git@github.com:awillfox/simple-incus.git`. Current branch: `feat/resume_task` (see "Git state").
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
| 10 Create instance (advanced) | ✅ **reviewed and live-tested**; on `feat/resume_task`, not merged | 76f059c (draft) + `feat: advanced create instance` |
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
- **Instances table:** 2 px of horizontal overflow at 1280 px, so a scrollbar shows; row actions stay visible.
- **Create form:** opening it fetches local images and the catalog whichever tab is shown, and "Launch" fetches them again; a failed Launch prefill shows no error.
- **Create code:** Incus status strings are mapped in both `ui/pages/create.rs` and `server/create.rs`; the `default` profile fallback in `server/create.rs` is now unreachable from the UI; the all-digits check in `validate_name` can never fire.

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

State on 2026-11-06, after cleanup:

- **Instances and images:** none. All `si-test-*` instances and images from the tests were deleted, so the first create re-downloads its image. The Alpine image `f17d529e9e66` the old handoff said to keep was already gone, and the catalog now offers Alpine 3.24, not 3.21.
- **Operations:** none. The old official-UI trust-token operation is no longer pending.
- **Official Incus UI:** still live, with `core.https_address` at `[::]:5000` until Task 13.
- **`simple-incus` service:** not installed yet, as expected before Task 13.
