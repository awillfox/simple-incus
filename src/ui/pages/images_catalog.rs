//! Images page, "Catalog" tab: browse the remote image catalog and launch from it.
use leptos::prelude::*;
use leptos_router::hooks::use_query_map;

use super::actions::{after_ms, err_msg};
use crate::model::{CatalogImage, ImageSource};
use crate::server::images::{catalog, CATALOG_LIMIT};
use crate::ui::components::{use_create_drawer, EmptyState, Icon, IconName, Skeleton};

const DEBOUNCE_MS: u64 = 250;

/// Container / VM filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KindFilter {
    All,
    Container,
    Vm,
}

impl KindFilter {
    fn matches(self, i: &CatalogImage) -> bool {
        match self {
            KindFilter::All => true,
            KindFilter::Container => i.container,
            KindFilter::Vm => i.vm,
        }
    }
}

/// Images passing the type filter.
pub fn filter_kind(all: &[CatalogImage], kind: KindFilter) -> Vec<CatalogImage> {
    all.iter().filter(|i| kind.matches(i)).cloned().collect()
}

/// Distros with their image counts, in the order they first appear.
pub fn distro_counts(images: &[CatalogImage]) -> Vec<(String, usize)> {
    let mut out: Vec<(String, usize)> = Vec::new();
    for i in images {
        match out.iter_mut().find(|(d, _)| *d == i.distro) {
            Some((_, n)) => *n += 1,
            None => out.push((i.distro.clone(), 1)),
        }
    }
    out
}

/// Consecutive images of one distro, as `(distro, images)` groups. With
/// `only` set, just that distro.
pub fn group_by_distro(images: &[CatalogImage], only: Option<&str>) -> Vec<(String, Vec<CatalogImage>)> {
    let mut out: Vec<(String, Vec<CatalogImage>)> = Vec::new();
    for i in images.iter().filter(|i| only.is_none_or(|d| d == i.distro)) {
        match out.last_mut() {
            Some((d, list)) if *d == i.distro => list.push(i.clone()),
            _ => out.push((i.distro.clone(), vec![i.clone()])),
        }
    }
    out
}

fn kind_chips(i: &CatalogImage) -> impl IntoView {
    let chip = |label: &'static str| {
        view! {
            <span class="inline-flex h-5 items-center rounded border border-border-strong px-1.5 text-[11px] font-medium text-text-2">
                {label}
            </span>
        }
    };
    view! {
        <span class="flex items-center gap-1">
            {i.container.then(|| chip("Container"))}
            {i.vm.then(|| chip("VM"))}
        </span>
    }
}

fn image_row(i: CatalogImage, on_launch: Callback<String>) -> impl IntoView {
    let alias = i.alias.clone();
    let label = format!("Launch {}", i.alias);
    let variant = (i.variant != "default").then(|| {
        view! { <span class="rounded bg-surface-2 px-1.5 py-0.5 text-[11px] text-muted">{i.variant.clone()}</span> }
    });
    view! {
        <li class="flex h-10 items-center gap-4 px-4 text-[13px] transition-colors hover:bg-hover/60">
            <span class="mono w-56 shrink-0 truncate font-medium text-text" title=i.alias.clone()>{i.alias.clone()}</span>
            <span class="flex min-w-0 flex-1 items-center gap-2">
                <span class="truncate text-text-2">{i.description.clone()}</span>
                {variant}
            </span>
            {kind_chips(&i)}
            <button
                type="button"
                class="btn h-7 px-2.5 text-[12px]"
                aria-label=label
                on:click=move |_| on_launch.run(alias.clone())
            >
                <Icon name=IconName::Play class="size-3"/>
                "Launch"
            </button>
        </li>
    }
}

/// Distro and type facets. `distros` are the counts for the current type filter.
#[component]
pub fn Facets(
    #[prop(into)] distros: Signal<Vec<(String, usize)>>,
    selected: RwSignal<Option<String>>,
    kind: RwSignal<KindFilter>,
) -> impl IntoView {
    let kind_btn = move |label: &'static str, value: KindFilter| {
        view! {
            <button
                type="button"
                aria-pressed=move || (kind.get() == value).to_string()
                class=move || {
                    if kind.get() == value {
                        "h-7 flex-1 rounded bg-accent-soft text-[12px] font-medium text-text"
                    } else {
                        "h-7 flex-1 rounded text-[12px] font-medium text-muted hover:text-text"
                    }
                }
                on:click=move |_| kind.set(value)
            >
                {label}
            </button>
        }
    };
    let total = move || distros.with(|d| d.iter().map(|(_, n)| n).sum::<usize>());
    let item = move |label: String, count: usize, value: Option<String>| {
        let v = value.clone();
        let active = Memo::new(move |_| selected.get() == v);
        view! {
            <li>
                <button
                    type="button"
                    aria-pressed=move || active.get().to_string()
                    class=move || {
                        if active.get() {
                            "flex h-8 w-full items-center justify-between gap-2 rounded-md bg-accent-soft px-2.5 text-[13px] font-medium text-text"
                        } else {
                            "flex h-8 w-full items-center justify-between gap-2 rounded-md px-2.5 text-[13px] text-text-2 hover:bg-hover hover:text-text"
                        }
                    }
                    on:click=move |_| selected.set(value.clone())
                >
                    <span class="truncate">{label}</span>
                    <span class="num text-[12px] text-muted">{count}</span>
                </button>
            </li>
        }
    };
    view! {
        <aside class="panel flex max-h-[calc(100dvh-14rem)] flex-col overflow-hidden" aria-label="Filters">
            <div class="border-b border-border p-3">
                <div class="mb-2 text-[12px] font-medium text-muted">"Type"</div>
                <div class="flex gap-0.5 rounded-md border border-border bg-surface-2 p-0.5">
                    {kind_btn("All", KindFilter::All)}
                    {kind_btn("Container", KindFilter::Container)}
                    {kind_btn("VM", KindFilter::Vm)}
                </div>
            </div>
            <div class="min-h-0 flex-1 overflow-y-auto p-2">
                <div class="px-2.5 pb-1.5 pt-1 text-[12px] font-medium text-muted">"Distribution"</div>
                <ul class="space-y-0.5">
                    {move || item("All".into(), total(), None)}
                    {move || {
                        distros.get().into_iter().map(|(d, n)| item(d.clone(), n, Some(d))).collect_view()
                    }}
                </ul>
            </div>
        </aside>
    }
}

/// Result groups, one panel per distro.
#[component]
pub fn CatalogGroups(
    #[prop(into)] groups: Signal<Vec<(String, Vec<CatalogImage>)>>,
    on_launch: Callback<String>,
) -> impl IntoView {
    move || {
        groups
            .get()
            .into_iter()
            .map(|(distro, images)| {
                let n = images.len();
                view! {
                    <section class="panel overflow-hidden" aria-label=distro.clone()>
                        <header class="flex items-center justify-between border-b border-border bg-surface-2 px-4 py-2">
                            <h2 class="text-[13px] font-semibold capitalize text-text">{distro.clone()}</h2>
                            <span class="num text-[12px] text-muted">{n}</span>
                        </header>
                        <ul class="divide-y divide-border">
                            {images.into_iter().map(|i| image_row(i, on_launch)).collect_view()}
                        </ul>
                    </section>
                }
            })
            .collect_view()
    }
}

#[component]
pub fn CatalogImages() -> impl IntoView {
    // `/images/catalog?q=ubuntu` opens with that search.
    let initial = use_query_map().with_untracked(|q| q.get("q")).unwrap_or_default();
    let input = RwSignal::new(initial.clone());
    let query = RwSignal::new(initial);
    let debounce = StoredValue::new(0u32);
    let kind = RwSignal::new(KindFilter::All);
    let selected = RwSignal::new(None::<String>);
    let retry = RwSignal::new(0u32);

    let on_input = move |e| {
        let v = event_target_value(&e);
        input.set(v.clone());
        let Some(g) = debounce.try_get_value().map(|g| g.wrapping_add(1)) else { return };
        debounce.try_set_value(g);
        after_ms(DEBOUNCE_MS, move || {
            if debounce.try_get_value() == Some(g) {
                query.try_set(v);
            }
        });
    };

    let resource = LocalResource::new(move || {
        retry.get();
        catalog(query.get())
    });
    // Keep the previous results on screen while a new search runs or fails.
    let results = RwSignal::new(None::<Vec<CatalogImage>>);
    let error = RwSignal::new(None::<String>);
    let searching = RwSignal::new(true);
    Effect::new(move |_| {
        match resource.get() {
            Some(Ok(v)) => {
                results.set(Some(v));
                error.set(None);
                searching.set(false);
            }
            Some(Err(e)) => {
                error.set(Some(err_msg(&e)));
                searching.set(false);
            }
            None => searching.set(true),
        };
    });

    let typed = Memo::new(move |_| results.with(|r| r.as_ref().map(|r| filter_kind(r, kind.get()))));
    let distros = Signal::derive(move || typed.with(|t| t.as_deref().map(distro_counts).unwrap_or_default()));
    // A distro that dropped out of the results no longer filters.
    let active = Memo::new(move |_| selected.get().filter(|d| distros.with(|v| v.iter().any(|(x, _)| x == d))));
    let groups = Signal::derive(move || {
        typed.with(|t| t.as_deref().map(|t| group_by_distro(t, active.get().as_deref())).unwrap_or_default())
    });
    let shown = Memo::new(move |_| groups.with(|g| g.iter().map(|(_, l)| l.len()).sum::<usize>()));
    let capped = Memo::new(move |_| results.with(|r| r.as_ref().is_some_and(|r| r.len() >= CATALOG_LIMIT)));

    let create = use_create_drawer();
    let on_launch =
        Callback::new(move |alias: String| create.open_with(Some(ImageSource::Catalog { alias })));

    let body = move || {
        if results.with(Option::is_none) {
            return match error.get() {
                Some(msg) => view! {
                    <EmptyState icon=IconName::AlertCircle title="Catalog unavailable" body=msg>
                        <button type="button" class="btn" on:click=move |_| retry.update(|r| *r = r.wrapping_add(1))>
                            "Retry"
                        </button>
                    </EmptyState>
                }
                .into_any(),
                None => view! {
                    <div class="panel space-y-3 p-5" aria-busy="true" aria-label="Loading catalog">
                        <Skeleton class="h-3 w-40"/>
                        <Skeleton class="h-3 w-72"/>
                        <Skeleton class="h-3 w-56"/>
                        <Skeleton class="h-3 w-64"/>
                    </div>
                }
                .into_any(),
            };
        }
        if shown.get() == 0 {
            return view! {
                <EmptyState icon=IconName::Search title="No images match" body="Try a different search or clear the filters."/>
            }
            .into_any();
        }
        view! { <div class="space-y-4"><CatalogGroups groups=groups on_launch=on_launch/></div> }.into_any()
    };

    view! {
        <div class="grid grid-cols-1 gap-4 lg:grid-cols-[220px_minmax(0,1fr)]">
            <Facets distros=distros selected=selected kind=kind/>
            <div class="min-w-0 space-y-4">
                <div class="flex flex-wrap items-center gap-3">
                    <label class="relative min-w-0 flex-1 sm:max-w-[360px]">
                        <span class="sr-only">"Search the catalog"</span>
                        <span class="pointer-events-none absolute inset-y-0 left-2.5 flex items-center text-muted">
                            <Icon name=IconName::Search class="size-3.5"/>
                        </span>
                        <input
                            type="search"
                            class="input h-8 w-full pl-8"
                            placeholder="Search distro, release or description…"
                            prop:value=move || input.get()
                            on:input=on_input
                        />
                    </label>
                    {move || searching.get().then(|| view! {
                        <span class="text-accent" role="status" aria-label="Searching">
                            <Icon name=IconName::Spinner class="size-4 animate-spin"/>
                        </span>
                    })}
                    <span class="num ml-auto text-[12px] text-muted">
                        {move || {
                            let n = shown.get();
                            let more = if capped.get() { "+" } else { "" };
                            format!("{n}{more} {}", if n == 1 { "image" } else { "images" })
                        }}
                    </span>
                </div>
                {move || {
                    // Stale data stays visible, with the reason, when a refresh fails.
                    error.get().filter(|_| results.with(Option::is_some)).map(|msg| view! {
                        <div class="rounded-md border border-warn/30 bg-warn/10 px-3 py-2 text-[13px] text-warn" role="alert">{msg}</div>
                    })
                }}
                {body}
                {move || capped.get().then(|| view! {
                    <p class="text-center text-[12px] text-muted">
                        {format!("Showing the first {CATALOG_LIMIT} matches. Search to narrow the list.")}
                    </p>
                })}
            </div>
        </div>
    }
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;

    fn img(alias: &str, distro: &str, container: bool, vm: bool) -> CatalogImage {
        CatalogImage {
            alias: alias.into(),
            distro: distro.into(),
            release: alias.split('/').nth(1).unwrap_or_default().into(),
            variant: if alias.matches('/').count() > 1 { "cloud".into() } else { "default".into() },
            container,
            vm,
            description: format!("{distro} image"),
        }
    }

    fn sample() -> Vec<CatalogImage> {
        vec![
            img("alpine/3.21", "alpine", true, true),
            img("alpine/3.20", "alpine", true, true),
            img("busybox/1.38.0", "busybox", true, false),
            img("freebsd/15.0", "freebsd", false, true),
            img("ubuntu/24.04/cloud", "ubuntu", true, true),
        ]
    }

    #[test]
    fn facets_filter_and_group() {
        let all = sample();
        assert_eq!(filter_kind(&all, KindFilter::All).len(), 5);
        let vms = filter_kind(&all, KindFilter::Vm);
        assert_eq!(vms.iter().map(|i| i.alias.as_str()).collect::<Vec<_>>(), ["alpine/3.21", "alpine/3.20", "freebsd/15.0", "ubuntu/24.04/cloud"]);
        let containers = filter_kind(&all, KindFilter::Container);
        assert!(containers.iter().all(|i| i.container) && containers.len() == 4);
        assert_eq!(
            distro_counts(&containers),
            [("alpine".to_string(), 2), ("busybox".to_string(), 1), ("ubuntu".to_string(), 1)]
        );
        let groups = group_by_distro(&all, None);
        assert_eq!(groups.iter().map(|(d, l)| (d.as_str(), l.len())).collect::<Vec<_>>(), [("alpine", 2), ("busybox", 1), ("freebsd", 1), ("ubuntu", 1)]);
        let only = group_by_distro(&all, Some("alpine"));
        assert_eq!(only.len(), 1);
        assert_eq!(only[0].1.len(), 2);
    }

    #[test]
    fn catalog_groups_render() {
        let owner = Owner::new();
        let html = owner.with(|| {
            let groups = Signal::stored(group_by_distro(&sample(), None));
            view! { <CatalogGroups groups=groups on_launch=Callback::new(|_| {})/> }.to_html()
        });
        assert!(html.contains("aria-label=\"Launch alpine/3.21\""), "{html}");
        assert!(html.contains("aria-label=\"Launch ubuntu/24.04/cloud\""), "{html}");
        // The cloud variant is labelled, and VM-only images carry only the VM chip.
        assert!(html.contains(">cloud<"), "{html}");
        let row = |alias: &str| {
            let start = html.find(&format!("title=\"{alias}\"")).expect("row");
            let end = html[start..].find("</li>").unwrap() + start;
            html[start..end].to_string()
        };
        let free = row("freebsd/15.0");
        assert!(free.contains(">VM<") && !free.contains(">Container<"), "{free}");
        let bb = row("busybox/1.38.0");
        assert!(bb.contains(">Container<") && !bb.contains(">VM<"), "{bb}");
    }
}
