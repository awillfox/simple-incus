//! Route pages.
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

mod actions;
mod instance_detail;
mod instance_metrics;
mod images_catalog;
mod images_local;
mod instances;
mod live;
mod overview;

pub use instance_detail::InstancePage;
pub use instances::InstancesPage;
pub use overview::OverviewPage;

use images_catalog::CatalogImages;
use images_local::LocalImages;

use super::components::{EmptyState, IconName, PageHeader, Tabs};

/// Shared page frame: centred content column with consistent gutters.
#[component]
pub fn PageFrame(children: Children) -> impl IntoView {
    view! { <div class="mx-auto w-full max-w-[1400px] px-4 py-5 md:px-6 md:py-6">{children()}</div> }
}

#[component]
pub fn ImagesPage() -> impl IntoView {
    let params = use_params_map();
    let tab = Memo::new(move |_| params.with(|p| if p.get("tab").as_deref() == Some("catalog") { "catalog" } else { "local" }));
    let tabs = vec![("local".to_string(), "Local".to_string()), ("catalog".to_string(), "Catalog".to_string())];
    let active = Signal::derive(move || tab.get().to_string());
    view! {
        <PageFrame>
            <PageHeader title="Images" description="Images cached on this host and the remote image catalog."/>
            <Tabs tabs=tabs active=active base=Signal::stored("/images".to_string())/>
            <div class="mt-5">
                {move || match tab.get() {
                    "catalog" => view! { <CatalogImages/> }.into_any(),
                    _ => view! { <LocalImages/> }.into_any(),
                }}
            </div>
        </PageFrame>
    }
}

#[component]
pub fn NotFoundPage() -> impl IntoView {
    view! {
        <PageFrame>
            <EmptyState icon=IconName::AlertCircle title="Page not found" body="The page you asked for does not exist.">
                <a href="/" class="btn">"Go to Overview"</a>
            </EmptyState>
        </PageFrame>
    }
}
