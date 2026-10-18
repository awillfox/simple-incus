//! Images page, "Local" tab: images cached on this host.
use std::collections::HashSet;

use leptos::ev::{KeyboardEvent, MouseEvent};
use leptos::prelude::*;
use leptos::task::spawn_local;

use super::actions::err_msg;
use super::live::{bump, live_tick};
use crate::model::{ImageInfo, ImageSource, InstanceKind};
use crate::server::images::{delete_image, list_images};
use crate::ui::components::{
    toast, use_create_drawer, Column, ConfirmModal, DataTable, EmptyState, Icon, IconName, ToastKind,
};
use crate::ui::format::{created_label, format_bytes, parse_rfc3339};

const POLL_MS: u64 = 15_000;

/// Page state the row action cells reach through context (column renderers
/// are plain `fn`s).
#[derive(Clone, Copy)]
struct RowCtx {
    busy: RwSignal<HashSet<String>>,
    launch: Callback<ImageInfo>,
    delete: Callback<ImageInfo>,
}

fn stop_click(e: MouseEvent) {
    e.stop_propagation();
}
fn stop_key(e: KeyboardEvent) {
    e.stop_propagation();
}

/// The aliases, or the short fingerprint for an image without any.
pub fn display_name(i: &ImageInfo) -> String {
    if i.aliases.is_empty() {
        short_fingerprint(&i.fingerprint)
    } else {
        i.aliases.join(", ")
    }
}

pub fn short_fingerprint(fp: &str) -> String {
    fp.chars().take(12).collect()
}

fn name_cell(i: &ImageInfo) -> AnyView {
    if i.aliases.is_empty() {
        return view! { <span class="text-muted">"\u{2014}"</span> }.into_any();
    }
    view! { <span class="mono text-[13px] font-medium text-text">{i.aliases.join(", ")}</span> }.into_any()
}

fn description_cell(i: &ImageInfo) -> AnyView {
    view! { <span class="block max-w-[420px] truncate text-text-2" title=i.description.clone()>{i.description.clone()}</span> }
        .into_any()
}

fn type_cell(i: &ImageInfo) -> AnyView {
    let label = match i.kind {
        InstanceKind::Container => "Container",
        InstanceKind::Vm => "VM",
    };
    view! { <span class="text-text-2">{label}</span> }.into_any()
}

fn size_cell(i: &ImageInfo) -> AnyView {
    view! { <span>{format_bytes(i.size)}</span> }.into_any()
}

fn fingerprint_cell(i: &ImageInfo) -> AnyView {
    view! { <span class="mono text-muted" title=i.fingerprint.clone()>{short_fingerprint(&i.fingerprint)}</span> }.into_any()
}

fn uploaded_cell(i: &ImageInfo) -> AnyView {
    view! { <span class="text-text-2" title=i.uploaded_at.clone()>{created_label(&i.uploaded_at)}</span> }.into_any()
}

fn actions_cell(i: &ImageInfo) -> AnyView {
    let ctx = use_context::<RowCtx>();
    let fp = i.fingerprint.clone();
    let busy = move || ctx.is_some_and(|c| c.busy.with(|b| b.contains(&fp)));
    let launch_img = i.clone();
    let delete_img = i.clone();
    let name = display_name(i);
    view! {
        <div class="flex items-center justify-end gap-1" on:click=stop_click on:keydown=stop_key>
            {
                let busy = busy.clone();
                move || busy().then(|| view! {
                    <span class="mr-1 text-accent" role="status" aria-label="Working">
                        <Icon name=IconName::Spinner class="size-3.5 animate-spin"/>
                    </span>
                })
            }
            <button
                type="button"
                class="btn h-7 px-2.5 text-[12px]"
                aria-label=format!("Launch {name}")
                disabled=busy.clone()
                on:click=move |_| {
                    if let Some(c) = ctx {
                        c.launch.run(launch_img.clone());
                    }
                }
            >
                <Icon name=IconName::Play class="size-3"/>
                "Launch"
            </button>
            <button
                type="button"
                title="Delete"
                aria-label=format!("Delete {name}")
                disabled=busy
                class="inline-flex size-7 items-center justify-center rounded-md text-muted transition-colors hover:bg-hover hover:text-danger focus-visible:bg-hover disabled:cursor-not-allowed disabled:opacity-40 disabled:hover:bg-transparent"
                on:click=move |_| {
                    if let Some(c) = ctx {
                        c.delete.run(delete_img.clone());
                    }
                }
            >
                <Icon name=IconName::Trash class="size-3.5"/>
            </button>
        </div>
    }
    .into_any()
}

fn columns() -> Vec<Column<ImageInfo>> {
    vec![
        Column::new("Image", name_cell).sortable(|a, b| display_name(a).to_lowercase().cmp(&display_name(b).to_lowercase())),
        Column::new("Description", description_cell).sortable(|a, b| a.description.cmp(&b.description)),
        Column::new("Type", type_cell).sortable(|a, b| (a.kind as u8).cmp(&(b.kind as u8))),
        Column::new("Size", size_cell).sortable(|a, b| a.size.cmp(&b.size)).numeric(),
        Column::new("Fingerprint", fingerprint_cell),
        Column::new("Uploaded", uploaded_cell)
            .sortable(|a, b| parse_rfc3339(&a.uploaded_at).cmp(&parse_rfc3339(&b.uploaded_at))),
        Column::new("", actions_cell).numeric().deps(|_| String::new()),
    ]
}

/// The image table (the action buttons are inert unless the page provides [`RowCtx`]).
#[component]
pub fn LocalImagesTable(
    #[prop(into)] rows: Signal<Vec<ImageInfo>>,
    #[prop(optional, into)] loading: Signal<bool>,
) -> impl IntoView {
    view! {
        <DataTable
            rows=rows
            columns=columns()
            row_key=|i: &ImageInfo| i.fingerprint.clone()
            search=|i: &ImageInfo| format!("{} {} {}", i.aliases.join(" "), i.description, i.fingerprint)
            placeholder="Filter by alias, description or fingerprint…"
            empty="No local images. Launching an instance from the catalog downloads one."
            loading=loading
        />
    }
}

/// Confirmation for deleting the image in `pending`.
#[component]
fn DeleteConfirm(pending: RwSignal<Option<ImageInfo>>, run: Callback<ImageInfo>) -> impl IntoView {
    move || {
        pending.get().map(|img| {
            let open = RwSignal::new(true);
            Effect::new(move |_| {
                if !open.get() {
                    pending.set(None);
                }
            });
            let name = display_name(&img);
            view! {
                <ConfirmModal
                    open=open
                    title=format!("Delete image {name}?")
                    body="The image is removed from this host. Instances created from it keep working; launching it again downloads it anew."
                    confirm_label="Delete"
                    danger=true
                    on_confirm=Callback::new(move |_| run.run(img.clone()))
                />
            }
        })
    }
}

#[component]
pub fn LocalImages() -> impl IntoView {
    let tick = live_tick(POLL_MS);
    let resource = LocalResource::new(move || {
        tick.get();
        list_images()
    });
    let rows = RwSignal::new(Vec::<ImageInfo>::new());
    let error = RwSignal::new(None::<String>);
    let loaded = RwSignal::new(false);
    let busy = RwSignal::new(HashSet::<String>::new());
    let pending = RwSignal::new(None::<ImageInfo>);

    // Keep the last good rows on screen when a refetch fails.
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

    let run_delete = Callback::new(move |img: ImageInfo| {
        let (fp, name) = (img.fingerprint.clone(), display_name(&img));
        busy.update(|b| {
            b.insert(fp.clone());
        });
        spawn_local(async move {
            let result = delete_image(fp.clone()).await;
            busy.try_update(|b| {
                b.remove(&fp);
            });
            match result {
                Ok(()) => {
                    toast(ToastKind::Success, format!("Image {name} deleted"));
                    bump(tick);
                }
                // e.g. the image is still in use by an instance
                Err(e) => toast(ToastKind::Error, err_msg(&e)),
            }
        });
    });

    let create = use_create_drawer();
    let launch = Callback::new(move |img: ImageInfo| {
        create.open_with(Some(ImageSource::Local { fingerprint: img.fingerprint }));
    });
    provide_context(RowCtx { busy, launch, delete: Callback::new(move |img| pending.set(Some(img))) });

    let list: Signal<Vec<ImageInfo>> = rows.into();
    let loading = Signal::derive(move || !loaded.get());
    let show_error = Memo::new(move |_| rows.with(Vec::is_empty) && error.with(Option::is_some));
    view! {
        {move || {
            if show_error.get() {
                view! {
                    <EmptyState icon=IconName::AlertCircle title="Can't load images" body=error.get().unwrap_or_default()>
                        <button type="button" class="btn" on:click=move |_| bump(tick)>
                            "Retry"
                        </button>
                    </EmptyState>
                }
                .into_any()
            } else {
                view! { <LocalImagesTable rows=list loading=loading/> }.into_any()
            }
        }}
        <DeleteConfirm pending=pending run=run_delete/>
    }
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;

    fn image(fp: &str, aliases: &[&str], kind: InstanceKind) -> ImageInfo {
        ImageInfo {
            fingerprint: fp.into(),
            aliases: aliases.iter().map(|s| s.to_string()).collect(),
            description: "Alpine 3.21 amd64 (20261016_13:00)".into(),
            kind,
            size: 3_367_796,
            uploaded_at: "2026-10-18T13:13:29Z".into(),
        }
    }

    #[test]
    fn local_images_table_lists_rows() {
        let owner = Owner::new();
        let html = owner.with(|| {
            let rows = RwSignal::new(vec![
                image("f17d529e9e668c848daf4216142a60739a4ec2e842469577272e867f043fb7cc", &[], InstanceKind::Container),
                image("aa11bb22cc33dd44", &["si-test-img"], InstanceKind::Vm),
            ]);
            view! { <LocalImagesTable rows=rows/> }.to_html()
        });
        // No alias: the name column is empty; the short fingerprint labels the actions.
        assert!(html.contains("f17d529e9e66"), "{html}");
        assert!(html.contains("si-test-img"), "{html}");
        assert!(html.contains("aria-label=\"Launch si-test-img\""), "{html}");
        assert!(html.contains("aria-label=\"Delete f17d529e9e66\""), "{html}");
        assert!(html.contains("3.2 MiB"), "{html}");
        assert!(html.contains("Alpine 3.21 amd64"), "{html}");
    }
}
