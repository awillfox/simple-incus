//! Image picker of the create drawer: local images and the remote catalog.
use leptos::prelude::*;

use super::actions::{after_ms, err_msg};
use crate::model::{CatalogImage, ImageInfo, ImageSource, InstanceKind};
use crate::server::images::{catalog, list_images};
use crate::ui::components::{Icon, IconName};
use crate::ui::format::format_bytes;

const DEBOUNCE_MS: u64 = 250;

/// The image chosen in the form, with what it can run as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picked {
    pub source: ImageSource,
    pub label: String,
    pub container: bool,
    pub vm: bool,
}

impl Picked {
    pub fn from_catalog(i: &CatalogImage) -> Picked {
        Picked {
            source: ImageSource::Catalog { alias: i.alias.clone() },
            label: i.alias.clone(),
            container: i.container,
            vm: i.vm,
        }
    }

    pub fn from_local(i: &ImageInfo) -> Picked {
        let label = i.aliases.first().cloned().unwrap_or_else(|| i.description.clone());
        Picked {
            source: ImageSource::Local { fingerprint: i.fingerprint.clone() },
            label,
            container: i.kind == InstanceKind::Container,
            vm: i.kind == InstanceKind::Vm,
        }
    }

    pub fn supports(&self, kind: InstanceKind) -> bool {
        match kind {
            InstanceKind::Container => self.container,
            InstanceKind::Vm => self.vm,
        }
    }

    /// `current` when this image supports it, otherwise the kind it does.
    pub fn default_kind(&self, current: InstanceKind) -> InstanceKind {
        if self.supports(current) {
            current
        } else if self.container {
            InstanceKind::Container
        } else {
            InstanceKind::Vm
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Source {
    Local,
    Catalog,
}

fn caps_badges(container: bool, vm: bool) -> impl IntoView {
    view! {
        <span class="flex shrink-0 gap-1">
            {container.then(|| view! { <span class="rounded border border-border px-1.5 text-[11px] text-muted">"Container"</span> })}
            {vm.then(|| view! { <span class="rounded border border-border px-1.5 text-[11px] text-muted">"VM"</span> })}
        </span>
    }
}

fn row_class(active: bool) -> &'static str {
    if active {
        "flex w-full items-center justify-between gap-3 rounded-md bg-accent-soft px-2.5 py-1.5 text-left"
    } else {
        "flex w-full items-center justify-between gap-3 rounded-md px-2.5 py-1.5 text-left hover:bg-hover"
    }
}

/// Resolves a preselected image (from "Launch") to its capabilities.
async fn resolve(source: ImageSource) -> Option<Picked> {
    match source {
        ImageSource::Catalog { alias } => {
            let hits = catalog(alias.clone()).await.ok()?;
            let hit = hits.iter().find(|i| i.alias == alias)?;
            Some(Picked::from_catalog(hit))
        }
        ImageSource::Local { fingerprint } => {
            let all = list_images().await.ok()?;
            all.iter().find(|i| i.fingerprint == fingerprint).map(Picked::from_local)
        }
    }
}

#[component]
pub fn ImagePicker(picked: RwSignal<Option<Picked>>, initial: Option<ImageSource>) -> impl IntoView {
    let tab = RwSignal::new(match initial {
        Some(ImageSource::Local { .. }) => Source::Local,
        _ => Source::Catalog,
    });
    if let Some(src) = initial {
        leptos::task::spawn_local(async move {
            if let Some(p) = resolve(src).await {
                // The user may have picked something else meanwhile.
                picked.try_update(|cur| {
                    if cur.is_none() {
                        *cur = Some(p);
                    }
                });
            }
        });
    }
    let input = RwSignal::new(String::new());
    let query = RwSignal::new(String::new());
    let debounce = StoredValue::new(0u32);
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

    let local = LocalResource::new(list_images);
    let remote = LocalResource::new(move || catalog(query.get()));

    let tab_btn = move |label: &'static str, t: Source| {
        view! {
            <button
                type="button"
                aria-pressed=move || (tab.get() == t).to_string()
                class=move || {
                    if tab.get() == t {
                        "h-7 rounded px-3 text-[12px] font-medium bg-surface text-text shadow-sm"
                    } else {
                        "h-7 rounded px-3 text-[12px] text-text-2 hover:text-text"
                    }
                }
                on:click=move |_| tab.set(t)
            >
                {label}
            </button>
        }
    };

    let local_list = move || match local.get() {
        None => view! { <p class="px-2.5 py-3 text-[13px] text-muted">"Loading images..."</p> }.into_any(),
        Some(Err(e)) => view! { <p class="px-2.5 py-3 text-[13px] text-danger">{err_msg(&e)}</p> }.into_any(),
        Some(Ok(all)) => {
            let q = input.get().to_lowercase();
            let rows: Vec<ImageInfo> = all
                .into_iter()
                .filter(|i| {
                    q.is_empty()
                        || i.description.to_lowercase().contains(&q)
                        || i.aliases.iter().any(|a| a.to_lowercase().contains(&q))
                        || i.fingerprint.starts_with(&q)
                })
                .collect();
            if rows.is_empty() {
                return view! { <p class="px-2.5 py-3 text-[13px] text-muted">"No local images match."</p> }.into_any();
            }
            rows.into_iter()
                .map(|i| {
                    let p = Picked::from_local(&i);
                    let active = {
                        let s = p.source.clone();
                        move || picked.with(|c| c.as_ref().map(|c| &c.source) == Some(&s))
                    };
                    let (name, short) = (p.label.clone(), i.fingerprint.chars().take(12).collect::<String>());
                    let detail = format!("{} \u{b7} {} \u{b7} ", i.description, format_bytes(i.size));
                    let (c, v) = (p.container, p.vm);
                    view! {
                        <button type="button" class=move || row_class(active()) on:click=move |_| picked.set(Some(p.clone()))>
                            <span class="min-w-0">
                                <span class="block truncate text-[13px] font-medium text-text">{name}</span>
                                <span class="block truncate text-[12px] text-muted">{detail}<span class="mono">{short}</span></span>
                            </span>
                            {caps_badges(c, v)}
                        </button>
                    }
                })
                .collect_view()
                .into_any()
        }
    };

    let remote_list = move || match remote.get() {
        None => view! { <p class="px-2.5 py-3 text-[13px] text-muted">"Searching the catalog..."</p> }.into_any(),
        Some(Err(e)) => view! { <p class="px-2.5 py-3 text-[13px] text-danger">{err_msg(&e)}</p> }.into_any(),
        Some(Ok(rows)) if rows.is_empty() => {
            view! { <p class="px-2.5 py-3 text-[13px] text-muted">"No catalog images match."</p> }.into_any()
        }
        Some(Ok(rows)) => rows
            .into_iter()
            .map(|i| {
                let p = Picked::from_catalog(&i);
                let active = {
                    let s = p.source.clone();
                    move || picked.with(|c| c.as_ref().map(|c| &c.source) == Some(&s))
                };
                let (c, v) = (p.container, p.vm);
                view! {
                    <button type="button" class=move || row_class(active()) on:click=move |_| picked.set(Some(p.clone()))>
                        <span class="min-w-0">
                            <span class="block truncate text-[13px] font-medium text-text">{i.alias.clone()}</span>
                            <span class="block truncate text-[12px] text-muted">{i.description.clone()}</span>
                        </span>
                        {caps_badges(c, v)}
                    </button>
                }
            })
            .collect_view()
            .into_any(),
    };

    view! {
        <div class="space-y-2">
            <div class="flex items-center gap-3">
                <div class="flex gap-0.5 rounded-md border border-border bg-surface-2 p-0.5">
                    {tab_btn("Catalog", Source::Catalog)} {tab_btn("Local", Source::Local)}
                </div>
                <label class="relative min-w-0 flex-1">
                    <span class="sr-only">"Search images"</span>
                    <span class="pointer-events-none absolute left-2.5 top-2 text-muted">
                        <Icon name=IconName::Search class="size-4"/>
                    </span>
                    <input
                        type="search"
                        class="input h-8 w-full pl-8"
                        placeholder="Search images, e.g. ubuntu or alpine"
                        prop:value=move || input.get()
                        on:input=on_input
                    />
                </label>
            </div>
            <div class="h-52 overflow-y-auto rounded-md border border-border bg-bg p-1" role="listbox" aria-label="Images">
                {move || if tab.get() == Source::Local { local_list().into_any() } else { remote_list().into_any() }}
            </div>
        </div>
    }
}
