//! Instances page: live table of instances with lifecycle actions.
//!
//! The page is split in two so the markup can be unit-tested without server
//! functions or a router:
//! * [`InstancesTable`] / [`InstancesStats`] are presentational components
//!   fed with plain signals (this is what the SSR test renders);
//! * [`InstancesPage`] owns the data (resource, refetch triggers, actions).
use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::Arc;

use leptos::ev::{KeyboardEvent, MouseEvent};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_navigate;

use super::PageFrame;
use crate::model::{Action, InstanceKind, InstanceStatus, InstanceSummary, UiEventKind};
use crate::server::instances::{delete_instance, instance_action, list_instances};
use crate::ui::components::{
    toast, use_create_drawer, Column, ConfirmModal, DataTable, EmptyState, Icon, IconName, Meter, PageHeader,
    StatusPill, ToastKind,
};
use crate::ui::events::{use_events, use_incus_connected};
use crate::ui::format::{created_label, format_bytes, parse_rfc3339};

/// Background refresh while the tab is visible.
#[cfg(feature = "hydrate")]
const POLL_MS: u64 = 5000;
/// Lifecycle events arrive in bursts; refetch once after they settle.
const DEBOUNCE_MS: u64 = 300;
/// A row stays "busy" at most this long after a successful action.
const BUSY_MAX_MS: u64 = 10_000;

// ---------------------------------------------------------------- actions

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowAction {
    Start,
    Stop,
    Restart,
    Freeze,
    Unfreeze,
    Delete,
}

impl RowAction {
    /// Actions offered for an instance in `status`, in display order.
    pub fn available(status: &InstanceStatus) -> &'static [RowAction] {
        use RowAction::*;
        match status {
            InstanceStatus::Running => &[Stop, Restart, Freeze, Delete],
            InstanceStatus::Stopped => &[Start, Delete],
            InstanceStatus::Frozen => &[Unfreeze, Stop, Delete],
            _ => &[Delete],
        }
    }

    fn label(self) -> &'static str {
        match self {
            RowAction::Start => "Start",
            RowAction::Stop => "Stop",
            RowAction::Restart => "Restart",
            RowAction::Freeze => "Freeze",
            RowAction::Unfreeze => "Unfreeze",
            RowAction::Delete => "Delete",
        }
    }

    /// Past tense used in the success toast.
    fn verb(self) -> &'static str {
        match self {
            RowAction::Start => "started",
            RowAction::Stop => "stopped",
            RowAction::Restart => "restarted",
            RowAction::Freeze => "frozen",
            RowAction::Unfreeze => "unfrozen",
            RowAction::Delete => "deleted",
        }
    }

    fn icon(self) -> IconName {
        match self {
            RowAction::Start | RowAction::Unfreeze => IconName::Play,
            RowAction::Stop => IconName::Stop,
            RowAction::Restart => IconName::Restart,
            RowAction::Freeze => IconName::Pause,
            RowAction::Delete => IconName::Trash,
        }
    }

    /// Stop, restart and delete ask for confirmation (with a Force option).
    fn needs_confirm(self) -> bool {
        matches!(self, RowAction::Stop | RowAction::Restart | RowAction::Delete)
    }

    /// The server action; `None` for delete, which has its own server fn.
    fn model(self, force: bool) -> Option<Action> {
        match self {
            RowAction::Start => Some(Action::Start),
            RowAction::Stop => Some(Action::Stop { force }),
            RowAction::Restart => Some(Action::Restart { force }),
            RowAction::Freeze => Some(Action::Freeze),
            RowAction::Unfreeze => Some(Action::Unfreeze),
            RowAction::Delete => None,
        }
    }
}

/// Shared with the row action cells (column renderers are plain `fn`s, so
/// they reach page state through context).
#[derive(Clone, Copy)]
struct RowCtx {
    /// Instances with an action in flight, with the status they had when it started.
    busy: RwSignal<HashMap<String, InstanceStatus>>,
    request: Callback<(String, RowAction)>,
}

fn stop_click(e: MouseEvent) {
    e.stop_propagation();
}
fn stop_key(e: KeyboardEvent) {
    e.stop_propagation();
}

fn actions_cell(r: &InstanceSummary) -> AnyView {
    let name = r.name.clone();
    let actions = RowAction::available(&r.status);
    let ctx = use_context::<RowCtx>();
    let is_busy = {
        let name = name.clone();
        move || ctx.is_some_and(|c| c.busy.with(|b| b.contains_key(&name)))
    };
    let buttons = actions
        .iter()
        .map(|&action| {
            let name = name.clone();
            let label = format!("{} {}", action.label(), name);
            let hover = if action == RowAction::Delete { "hover:text-danger" } else { "hover:text-text" };
            let is_busy = is_busy.clone();
            view! {
                <button
                    type="button"
                    title=action.label()
                    aria-label=label
                    disabled=is_busy
                    class=format!(
                        "inline-flex size-7 items-center justify-center rounded-md text-muted transition-colors hover:bg-hover focus-visible:bg-hover disabled:cursor-not-allowed disabled:opacity-40 disabled:hover:bg-transparent {hover}"
                    )
                    on:click=move |_| {
                        if let Some(c) = ctx {
                            c.request.run((name.clone(), action));
                        }
                    }
                >
                    <Icon name=action.icon() class="size-3.5"/>
                </button>
            }
        })
        .collect_view();
    let spinner = move || {
        is_busy().then(|| {
            view! {
                <span class="mr-1 text-accent" role="status" aria-label="Working">
                    <Icon name=IconName::Spinner class="size-3.5 animate-spin"/>
                </span>
            }
        })
    };
    view! {
        <div class="flex items-center justify-end" on:click=stop_click on:keydown=stop_key>
            {spinner}
            {buttons}
        </div>
    }
    .into_any()
}

// ---------------------------------------------------------------- table

fn name_cell(r: &InstanceSummary) -> AnyView {
    let href = format!("/instances/{}", r.name);
    view! {
        <a
            href=href
            class="mono text-[13px] font-medium text-text hover:text-accent focus-visible:text-accent focus-visible:outline-none"
            on:click=|e: MouseEvent| {
                // Plain click: let the row navigate client-side. Modified click: native behaviour only.
                if e.button() == 0 && !(e.ctrl_key() || e.meta_key() || e.shift_key() || e.alt_key()) {
                    e.prevent_default();
                } else {
                    e.stop_propagation();
                }
            }
            on:keydown=stop_key
        >
            {r.name.clone()}
        </a>
    }
    .into_any()
}

fn dash() -> AnyView {
    view! { <span class="text-muted">"—"</span> }.into_any()
}

fn type_cell(r: &InstanceSummary) -> AnyView {
    let label = match r.kind {
        InstanceKind::Container => "Container",
        InstanceKind::Vm => "VM",
    };
    view! { <span class="text-text-2">{label}</span> }.into_any()
}

fn ipv4_cell(r: &InstanceSummary) -> AnyView {
    match r.ipv4.split_first() {
        None => dash(),
        Some((first, rest)) => {
            let extra =
                (!rest.is_empty()).then(|| view! { <span class="ml-1 text-muted">{format!("+{}", rest.len())}</span> });
            view! {
                <span class="mono text-text-2">{first.clone()}</span>
                {extra}
            }
            .into_any()
        }
    }
}

fn cpu_cell(r: &InstanceSummary) -> AnyView {
    match (&r.status, r.cpu_pct) {
        (InstanceStatus::Running, Some(p)) => view! { <span>{format!("{p:.1}%")}</span> }.into_any(),
        _ => dash(),
    }
}

fn memory_cell(r: &InstanceSummary) -> AnyView {
    if r.mem_used == 0 && r.status != InstanceStatus::Running {
        return dash();
    }
    match r.mem_limit.filter(|&l| l > 0) {
        Some(limit) => {
            let label = format!("{} / {}", format_bytes(r.mem_used), format_bytes(limit));
            view! {
                <div class="ml-auto w-44 text-left">
                    <Meter value=r.mem_used as f64 max=limit as f64 label=label/>
                </div>
            }
            .into_any()
        }
        None => view! { <span>{format_bytes(r.mem_used)}</span> }.into_any(),
    }
}

fn disk_cell(r: &InstanceSummary) -> AnyView {
    if r.disk_used == 0 {
        dash()
    } else {
        view! { <span>{format_bytes(r.disk_used)}</span> }.into_any()
    }
}

fn created_cell(r: &InstanceSummary) -> AnyView {
    view! { <span class="text-text-2" title=r.created_at.clone()>{created_label(&r.created_at)}</span> }.into_any()
}

fn status_rank(s: &InstanceStatus) -> u8 {
    match s {
        InstanceStatus::Running => 0,
        InstanceStatus::Frozen => 1,
        InstanceStatus::Stopped => 2,
        InstanceStatus::Error => 3,
        InstanceStatus::Other(_) => 4,
    }
}

fn columns() -> Vec<Column<InstanceSummary>> {
    vec![
        Column::new("Name", name_cell).sortable(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
        Column::new("Status", |r: &InstanceSummary| view! { <StatusPill status=r.status.clone()/> }.into_any())
            .sortable(|a, b| status_rank(&a.status).cmp(&status_rank(&b.status))),
        Column::new("Type", type_cell).sortable(|a, b| (a.kind as u8).cmp(&(b.kind as u8))),
        Column::new("IPv4", ipv4_cell),
        Column::new("CPU", cpu_cell)
            .sortable(|a, b| {
                a.cpu_pct.unwrap_or(-1.0).partial_cmp(&b.cpu_pct.unwrap_or(-1.0)).unwrap_or(Ordering::Equal)
            })
            .numeric(),
        Column::new("Memory", memory_cell).sortable(|a, b| a.mem_used.cmp(&b.mem_used)).numeric(),
        Column::new("Disk", disk_cell).sortable(|a, b| a.disk_used.cmp(&b.disk_used)).numeric(),
        Column::new("Created", created_cell)
            .sortable(|a, b| parse_rfc3339(&a.created_at).cmp(&parse_rfc3339(&b.created_at))),
        Column::new("", actions_cell).numeric(),
    ]
}

/// Running / Stopped / Total counts.
pub fn counts(rows: &[InstanceSummary]) -> (usize, usize, usize) {
    let running = rows.iter().filter(|r| r.status == InstanceStatus::Running).count();
    let stopped = rows.iter().filter(|r| r.status == InstanceStatus::Stopped).count();
    (running, stopped, rows.len())
}

/// Header counts (Running / Stopped / Total).
#[component]
pub fn InstancesStats(#[prop(into)] rows: Signal<Vec<InstanceSummary>>) -> impl IntoView {
    let c = Memo::new(move |_| rows.with(|r| counts(r)));
    let stat = |label: &'static str, dot: &'static str, n: Signal<usize>| {
        view! {
            <div class="flex items-center gap-1.5 text-[12px] text-muted">
                <span class=format!("size-1.5 rounded-full {dot}") aria-hidden="true"></span>
                {label}
                <span class="num text-[13px] font-semibold text-text">{move || n.get()}</span>
            </div>
        }
    };
    view! {
        <div class="flex items-center gap-4 pr-2" aria-label="Instance counts">
            {stat("Running", "bg-ok", Signal::derive(move || c.get().0))}
            {stat("Stopped", "bg-muted", Signal::derive(move || c.get().1))}
            {stat("Total", "bg-border-strong", Signal::derive(move || c.get().2))}
        </div>
    }
}

/// The instance table with search, sorting and row actions (the action
/// buttons are inert unless the page provides [`RowCtx`]).
#[component]
pub fn InstancesTable(
    #[prop(into)] rows: Signal<Vec<InstanceSummary>>,
    #[prop(optional, into)] loading: Signal<bool>,
    /// Called when a row (not one of its controls) is activated.
    on_open: Callback<InstanceSummary>,
) -> impl IntoView {
    view! {
        <DataTable
            rows=rows
            columns=columns()
            row_key=|r: &InstanceSummary| r.name.clone()
            search=|r: &InstanceSummary| format!("{} {} {}", r.name, r.ipv4.join(" "), r.image_desc)
            placeholder="Filter by name, IP or image…"
            empty="No instances yet. Create one to get started."
            loading=loading
            on_row_click=on_open
        />
    }
}

// ---------------------------------------------------------------- page

fn err_msg(e: &ServerFnError) -> String {
    match e {
        ServerFnError::ServerError(m) => m.clone(),
        other => other.to_string(),
    }
}

/// Runs `f` once after `ms` (browser only; the server never needs it).
fn after_ms(ms: u64, f: impl FnOnce() + 'static) {
    #[cfg(feature = "hydrate")]
    set_timeout(f, std::time::Duration::from_millis(ms));
    #[cfg(not(feature = "hydrate"))]
    let _ = (ms, f);
}

#[cfg(feature = "hydrate")]
fn page_visible() -> bool {
    use leptos::web_sys::VisibilityState;
    leptos::prelude::document().visibility_state() == VisibilityState::Visible
}

#[component]
pub fn InstancesPage() -> impl IntoView {
    let tick = RwSignal::new(0u32);
    let resource = Resource::new(move || tick.get(), |_| list_instances());
    let rows = RwSignal::new(Vec::<InstanceSummary>::new());
    let error = RwSignal::new(None::<String>);
    let loaded = RwSignal::new(false);
    let busy = RwSignal::new(HashMap::<String, InstanceStatus>::new());
    let pending = RwSignal::new(None::<(String, RowAction)>);
    let refetch = move || tick.update(|t| *t = t.wrapping_add(1));

    // Keep the last good rows on screen when a refetch fails.
    Effect::new(move |_| match resource.get() {
        Some(Ok(v)) => {
            // A row is done once its status moved (or it is gone).
            busy.update(|b| b.retain(|n, st| v.iter().find(|r| &r.name == n).is_some_and(|r| &r.status == st)));
            rows.set(v);
            error.set(None);
            loaded.set(true);
        }
        Some(Err(e)) => {
            error.set(Some(err_msg(&e)));
            loaded.set(true);
        }
        None => {}
    });

    // Refetch: debounced on Lifecycle events ...
    let events = use_events();
    let generation = StoredValue::new(0u32);
    Effect::new(move |_| {
        if events.get().is_some_and(|e| e.kind == UiEventKind::Lifecycle) {
            let g = generation.get_value().wrapping_add(1);
            generation.set_value(g);
            after_ms(DEBOUNCE_MS, move || {
                if generation.get_value() == g {
                    refetch();
                }
            });
        }
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

    let run = move |name: String, action: RowAction, force: bool| {
        let status = rows.with_untracked(|r| r.iter().find(|i| i.name == name).map(|i| i.status.clone()));
        busy.update(|b| {
            b.insert(name.clone(), status.unwrap_or(InstanceStatus::Stopped));
        });
        spawn_local(async move {
            let result = match action.model(force) {
                Some(a) => instance_action(name.clone(), a).await.map(|_| ()),
                None => delete_instance(name.clone(), force).await,
            };
            let unbusy = move |name: &str| {
                busy.update(|b| {
                    b.remove(name);
                })
            };
            match result {
                Ok(()) => {
                    toast(ToastKind::Success, format!("Instance {name} {}", action.verb()));
                    refetch();
                    if action == RowAction::Delete {
                        unbusy(&name);
                    } else {
                        // Normally cleared earlier, when the refetched status changes.
                        after_ms(BUSY_MAX_MS, move || unbusy(&name));
                    }
                }
                Err(e) => {
                    unbusy(&name);
                    toast(ToastKind::Error, err_msg(&e));
                }
            }
        });
    };

    let request = Callback::new(move |(name, action): (String, RowAction)| {
        if action.needs_confirm() {
            pending.set(Some((name, action)));
        } else {
            run(name, action, false);
        }
    });
    provide_context(RowCtx { busy, request });

    let navigate = use_navigate();
    let on_open =
        Callback::new(move |r: InstanceSummary| navigate(&format!("/instances/{}", r.name), Default::default()));

    let create = use_create_drawer();
    let show_error = Memo::new(move |_| rows.with(|r| r.is_empty()) && error.with(|e| e.is_some()));
    let list: Signal<Vec<InstanceSummary>> = rows.into();
    let loading = Signal::derive(move || !loaded.get());

    let confirm = move || {
        pending.get().map(|(name, action)| {
            let open = RwSignal::new(true);
            let force = RwSignal::new(false);
            Effect::new(move |_| {
                if !open.get() {
                    pending.set(None);
                }
            });
            let (title, body, label, force_label, danger) = match action {
                RowAction::Delete => (
                    format!("Delete {name}?"),
                    "This permanently removes the instance and all of its data. This cannot be undone.",
                    "Delete",
                    "Force (stop first)",
                    true,
                ),
                RowAction::Restart => (
                    format!("Restart {name}?"),
                    "The instance will be shut down and started again.",
                    "Restart",
                    "Force (do not wait for a clean shutdown)",
                    false,
                ),
                _ => (
                    format!("Stop {name}?"),
                    "The instance will be shut down.",
                    "Stop",
                    "Force (do not wait for a clean shutdown)",
                    false,
                ),
            };
            let extra: ChildrenFn = Arc::new(move || {
                view! {
                    <label class="flex cursor-pointer items-center gap-2 text-[13px] text-text-2">
                        <input
                            type="checkbox"
                            class="size-3.5 accent-[var(--accent)]"
                            prop:checked=move || force.get()
                            on:change=move |e| force.set(event_target_checked(&e))
                        />
                        {force_label}
                    </label>
                }
                .into_any()
            });
            view! {
                <ConfirmModal
                    open=open
                    title=title
                    body=body
                    confirm_label=label
                    danger=danger
                    extra=extra
                    on_confirm=Callback::new(move |_| run(name.clone(), action, force.get_untracked()))
                />
            }
        })
    };

    let header_actions: ChildrenFn = Arc::new(move || {
        view! {
            <InstancesStats rows=list/>
            <button type="button" class="btn btn-primary" on:click=move |_| create.open.set(true)>
                <Icon name=IconName::Plus/>
                "Create instance"
            </button>
        }
        .into_any()
    });

    view! {
        <PageFrame>
            <PageHeader
                title="Instances"
                description="Containers and virtual machines on this host."
                actions=header_actions
            />
            {move || {
                if show_error.get() {
                    view! {
                        <EmptyState
                            icon=IconName::AlertCircle
                            title="Can't load instances"
                            body=error.get().unwrap_or_default()
                        >
                            <button type="button" class="btn" on:click=move |_| refetch()>
                                "Retry"
                            </button>
                        </EmptyState>
                    }
                    .into_any()
                } else {
                    view! { <InstancesTable rows=list loading=loading on_open=on_open/> }.into_any()
                }
            }}
            {confirm}
        </PageFrame>
    }
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;

    fn fixture(name: &str, status: InstanceStatus) -> InstanceSummary {
        let running = status == InstanceStatus::Running;
        InstanceSummary {
            name: name.into(),
            kind: InstanceKind::Container,
            status,
            image_desc: "Alpine 3.21".into(),
            ipv4: if running { vec!["10.0.0.5".into()] } else { vec![] },
            ipv6: vec![],
            cpu_pct: running.then_some(12.5),
            mem_used: if running { 256 * 1024 * 1024 } else { 0 },
            mem_limit: running.then_some(1024 * 1024 * 1024),
            disk_used: 50 * 1024 * 1024,
            net_rx: 0,
            net_tx: 0,
            created_at: "2026-10-17T10:00:00Z".into(),
            profiles: vec!["default".into()],
        }
    }

    fn fixtures() -> Vec<InstanceSummary> {
        vec![fixture("web-1", InstanceStatus::Running), fixture("db-1", InstanceStatus::Stopped)]
    }

    /// Rendering the whole page needs the server-fn resource, the router and
    /// the event socket, so the presentational pieces are rendered instead.
    #[test]
    fn instances_page_lists_rows() {
        let owner = Owner::new();
        let html = owner.with(|| {
            let rows = RwSignal::new(fixtures());
            view! {
                <InstancesStats rows=rows/>
                <InstancesTable rows=rows on_open=Callback::new(|_| {})/>
            }
            .to_html()
        });
        assert!(html.contains("web-1"), "{html}");
        assert!(html.contains("db-1"), "{html}");
        assert!(html.contains("Running"), "{html}");
        assert!(html.contains("Stopped"), "{html}");
        assert!(html.contains("10.0.0.5"), "{html}");
        assert!(html.contains("aria-label=\"Stop web-1\""), "{html}");
        assert!(html.contains("aria-label=\"Start db-1\""), "{html}");
    }

    #[test]
    fn counts_and_actions() {
        let mut rows = fixtures();
        rows.push(fixture("x", InstanceStatus::Frozen));
        assert_eq!(counts(&rows), (1, 1, 3));
        use RowAction::*;
        assert_eq!(RowAction::available(&InstanceStatus::Running), [Stop, Restart, Freeze, Delete]);
        assert_eq!(RowAction::available(&InstanceStatus::Stopped), [Start, Delete]);
        assert_eq!(RowAction::available(&InstanceStatus::Frozen), [Unfreeze, Stop, Delete]);
    }
}
