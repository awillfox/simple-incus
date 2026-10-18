//! Route pages. Pages other than Instances are placeholders; later tasks replace their bodies.
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

mod instances;

pub use instances::InstancesPage;

use super::components::{EmptyState, IconName, PageHeader};

/// Shared page frame: centred content column with consistent gutters.
#[component]
pub fn PageFrame(children: Children) -> impl IntoView {
    view! { <div class="mx-auto w-full max-w-[1400px] px-4 py-5 md:px-6 md:py-6">{children()}</div> }
}

#[component]
pub fn OverviewPage() -> impl IntoView {
    view! {
        <PageFrame>
            <PageHeader title="Overview" description="Host capacity and instance health at a glance."/>
            <EmptyState
                icon=IconName::Overview
                title="Overview is on its way"
                body="Host CPU, memory and storage usage will appear here."
            />
        </PageFrame>
    }
}

#[component]
pub fn InstancePage() -> impl IntoView {
    let params = use_params_map();
    let name = move || params.with(|p| p.get("name").unwrap_or_default());
    view! {
        <PageFrame>
            {move || view! { <PageHeader title=name()/> }}
            <EmptyState
                icon=IconName::Instances
                title="Instance details are on their way"
                body="Status, metrics, console and snapshots will appear here."
            />
        </PageFrame>
    }
}

#[component]
pub fn ImagesPage() -> impl IntoView {
    view! {
        <PageFrame>
            <PageHeader title="Images" description="Local images and the remote image catalog."/>
            <EmptyState icon=IconName::Images title="No images to show yet" body="Local and catalog images will appear here."/>
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
