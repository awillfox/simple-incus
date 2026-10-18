//! Console tab body. Placeholder until the console task replaces it.
use leptos::prelude::*;

use crate::model::InstanceKind;
use crate::ui::components::{EmptyState, IconName};

#[component]
pub fn ConsoleTab(name: String, kind: InstanceKind) -> impl IntoView {
    let what = match kind {
        InstanceKind::Container => "console",
        InstanceKind::Vm => "VGA and serial console",
    };
    view! {
        <EmptyState
            icon=IconName::Terminal
            title="Console: coming soon"
            body=format!("The {what} of {name} will be available here.")
        />
    }
}
