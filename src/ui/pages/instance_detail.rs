//! Instance detail page: header with lifecycle actions and the tabs
//! Summary, Terminal, Console, Snapshots and Metrics.
//!
//! [`InstancePage`] owns the data (one resource for the instance, refetched on
//! lifecycle events, on reconnect and every few seconds while the tab is
//! visible); the tab bodies are plain components fed with signals so they can
//! be rendered in tests without server functions or a router.
use std::sync::Arc;

use leptos::ev::SubmitEvent;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_meta::Title;
use leptos_router::hooks::{use_navigate, use_params_map};

use super::actions::{after_ms, err_msg, perform, ActionConfirm, RowAction};
#[cfg(feature = "hydrate")]
use super::actions::page_visible;
use super::instance_metrics::{push_sample, MetricsTab, Sample};
use super::PageFrame;
use crate::model::{InstanceDetail, InstanceKind, InstanceStatus, SnapshotInfo, UiEventKind};
use crate::server::instances::get_instance;
use crate::server::snapshots::{create_snapshot, delete_snapshot, list_snapshots, restore_snapshot};
use crate::ui::components::{
    toast, Column, ConfirmModal, DataTable, EmptyState, Icon, IconName, Meter, Skeleton, StatusPill, Tabs, ToastKind,
};
use crate::ui::console::ConsoleTab;
use crate::ui::events::{use_events, use_incus_connected};
use crate::ui::format::{created_label, format_bytes, now_ms, parse_rfc3339};
use crate::ui::terminal::TerminalTab;

/// Background refresh while the tab is visible.
#[cfg(feature = "hydrate")]
const POLL_MS: u64 = 5000;
/// Lifecycle events arrive in bursts; refetch once after they settle.
const DEBOUNCE_MS: u64 = 300;
/// The header shows a spinner at most this long after a successful action.
const BUSY_MAX_MS: u64 = 10_000;

/// Tab slugs and labels, in display order.
pub const TABS: [(&str, &str); 5] = [
    ("summary", "Summary"),
    ("terminal", "Terminal"),
    ("console", "Console"),
    ("snapshots", "Snapshots"),
    ("metrics", "Metrics"),
];

/// The tab for a URL segment; anything unknown (or missing) is the summary.
pub fn tab_slug(raw: &str) -> &'static str {
    TABS.iter().map(|t| t.0).find(|s| *s == raw).unwrap_or("summary")
}

fn kind_label(kind: InstanceKind) -> &'static str {
    match kind {
        InstanceKind::Container => "Container",
        InstanceKind::Vm => "Virtual machine",
    }
}

// ---------------------------------------------------------------- summary

fn kv(label: &'static str, value: impl IntoView + 'static) -> impl IntoView {
    view! {
        <div class="min-w-0">
            <dt class="text-[12px] text-muted">{label}</dt>
            <dd class="mt-1 min-w-0 break-words text-[13px] text-text">{value}</dd>
        </div>
    }
}

fn dash() -> AnyView {
    view! { <span class="text-muted">"\u{2014}"</span> }.into_any()
}

fn address_list(addrs: &[String]) -> AnyView {
    if addrs.is_empty() {
        return dash();
    }
    view! {
        <ul class="space-y-0.5">
            {addrs.iter().map(|a| view! { <li class="mono text-text-2">{a.clone()}</li> }).collect_view()}
        </ul>
    }
    .into_any()
}

fn stat_row(label: &'static str, value: String) -> impl IntoView {
    view! {
        <div class="flex items-baseline justify-between gap-3 text-[13px]">
            <span class="text-muted">{label}</span>
            <span class="num font-medium text-text-2">{value}</span>
        </div>
    }
}

/// Detail without the numbers that tick on every poll, so the static panels
/// re-render (and lose a text selection) only when something real changes.
fn without_live_numbers(d: &InstanceDetail) -> InstanceDetail {
    let mut d = d.clone();
    d.summary.cpu_pct = None;
    d.summary.mem_used = 0;
    d.summary.disk_used = 0;
    d.summary.net_rx = 0;
    d.summary.net_tx = 0;
    d
}

#[component]
pub fn SummaryTab(#[prop(into)] detail: Signal<Option<InstanceDetail>>) -> impl IntoView {
    let fixed = Memo::new(move |_| detail.with(|d| d.as_ref().map(without_live_numbers)));
    let live = move || {
        detail.with(|d| {
            d.as_ref().map(|d| {
                let s = &d.summary;
                let running = s.status == InstanceStatus::Running;
                let cpu = match (running, s.cpu_pct) {
                    (true, Some(p)) => view! { <Meter value=f64::from(p) max=100.0 label="CPU"/> }.into_any(),
                    _ => stat_row("CPU", "\u{2014}".into()).into_any(),
                };
                let mem = match s.mem_limit.filter(|&l| l > 0) {
                    Some(limit) => view! {
                        <Meter
                            value=s.mem_used as f64
                            max=limit as f64
                            label=format!("Memory {} / {}", format_bytes(s.mem_used), format_bytes(limit))
                        />
                    }
                    .into_any(),
                    None => stat_row("Memory", format_bytes(s.mem_used)).into_any(),
                };
                view! {
                    <div class="space-y-4">
                        {cpu} {mem}
                        <div class="space-y-2 border-t border-border pt-4">
                            {stat_row("Disk used", if s.disk_used == 0 { "\u{2014}".into() } else { format_bytes(s.disk_used) })}
                            {stat_row("Network received", format_bytes(s.net_rx))}
                            {stat_row("Network sent", format_bytes(s.net_tx))}
                        </div>
                    </div>
                }
            })
        })
    };
    let details = move || {
        fixed.get().map(|d| {
            let s = d.summary;
            let profiles = if s.profiles.is_empty() {
                dash()
            } else {
                view! {
                    <span class="flex flex-wrap gap-1.5">
                        {s
                            .profiles
                            .iter()
                            .map(|p| {
                                view! {
                                    <span class="rounded border border-border-strong bg-surface-2 px-1.5 py-px text-[12px] text-text-2">
                                        {p.clone()}
                                    </span>
                                }
                            })
                            .collect_view()}
                    </span>
                }
                .into_any()
            };
            let image = if s.image_desc.is_empty() { dash() } else { view! { <span>{s.image_desc.clone()}</span> }.into_any() };
            view! {
                <dl class="grid grid-cols-2 gap-x-6 gap-y-5">
                    {kv("Type", kind_label(s.kind))}
                    {kv("Image", image)}
                    {kv("IPv4", address_list(&s.ipv4))}
                    {kv("IPv6", address_list(&s.ipv6))}
                    {kv("Created", view! { <span title=s.created_at.clone()>{created_label(&s.created_at)}</span> })}
                    {kv("Profiles", profiles)}
                </dl>
            }
        })
    };
    let devices = move || {
        fixed.get().map(|d| {
            if d.devices.is_empty() {
                return view! { <p class="px-4 py-3 text-[13px] text-muted">"No devices."</p> }.into_any();
            }
            view! {
                <ul class="divide-y divide-border">
                    {d
                        .devices
                        .into_iter()
                        .map(|x| {
                            view! {
                                <li class="flex items-baseline gap-3 px-4 py-2.5 text-[13px]">
                                    <span class="mono w-24 shrink-0 truncate text-text">{x.name}</span>
                                    <span class="w-14 shrink-0 text-muted">{x.kind}</span>
                                    <span class="mono min-w-0 flex-1 truncate text-text-2">{x.detail}</span>
                                </li>
                            }
                        })
                        .collect_view()}
                </ul>
            }
            .into_any()
        })
    };
    let config = move || {
        fixed.get().map(|d| {
            if d.config.is_empty() {
                return view! { <p class="px-4 py-3 text-[13px] text-muted">"No limits, image or cloud-init settings."</p> }
                    .into_any();
            }
            view! {
                <ul class="divide-y divide-border">
                    {d
                        .config
                        .into_iter()
                        .map(|(k, v)| {
                            view! {
                                <li class="flex items-start gap-3 px-4 py-2.5 text-[13px]">
                                    <span class="mono w-56 shrink-0 break-all text-text">{k}</span>
                                    <pre class="mono m-0 max-h-28 min-w-0 flex-1 overflow-auto whitespace-pre-wrap break-all text-text-2">
                                        {v}
                                    </pre>
                                </li>
                            }
                        })
                        .collect_view()}
                </ul>
            }
            .into_any()
        })
    };
    view! {
        <div class="grid grid-cols-1 gap-4 lg:grid-cols-3">
            <section class="panel p-5 lg:col-span-2">
                <h2 class="mb-4 text-[13px] font-semibold text-text">"Details"</h2>
                {details}
            </section>
            <section class="panel p-5">
                <h2 class="mb-4 text-[13px] font-semibold text-text">"Resources"</h2>
                {live}
            </section>
            <section class="panel overflow-hidden lg:col-span-3">
                <h2 class="border-b border-border px-4 py-3 text-[13px] font-semibold text-text">"Devices"</h2>
                {devices}
            </section>
            <section class="panel overflow-hidden lg:col-span-3">
                <h2 class="border-b border-border px-4 py-3 text-[13px] font-semibold text-text">"Configuration"</h2>
                {config}
            </section>
        </div>
    }
}

// ---------------------------------------------------------------- snapshots

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SnapAction {
    Restore,
    Delete,
}

/// Reached by the snapshot row actions (column renderers are plain `fn`s).
#[derive(Clone, Copy)]
struct SnapCtx {
    working: RwSignal<Option<String>>,
    request: Callback<(String, SnapAction)>,
}

fn snap_actions_cell(r: &SnapshotInfo) -> AnyView {
    let ctx = use_context::<SnapCtx>();
    let disabled = move || ctx.is_some_and(|c| c.working.with(Option::is_some));
    let button = |action: SnapAction, label: &'static str, icon: IconName, tone: &'static str| {
        let name = r.name.clone();
        view! {
            <button
                type="button"
                title=label
                aria-label=format!("{label} snapshot {}", r.name)
                disabled=disabled
                class=format!(
                    "inline-flex size-7 items-center justify-center rounded-md text-muted transition-colors hover:bg-hover focus-visible:bg-hover disabled:cursor-not-allowed disabled:opacity-40 disabled:hover:bg-transparent {tone}"
                )
                on:click=move |_| {
                    if let Some(c) = ctx {
                        c.request.run((name.clone(), action));
                    }
                }
            >
                <Icon name=icon class="size-3.5"/>
            </button>
        }
    };
    view! {
        <div class="flex items-center justify-end">
            {button(SnapAction::Restore, "Restore", IconName::Restart, "hover:text-text")}
            {button(SnapAction::Delete, "Delete", IconName::Trash, "hover:text-danger")}
        </div>
    }
    .into_any()
}

fn snapshot_columns() -> Vec<Column<SnapshotInfo>> {
    vec![
        Column::new("Name", |r: &SnapshotInfo| view! { <span class="mono text-[13px] font-medium text-text">{r.name.clone()}</span> }.into_any())
            .sortable(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
        Column::new("Created", |r: &SnapshotInfo| {
            view! { <span class="text-text-2" title=r.created_at.clone()>{created_label(&r.created_at)}</span> }.into_any()
        })
        .sortable(|a, b| parse_rfc3339(&a.created_at).cmp(&parse_rfc3339(&b.created_at))),
        Column::new("Type", |r: &SnapshotInfo| {
            view! { <span class="text-text-2">{if r.stateful { "Stateful" } else { "Disk only" }}</span> }.into_any()
        }),
        Column::new("", snap_actions_cell).numeric(),
    ]
}

/// Snapshot list with create, restore and delete.
#[component]
pub fn SnapshotsTab(
    #[prop(into)] name: Signal<String>,
    /// Bumped by the page to refetch (polls, events); bumped here after changes.
    tick: RwSignal<u32>,
) -> impl IntoView {
    let resource = Resource::new(move || (name.get(), tick.get()), |(n, _)| list_snapshots(n));
    let rows = RwSignal::new(Vec::<SnapshotInfo>::new());
    let error = RwSignal::new(None::<String>);
    let loaded = RwSignal::new(false);
    let working = RwSignal::new(None::<String>);
    let new_name = RwSignal::new(String::new());
    let pending = RwSignal::new(None::<(String, SnapAction)>);

    Effect::new(move |_| match resource.get() {
        Some(Ok(v)) => {
            if rows.with_untracked(|r| *r != v) {
                rows.set(v);
            }
            error.set(None);
            loaded.set(true);
        }
        Some(Err(e)) => {
            error.set(Some(err_msg(&e)));
            loaded.set(true);
        }
        None => {}
    });

    let refresh = move || {
        tick.try_update(|t| *t = t.wrapping_add(1));
    };
    // Runs one snapshot operation; `done` is the success toast.
    let run = move |busy: String, done: String, op: std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ServerFnError>>>>| {
        working.set(Some(busy));
        spawn_local(async move {
            let result = op.await;
            working.try_set(None);
            match result {
                Ok(()) => {
                    toast(ToastKind::Success, done);
                    refresh();
                }
                Err(e) => toast(ToastKind::Error, err_msg(&e)),
            }
        });
    };

    let create = move |ev: SubmitEvent| {
        ev.prevent_default();
        if working.with_untracked(Option::is_some) {
            return;
        }
        let snap = new_name.get_untracked();
        let n = name.get_untracked();
        working.set(Some("Creating snapshot\u{2026}".into()));
        spawn_local(async move {
            let result = create_snapshot(n, snap).await;
            working.try_set(None);
            match result {
                Ok(()) => {
                    toast(ToastKind::Success, "Snapshot created");
                    new_name.try_set(String::new());
                    refresh();
                }
                Err(e) => toast(ToastKind::Error, err_msg(&e)),
            }
        });
    };

    let request = Callback::new(move |(snap, action): (String, SnapAction)| pending.set(Some((snap, action))));
    provide_context(SnapCtx { working, request });

    let confirm = move || {
        pending.get().map(|(snap, action)| {
            let open = RwSignal::new(true);
            Effect::new(move |_| {
                if !open.get() {
                    pending.set(None);
                }
            });
            let inst = name.get_untracked();
            let (title, body, label) = match action {
                SnapAction::Restore => (
                    format!("Restore {inst} to {snap}?"),
                    "The current state of the instance will be lost and replaced by this snapshot. \
                     Create a snapshot first if you may want the current state back."
                        .to_string(),
                    "Restore",
                ),
                SnapAction::Delete => (
                    format!("Delete snapshot {snap}?"),
                    "This permanently removes the snapshot. This cannot be undone.".to_string(),
                    "Delete",
                ),
            };
            view! {
                <ConfirmModal
                    open=open
                    title=title
                    body=body
                    confirm_label=label
                    danger=true
                    on_confirm=Callback::new(move |_| {
                        let (n, s) = (inst.clone(), snap.clone());
                        match action {
                            SnapAction::Restore => {
                                let done = format!("Restored {n} to {s}");
                                run(
                                    format!("Restoring {s}\u{2026}"),
                                    done,
                                    Box::pin(async move { restore_snapshot(n, s).await }),
                                )
                            }
                            SnapAction::Delete => {
                                let done = format!("Deleted snapshot {s}");
                                run(
                                    format!("Deleting {s}\u{2026}"),
                                    done,
                                    Box::pin(async move { delete_snapshot(n, s).await }),
                                )
                            }
                        }
                    })
                />
            }
        })
    };

    let list: Signal<Vec<SnapshotInfo>> = rows.into();
    let loading = Signal::derive(move || !loaded.get());
    let show_error = Memo::new(move |_| rows.with(|r| r.is_empty()) && error.with(Option::is_some));
    view! {
        <div class="space-y-4">
            <form class="panel flex flex-wrap items-center gap-3 p-3" on:submit=create>
                <label class="sr-only" for="snapshot-name">"Snapshot name"</label>
                <input
                    id="snapshot-name"
                    class="input w-72 max-w-full"
                    type="text"
                    autocomplete="off"
                    spellcheck="false"
                    placeholder="Name (optional, e.g. before-upgrade)"
                    prop:value=move || new_name.get()
                    on:input=move |e| new_name.set(event_target_value(&e))
                />
                <button type="submit" class="btn btn-primary" disabled=move || working.with(Option::is_some)>
                    <Icon name=IconName::Camera/>
                    "Create snapshot"
                </button>
                <span class="text-[12px] text-muted">"Leave the name empty to use the date and time."</span>
                {move || {
                    working
                        .get()
                        .map(|w| {
                            view! {
                                <span class="ml-auto flex items-center gap-2 text-[13px] text-text-2" role="status">
                                    <Icon name=IconName::Spinner class="size-3.5 animate-spin text-accent"/>
                                    {w}
                                </span>
                            }
                        })
                }}
            </form>
            {move || {
                if show_error.get() {
                    view! {
                        <EmptyState icon=IconName::AlertCircle title="Can't load snapshots" body=error.get().unwrap_or_default()>
                            <button type="button" class="btn" on:click=move |_| refresh()>
                                "Retry"
                            </button>
                        </EmptyState>
                    }
                    .into_any()
                } else {
                    view! {
                        <DataTable
                            rows=list
                            columns=snapshot_columns()
                            row_key=|r: &SnapshotInfo| r.name.clone()
                            search=|r: &SnapshotInfo| r.name.clone()
                            placeholder="Filter snapshots\u{2026}"
                            empty="No snapshots yet. Create one to capture the current state."
                            loading=loading
                        />
                    }
                    .into_any()
                }
            }}
            {confirm}
        </div>
    }
}

// ---------------------------------------------------------------- page

/// An action in flight on the instance.
#[derive(Debug, Clone, PartialEq)]
struct Busy {
    /// Status when the action started; done once it changes.
    status: InstanceStatus,
    generation: u64,
}

#[component]
fn NotFound(#[prop(into)] name: Signal<String>) -> impl IntoView {
    view! {
        <EmptyState
            icon=IconName::AlertCircle
            title="Instance not found"
            body=format!("There is no instance named \"{}\" on this host. It may have been deleted.", name.get_untracked())
        >
            <a href="/instances" class="btn">
                <Icon name=IconName::ArrowLeft/>
                "Back to Instances"
            </a>
        </EmptyState>
    }
}

#[component]
fn Loading() -> impl IntoView {
    view! {
        <div class="grid grid-cols-1 gap-4 lg:grid-cols-3" aria-busy="true" aria-label="Loading instance">
            <div class="panel space-y-3 p-5 lg:col-span-2">
                <Skeleton class="h-3 w-24"/>
                <Skeleton class="h-3 w-64"/>
                <Skeleton class="h-3 w-48"/>
            </div>
            <div class="panel space-y-3 p-5">
                <Skeleton class="h-3 w-24"/>
                <Skeleton class="h-3 w-full"/>
            </div>
        </div>
    }
}

#[component]
pub fn InstancePage() -> impl IntoView {
    let params = use_params_map();
    let name = Memo::new(move |_| params.with(|p| p.get("name").unwrap_or_default()));
    let tab = Memo::new(move |_| params.with(|p| tab_slug(p.get("tab").as_deref().unwrap_or(""))));

    let tick = RwSignal::new(0u32);
    let resource = Resource::new(move || (name.get(), tick.get()), |(n, _)| get_instance(n));
    let detail = RwSignal::new(None::<InstanceDetail>);
    let samples = RwSignal::new(Vec::<Sample>::new());
    let missing = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let loaded = RwSignal::new(false);
    let busy = RwSignal::new(None::<Busy>);
    let next_generation = StoredValue::new(0u64);
    let pending = RwSignal::new(None::<(String, RowAction)>);
    // Set while this page deletes the instance itself (no "deleted" toast then).
    let deleting = RwSignal::new(false);
    let left = RwSignal::new(false);

    // Timers and async continuations can outlive the page; every touch of page
    // state from them goes through the `try_*` accessors (None once disposed).
    let refetch = move || {
        tick.try_update(|t| *t = t.wrapping_add(1));
    };
    let status = Memo::new(move |_| detail.with(|d| d.as_ref().map(|d| d.summary.status.clone())));
    let has_detail = Memo::new(move |_| detail.with(Option::is_some));

    // A different instance (same route, new :name) starts from scratch.
    Effect::new(move |prev: Option<String>| {
        let n = name.get();
        if prev.is_some_and(|p| p != n) {
            detail.set(None);
            samples.set(Vec::new());
            missing.set(false);
            error.set(None);
            loaded.set(false);
            busy.set(None);
            deleting.set(false);
        }
        n
    });

    // Keep the last good data on screen when a refetch fails.
    Effect::new(move |_| match resource.get() {
        Some(Ok(Some(d))) => {
            if d.summary.name != name.get_untracked() {
                return;
            }
            if d.summary.status == InstanceStatus::Running {
                samples.update(|b| {
                    push_sample(b, Sample::of(&d, now_ms()));
                });
            } else if samples.with_untracked(|b| !b.is_empty()) {
                samples.set(Vec::new());
            }
            // The action is done once the status moved.
            busy.update(|b| {
                if b.as_ref().is_some_and(|x| x.status != d.summary.status) {
                    *b = None;
                }
            });
            if detail.with_untracked(|c| c.as_ref() != Some(&d)) {
                detail.set(Some(d));
            }
            if missing.get_untracked() {
                missing.set(false);
            }
            error.set(None);
            loaded.set(true);
        }
        Some(Ok(None)) => {
            detail.set(None);
            missing.set(true);
            error.set(None);
            loaded.set(true);
        }
        Some(Err(e)) => {
            error.set(Some(err_msg(&e)));
            loaded.set(true);
        }
        None => {}
    });

    // Refetch: debounced on this instance's Lifecycle events ...
    let events = use_events();
    let generation = StoredValue::new(0u32);
    Effect::new(move |prev: Option<()>| {
        let ev = events.get();
        // The signal keeps the last event of an earlier page; only react to new ones.
        let Some(ev) = ev.filter(|_| prev.is_some()) else { return };
        if ev.kind != UiEventKind::Lifecycle || ev.instance.as_deref() != Some(name.get_untracked().as_str()) {
            return;
        }
        if ev.message == "instance-deleted" {
            if !deleting.get_untracked() && detail.with_untracked(Option::is_some) {
                toast(ToastKind::Info, format!("Instance {} was deleted", name.get_untracked()));
            }
            detail.set(None);
            samples.set(Vec::new());
            missing.set(true);
            return;
        }
        let Some(g) = generation.try_get_value().map(|g| g.wrapping_add(1)) else { return };
        generation.try_set_value(g);
        after_ms(DEBOUNCE_MS, move || {
            if generation.try_get_value() == Some(g) {
                refetch();
            }
        });
    });
    // ... immediately after Incus (or our socket) comes back (events are not replayed) ...
    let connected = use_incus_connected();
    Effect::new(move |was: Option<bool>| {
        let now = connected.get();
        if was == Some(false) && now {
            refetch();
        }
        now
    });
    // ... and every few seconds while the tab is visible.
    #[cfg(feature = "hydrate")]
    if let Ok(handle) = set_interval_with_handle(
        move || {
            if page_visible() {
                refetch();
            }
        },
        std::time::Duration::from_millis(POLL_MS),
    ) {
        on_cleanup(move || handle.clear());
    }

    let run = move |inst: String, action: RowAction, force: bool| {
        let generation = next_generation.try_get_value().unwrap_or(0).wrapping_add(1);
        next_generation.try_set_value(generation);
        let before = status.get_untracked().unwrap_or(InstanceStatus::Stopped);
        busy.set(Some(Busy { status: before, generation }));
        if action == RowAction::Delete {
            deleting.set(true);
        }
        spawn_local(async move {
            let result = perform(&inst, action, force).await;
            // Clears the flag only if it is still the one this action set.
            let unbusy = move || {
                busy.try_update(|b| {
                    if b.as_ref().is_some_and(|x| x.generation == generation) {
                        *b = None;
                    }
                });
            };
            match result {
                Ok(()) => {
                    toast(ToastKind::Success, format!("Instance {inst} {}", action.verb()));
                    if action == RowAction::Delete {
                        unbusy();
                        left.try_set(true);
                    } else {
                        refetch();
                        // Normally cleared earlier, when the refetched status changes.
                        after_ms(BUSY_MAX_MS, unbusy);
                    }
                }
                Err(e) => {
                    unbusy();
                    deleting.try_set(false);
                    toast(ToastKind::Error, err_msg(&e));
                }
            }
        });
    };
    let request = move |action: RowAction| {
        let inst = name.get_untracked();
        if action.needs_confirm() {
            pending.set(Some((inst, action)));
        } else {
            run(inst, action, false);
        }
    };
    let confirm_run = Callback::new(move |(inst, action, force): (String, RowAction, bool)| run(inst, action, force));

    // After deleting this instance, go back to the list.
    let navigate = use_navigate();
    Effect::new(move |_| {
        if left.get() {
            navigate("/instances", Default::default());
        }
    });

    let action_buttons = move || {
        status.get().map(|st| {
            RowAction::available(&st)
                .iter()
                .map(|&action| {
                    let tone = if action == RowAction::Delete { "text-danger" } else { "" };
                    view! {
                        <button
                            type="button"
                            class=format!("btn {tone}")
                            disabled=move || busy.with(Option::is_some)
                            on:click=move |_| request(action)
                        >
                            <Icon name=action.icon()/>
                            {action.label()}
                        </button>
                    }
                })
                .collect_view()
        })
    };
    let spinner = move || {
        busy.with(Option::is_some).then(|| {
            view! {
                <span class="text-accent" role="status" aria-label="Working">
                    <Icon name=IconName::Spinner class="size-4 animate-spin"/>
                </span>
            }
        })
    };

    let tabs: Vec<(String, String)> = TABS.iter().map(|(s, l)| (s.to_string(), l.to_string())).collect();
    let base = Signal::derive(move || format!("/instances/{}", name.get()));
    let active = Signal::derive(move || tab.get().to_string());
    let name_signal: Signal<String> = name.into();
    let detail_signal: Signal<Option<InstanceDetail>> = detail.into();
    let samples_signal: Signal<Vec<Sample>> = samples.into();

    let body = move || {
        let n = name.get();
        match tab.get() {
            "terminal" => view! { <TerminalTab name=n/> }.into_any(),
            "console" => {
                let kind = detail.with_untracked(|d| d.as_ref().map(|d| d.summary.kind)).unwrap_or(InstanceKind::Container);
                view! { <ConsoleTab name=n kind=kind/> }.into_any()
            }
            "snapshots" => view! { <SnapshotsTab name=name_signal tick=tick/> }.into_any(),
            "metrics" => view! { <MetricsTab detail=detail_signal samples=samples_signal/> }.into_any(),
            _ => view! { <SummaryTab detail=detail_signal/> }.into_any(),
        }
    };

    let content = move || {
        if missing.get() {
            view! { <NotFound name=name_signal/> }.into_any()
        } else if has_detail.get() {
            view! {
                <Tabs tabs=tabs.clone() active=active base=base/>
                <div class="mt-5">{body}</div>
            }
            .into_any()
        } else if let Some(msg) = error.get().filter(|_| loaded.get()) {
            view! {
                <EmptyState icon=IconName::AlertCircle title="Can't load instance" body=msg>
                    <button type="button" class="btn" on:click=move |_| refetch()>
                        "Retry"
                    </button>
                </EmptyState>
            }
            .into_any()
        } else {
            view! { <Loading/> }.into_any()
        }
    };

    let header_actions: ChildrenFn = Arc::new(move || {
        view! {
            {spinner}
            {action_buttons}
        }
        .into_any()
    });
    let title = move || format!("{} \u{b7} simple-incus", name.get());
    let show_header = move || !missing.get();

    view! {
        <PageFrame>
            <Title text=title/>
            <Show when=show_header>
                <div class="mb-5 flex flex-wrap items-center justify-between gap-3">
                    <div class="flex min-w-0 items-center gap-3">
                        <h1 class="mono truncate text-[20px] font-semibold tracking-[-0.01em] text-text">{move || name.get()}</h1>
                        {move || status.get().map(|st| view! { <StatusPill status=st/> })}
                        {move || {
                            detail
                                .with(|d| d.as_ref().map(|d| d.summary.kind))
                                .map(|k| view! { <span class="text-[12px] text-muted">{kind_label(k)}</span> })
                        }}
                    </div>
                    <div class="flex items-center gap-2">{header_actions()}</div>
                </div>
            </Show>
            {content}
            <ActionConfirm pending=pending run=confirm_run/>
        </PageFrame>
    }
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::model::{DeviceInfo, InstanceSummary};

    fn detail(status: InstanceStatus) -> InstanceDetail {
        let running = status == InstanceStatus::Running;
        InstanceDetail {
            summary: InstanceSummary {
                name: "web-1".into(),
                kind: InstanceKind::Container,
                status,
                image_desc: "Alpine 3.21".into(),
                ipv4: vec!["10.0.0.5".into()],
                ipv6: vec![],
                cpu_pct: running.then_some(12.5),
                mem_used: 256 * 1024 * 1024,
                mem_limit: Some(1024 * 1024 * 1024),
                disk_used: 50 * 1024 * 1024,
                net_rx: 2048,
                net_tx: 1024,
                created_at: "2026-10-17T10:00:00Z".into(),
                profiles: vec!["default".into()],
            },
            config: BTreeMap::from([("limits.memory".into(), "1GiB".into())]),
            devices: vec![DeviceInfo { name: "root".into(), kind: "disk".into(), detail: "default:/".into() }],
        }
    }

    #[test]
    fn unknown_tab_is_summary() {
        for t in ["summary", "terminal", "console", "snapshots", "metrics"] {
            assert_eq!(tab_slug(t), t);
        }
        assert_eq!(tab_slug(""), "summary");
        assert_eq!(tab_slug("nope"), "summary");
    }

    #[test]
    fn summary_shows_details_resources_devices_and_config() {
        let owner = Owner::new();
        let html = owner.with(|| {
            let d = RwSignal::new(Some(detail(InstanceStatus::Running)));
            view! { <SummaryTab detail=d/> }.to_html()
        });
        for needle in ["10.0.0.5", "Alpine 3.21", "default:/", "limits.memory", "1GiB", "256.0 MiB / 1.0 GiB", "12.5%"] {
            assert!(html.contains(needle), "missing {needle}: {html}");
        }
    }

    #[test]
    fn metrics_need_a_running_instance() {
        let owner = Owner::new();
        let html = owner.with(|| {
            let d = RwSignal::new(Some(detail(InstanceStatus::Stopped)));
            let s = RwSignal::new(Vec::<Sample>::new());
            view! { <MetricsTab detail=d samples=s/> }.to_html()
        });
        assert!(html.contains("Metrics are available while the instance is running"), "{html}");
        assert!(!html.contains("<polyline"), "{html}");
    }

    #[test]
    fn metrics_draw_sparklines_from_samples() {
        let owner = Owner::new();
        let html = owner.with(|| {
            let d = RwSignal::new(Some(detail(InstanceStatus::Running)));
            let mut buf = Vec::new();
            for i in 0..4u32 {
                let s = Sample {
                    t_ms: f64::from(i) * 5000.0,
                    cpu: Some(10.0 + i as f32),
                    mem: 1 << 20,
                    disk: 1 << 20,
                    rx: u64::from(i) * 5000,
                    tx: u64::from(i) * 1000,
                };
                push_sample(&mut buf, s);
            }
            let s = RwSignal::new(buf);
            view! { <MetricsTab detail=d samples=s/> }.to_html()
        });
        assert_eq!(html.matches("<svg").count(), 4, "{html}");
        assert!(html.contains("<polyline"), "{html}");
        assert!(html.contains("1000.0 B/s") || html.contains("1000 B/s"), "rx rate: {html}");
        assert!(html.contains("12.5%"), "{html}");
    }
}
