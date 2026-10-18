//! Overview: instance counts, host capacity, storage pools and recent activity.
//!
//! The panels are presentational components fed with plain signals (what the
//! SSR tests render); [`OverviewPage`] owns the data.
use leptos::prelude::*;

use super::actions::err_msg;
use super::live::{bump, live_tick};
use super::PageFrame;
use crate::model::{HostInfo, InstanceStatus, InstanceSummary, PoolUsage, UiEvent, UiEventKind};
use crate::server::host::host_info;
use crate::server::instances::list_instances;
use crate::ui::components::{EmptyState, IconName, Meter, PageHeader, Skeleton};
use crate::ui::events::{use_activity, Activity};
use crate::ui::format::{format_bytes, now_secs, relative_time};

const INSTANCES_POLL_MS: u64 = 5000;
const HOST_POLL_MS: u64 = 10_000;
#[cfg(feature = "hydrate")]
const CLOCK_MS: u64 = 15_000;

/// Instance counts by status.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub total: usize,
    pub running: usize,
    pub stopped: usize,
    pub frozen: usize,
    /// Error and any other state.
    pub other: usize,
}

pub fn count_by_status(rows: &[InstanceSummary]) -> Counts {
    let mut c = Counts { total: rows.len(), ..Counts::default() };
    for r in rows {
        match r.status {
            InstanceStatus::Running => c.running += 1,
            InstanceStatus::Stopped => c.stopped += 1,
            InstanceStatus::Frozen => c.frozen += 1,
            _ => c.other += 1,
        }
    }
    c
}

/// Total CPU use of the running instances, in percent of the host. Each
/// instance reports its use against its own CPU allowance, so a pinned
/// instance counts for more than it really takes: an upper bound.
pub fn instance_cpu_pct(rows: &[InstanceSummary]) -> f64 {
    rows.iter()
        .filter(|r| r.status == InstanceStatus::Running)
        .filter_map(|r| r.cpu_pct)
        .map(f64::from)
        .fold(0.0, |a, b| a + b)
        .clamp(0.0, 100.0)
}

/// One-line description of an event for the activity feed.
pub fn activity_text(ev: &UiEvent) -> String {
    match ev.kind {
        UiEventKind::Lifecycle => {
            let action = ev.message.trim_start_matches("instance-").replace('-', " ");
            let mut chars = action.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect(),
                None => "Event".into(),
            }
        }
        UiEventKind::Operation => match ev.progress.as_deref().filter(|p| !p.is_empty()) {
            Some(p) => format!("{} \u{b7} {p}", ev.message),
            None => ev.message.clone(),
        },
    }
}

fn tile(label: &'static str, value: usize, dot: &'static str, warn: bool) -> impl IntoView {
    let tone = if warn && value > 0 { "text-danger" } else { "text-text" };
    view! {
        <a
            href="/instances"
            class="panel group block px-4 py-3.5 transition-colors hover:border-border-strong focus-visible:border-accent focus-visible:outline-none"
        >
            <div class="flex items-center gap-1.5 text-[12px] text-muted">
                <span class=format!("size-1.5 rounded-full {dot}") aria-hidden="true"></span>
                {label}
            </div>
            <div class=format!("num mt-1.5 text-[26px] font-semibold leading-none tracking-[-0.02em] {tone}")>{value}</div>
        </a>
    }
}

/// Instance counts by status.
#[component]
pub fn CountTiles(#[prop(into)] counts: Signal<Option<Counts>>) -> impl IntoView {
    move || {
        let c = counts.get().unwrap_or_default();
        let known = counts.with(Option::is_some);
        if !known {
            return view! {
                <div class="grid grid-cols-2 gap-3 md:grid-cols-5" aria-busy="true">
                    {(0..5).map(|_| view! { <div class="panel h-[74px] p-4"><Skeleton class="h-2.5 w-16"/></div> }).collect_view()}
                </div>
            }
            .into_any();
        }
        view! {
            <div class="grid grid-cols-2 gap-3 md:grid-cols-5" aria-label="Instance counts">
                {tile("Instances", c.total, "bg-border-strong", false)}
                {tile("Running", c.running, "bg-ok", false)}
                {tile("Stopped", c.stopped, "bg-muted", false)}
                {tile("Frozen", c.frozen, "bg-info", false)}
                {tile("Error / other", c.other, "bg-danger", true)}
            </div>
        }
        .into_any()
    }
}

fn panel_title(title: &'static str, note: Option<String>) -> impl IntoView {
    view! {
        <div class="flex items-center justify-between gap-3 border-b border-border px-4 py-2.5">
            <h2 class="text-[13px] font-semibold text-text">{title}</h2>
            {note.map(|n| view! { <span class="mono truncate text-muted">{n}</span> })}
        </div>
    }
}

fn fact(label: &'static str, value: String) -> impl IntoView {
    view! {
        <div class="flex items-baseline justify-between gap-4 text-[13px]">
            <dt class="shrink-0 text-muted">{label}</dt>
            <dd class="min-w-0 text-right text-text-2">{value}</dd>
        </div>
    }
}

/// CPU and memory of the host.
#[component]
pub fn HostPanel(
    #[prop(into)] host: Signal<Option<HostInfo>>,
    /// CPU use of the instances in percent of the host.
    #[prop(into)]
    cpu_pct: Signal<Option<f64>>,
    #[prop(optional, into)] error: Signal<Option<String>>,
) -> impl IntoView {
    let body = move || match host.get() {
        None => match error.get() {
            Some(e) => view! { <p class="px-4 py-6 text-[13px] text-muted">{e}</p> }.into_any(),
            None => view! {
                <div class="space-y-3 p-4" aria-busy="true">
                    <Skeleton class="h-3 w-40"/>
                    <Skeleton class="h-2 w-full"/>
                    <Skeleton class="h-2 w-full"/>
                </div>
            }
            .into_any(),
        },
        Some(h) => {
            let cpu_label = format!("CPU \u{b7} {} threads", h.cpu_threads);
            let mem_label = format!("Memory \u{b7} {} / {}", format_bytes(h.mem_used), format_bytes(h.mem_total));
            view! {
                <div class="space-y-4 p-4">
                    {cpu_pct.get().map(|p| view! { <Meter value=p max=100.0 label=cpu_label.clone()/> })}
                    <Meter value=h.mem_used as f64 max=h.mem_total as f64 label=mem_label/>
                    <dl class="space-y-1.5 border-t border-border pt-3">
                        {fact("Processor", if h.cpu_model.is_empty() { "\u{2014}".into() } else { h.cpu_model.clone() })}
                        {fact("Incus", h.incus_version.clone())}
                    </dl>
                </div>
            }
            .into_any()
        }
    };
    view! {
        <section class="panel overflow-hidden" aria-label="Host">
            {panel_title("Host", None)}
            {body}
        </section>
    }
}

fn pool_row(p: &PoolUsage) -> impl IntoView {
    let label = format!("{} \u{b7} {}", p.name, p.driver);
    if p.total == 0 {
        view! {
            <div class="min-w-0">
                <div class="flex items-baseline justify-between gap-3 text-[12px]">
                    <span class="truncate text-muted">{label}</span>
                    <span class="text-muted">"Usage unavailable"</span>
                </div>
            </div>
        }
        .into_any()
    } else {
        let label = format!("{label} \u{b7} {} / {}", format_bytes(p.used), format_bytes(p.total));
        view! { <Meter value=p.used as f64 max=p.total as f64 label=label/> }.into_any()
    }
}

/// Usage of every storage pool.
#[component]
pub fn StoragePanel(#[prop(into)] host: Signal<Option<HostInfo>>) -> impl IntoView {
    let body = move || match host.get() {
        None => view! { <div class="p-4" aria-busy="true"><Skeleton class="h-2 w-full"/></div> }.into_any(),
        Some(h) if h.pools.is_empty() => {
            view! { <p class="px-4 py-6 text-[13px] text-muted">"No storage pools."</p> }.into_any()
        }
        Some(h) => view! { <div class="space-y-4 p-4">{h.pools.iter().map(pool_row).collect_view()}</div> }.into_any(),
    };
    view! {
        <section class="panel overflow-hidden" aria-label="Storage">
            {panel_title("Storage", None)}
            {body}
        </section>
    }
}

fn activity_row(a: &Activity, now: i64) -> impl IntoView {
    let dot = match a.event.kind {
        UiEventKind::Lifecycle => "bg-accent",
        UiEventKind::Operation => "bg-info",
    };
    let link = a.event.instance.as_ref().map(|name| {
        view! {
            <a
                href=format!("/instances/{name}")
                class="mono text-text hover:text-accent focus-visible:text-accent focus-visible:outline-none"
            >
                {name.clone()}
            </a>
        }
    });
    view! {
        <li class="flex items-center gap-3 px-4 py-2 text-[13px]">
            <span class=format!("size-1.5 shrink-0 rounded-full {dot}") aria-hidden="true"></span>
            {link}
            <span class="min-w-0 flex-1 truncate text-text-2">{activity_text(&a.event)}</span>
            <span class="num shrink-0 text-[12px] text-muted">{relative_time(a.at, now)}</span>
        </li>
    }
}

/// The latest events since the page loaded, newest first.
#[component]
pub fn ActivityPanel(#[prop(into)] items: Signal<Vec<Activity>>, #[prop(into)] now: Signal<i64>) -> impl IntoView {
    let body = move || {
        let list = items.get();
        if list.is_empty() {
            view! {
                <p class="px-4 py-10 text-center text-[13px] text-muted">
                    "No activity yet. Lifecycle changes and operations show up here as they happen."
                </p>
            }
            .into_any()
        } else {
            let now = now.get();
            view! { <ul class="divide-y divide-border">{list.iter().map(|a| activity_row(a, now)).collect_view()}</ul> }
                .into_any()
        }
    };
    view! {
        <section class="panel overflow-hidden" aria-label="Recent activity">
            {panel_title("Recent activity", Some("since page load".into()))}
            {body}
        </section>
    }
}

#[component]
pub fn OverviewPage() -> impl IntoView {
    let inst_tick = live_tick(INSTANCES_POLL_MS);
    let host_tick = live_tick(HOST_POLL_MS);
    let instances = LocalResource::new(move || {
        inst_tick.get();
        list_instances()
    });
    let host = LocalResource::new(move || {
        host_tick.get();
        host_info()
    });

    // Keep the last good data on screen when a refetch fails.
    let rows = RwSignal::new(None::<Vec<InstanceSummary>>);
    let inst_error = RwSignal::new(None::<String>);
    Effect::new(move |_| match instances.get() {
        Some(Ok(v)) => {
            if rows.with_untracked(|r| r.as_ref() != Some(&v)) {
                rows.set(Some(v));
            }
            inst_error.set(None);
        }
        Some(Err(e)) => inst_error.set(Some(err_msg(&e))),
        None => {}
    });
    let host_data = RwSignal::new(None::<HostInfo>);
    let host_error = RwSignal::new(None::<String>);
    Effect::new(move |_| match host.get() {
        Some(Ok(h)) => {
            if host_data.with_untracked(|c| c.as_ref() != Some(&h)) {
                host_data.set(Some(h));
            }
            host_error.set(None);
        }
        Some(Err(e)) => host_error.set(Some(err_msg(&e))),
        None => {}
    });

    // Relative times tick over while the page is open.
    let now = RwSignal::new(now_secs());
    #[cfg(feature = "hydrate")]
    if let Ok(handle) = set_interval_with_handle(
        move || {
            now.try_set(now_secs());
        },
        std::time::Duration::from_millis(CLOCK_MS),
    ) {
        on_cleanup(move || handle.clear());
    }

    let counts = Signal::derive(move || rows.with(|r| r.as_deref().map(count_by_status)));
    let cpu = Signal::derive(move || rows.with(|r| r.as_deref().map(instance_cpu_pct)));
    let failed = Memo::new(move |_| {
        rows.with(Option::is_none) && host_data.with(Option::is_none) && inst_error.with(Option::is_some) && host_error.with(Option::is_some)
    });

    view! {
        <PageFrame>
            <PageHeader title="Overview" description="Host capacity and instance health at a glance."/>
            {move || {
                if failed.get() {
                    view! {
                        <EmptyState
                            icon=IconName::AlertCircle
                            title="Can't load the overview"
                            body=inst_error.get().unwrap_or_default()
                        >
                            <button type="button" class="btn" on:click=move |_| { bump(inst_tick); bump(host_tick); }>
                                "Retry"
                            </button>
                        </EmptyState>
                    }
                    .into_any()
                } else {
                    view! {
                        <div class="grid grid-cols-1 gap-4 xl:grid-cols-3">
                            <div class="min-w-0 space-y-4 xl:col-span-2">
                                <CountTiles counts=counts/>
                                <ActivityPanel items=use_activity() now=now/>
                            </div>
                            <div class="min-w-0 space-y-4">
                                <HostPanel host=host_data cpu_pct=cpu error=host_error/>
                                <StoragePanel host=host_data/>
                            </div>
                        </div>
                    }
                    .into_any()
                }
            }}
        </PageFrame>
    }
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;
    use crate::model::InstanceKind;

    fn inst(status: InstanceStatus, cpu: Option<f32>) -> InstanceSummary {
        InstanceSummary {
            name: "i".into(),
            kind: InstanceKind::Container,
            status,
            image_desc: String::new(),
            ipv4: vec![],
            ipv6: vec![],
            cpu_pct: cpu,
            mem_used: 0,
            mem_limit: None,
            disk_used: 0,
            net_rx: 0,
            net_tx: 0,
            created_at: String::new(),
            profiles: vec![],
        }
    }

    fn host() -> HostInfo {
        HostInfo {
            cpu_model: "Intel(R) Core(TM) i5-9500T".into(),
            cpu_threads: 6,
            mem_total: 32 * 1024 * 1024 * 1024,
            mem_used: 8 * 1024 * 1024 * 1024,
            pools: vec![
                PoolUsage { name: "default".into(), driver: "dir".into(), used: 30 << 30, total: 500 << 30 },
                PoolUsage { name: "broken".into(), driver: "zfs".into(), used: 0, total: 0 },
            ],
            incus_version: "7.5.1".into(),
        }
    }

    #[test]
    fn counts_and_cpu() {
        let rows = vec![
            inst(InstanceStatus::Running, Some(30.0)),
            inst(InstanceStatus::Running, Some(90.0)),
            inst(InstanceStatus::Stopped, Some(50.0)),
            inst(InstanceStatus::Frozen, None),
            inst(InstanceStatus::Error, None),
        ];
        assert_eq!(
            count_by_status(&rows),
            Counts { total: 5, running: 2, stopped: 1, frozen: 1, other: 1 }
        );
        assert_eq!(instance_cpu_pct(&rows), 100.0);
        assert_eq!(instance_cpu_pct(&rows[..1]), 30.0);
    }

    #[test]
    fn activity_wording() {
        let mut ev = UiEvent {
            kind: UiEventKind::Lifecycle,
            instance: Some("web1".into()),
            op_id: None,
            progress: None,
            message: "instance-shutdown".into(),
        };
        assert_eq!(activity_text(&ev), "Shutdown");
        ev.message = "instance-snapshot-created".into();
        assert_eq!(activity_text(&ev), "Snapshot created");
        ev.kind = UiEventKind::Operation;
        ev.message = "Creating instance".into();
        ev.progress = Some("Unpacking image: 42%".into());
        assert_eq!(activity_text(&ev), "Creating instance \u{b7} Unpacking image: 42%");
    }

    #[test]
    fn panels_render() {
        let owner = Owner::new();
        let html = owner.with(|| {
            let items = vec![
                Activity {
                    at: 1000,
                    event: UiEvent {
                        kind: UiEventKind::Operation,
                        instance: Some("web1".into()),
                        op_id: Some("a".into()),
                        progress: Some("60%".into()),
                        message: "Starting instance".into(),
                    },
                },
                Activity {
                    at: 900,
                    event: UiEvent {
                        kind: UiEventKind::Lifecycle,
                        instance: None,
                        op_id: None,
                        progress: None,
                        message: "image-created".into(),
                    },
                },
            ];
            let host = RwSignal::new(Some(host()));
            view! {
                <HostPanel host=host cpu_pct=Signal::stored(Some(12.0)) />
                <StoragePanel host=host/>
                <ActivityPanel items=Signal::stored(items) now=Signal::stored(1030i64)/>
            }
            .to_html()
        });
        assert!(html.contains("7.5.1") && html.contains("Intel(R) Core(TM) i5-9500T"), "{html}");
        assert!(html.contains("8.0 GiB / 32.0 GiB"), "{html}");
        assert!(html.contains("default \u{b7} dir \u{b7} 30.0 GiB / 500.0 GiB"), "{html}");
        // A pool whose usage is unknown is listed, without a bar.
        assert!(html.contains("broken \u{b7} zfs") && html.contains("Usage unavailable"), "{html}");
        assert_eq!(html.matches("role=\"meter\"").count(), 3, "{html}");
        assert!(html.contains("href=\"/instances/web1\""), "{html}");
        assert!(html.contains("Starting instance \u{b7} 60%"), "{html}");
        assert!(html.contains("just now") && html.contains("2m ago"), "{html}");
    }

    #[test]
    fn empty_activity_has_a_message() {
        let owner = Owner::new();
        let html = owner.with(|| {
            view! { <ActivityPanel items=Signal::stored(Vec::<Activity>::new()) now=Signal::stored(0i64)/> }.to_html()
        });
        assert!(html.contains("No activity yet"), "{html}");
    }
}
