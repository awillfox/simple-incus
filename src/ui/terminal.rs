//! Terminal tab body. Placeholder until the terminal task replaces it.
use leptos::prelude::*;

use crate::ui::components::{EmptyState, IconName};

#[component]
pub fn TerminalTab(name: String) -> impl IntoView {
    view! {
        <EmptyState
            icon=IconName::Terminal
            title="Terminal: coming soon"
            body=format!("An interactive shell for {name} will be available here.")
        />
    }
}
