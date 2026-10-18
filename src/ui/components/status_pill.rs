//! Instance status as a dot + label pill.
use leptos::prelude::*;

use crate::model::InstanceStatus;

/// Classes and label for a status (split out for testing / reuse).
pub fn status_style(status: &InstanceStatus) -> (&'static str, &'static str, String) {
    match status {
        InstanceStatus::Running => ("text-ok bg-ok/10 border-ok/25", "bg-ok", "Running".into()),
        InstanceStatus::Stopped => ("text-text-2 bg-muted/10 border-border-strong", "bg-muted", "Stopped".into()),
        InstanceStatus::Frozen => ("text-info bg-info/10 border-info/25", "bg-info", "Frozen".into()),
        InstanceStatus::Error => ("text-danger bg-danger/10 border-danger/25", "bg-danger", "Error".into()),
        InstanceStatus::Other(s) => ("text-warn bg-warn/10 border-warn/25", "bg-warn", s.clone()),
    }
}

#[component]
pub fn StatusPill(status: InstanceStatus) -> impl IntoView {
    let (pill, dot, label) = status_style(&status);
    view! {
        <span class=format!(
            "inline-flex h-[22px] items-center gap-1.5 whitespace-nowrap rounded-full border px-2 text-[12px] font-medium leading-none {pill}"
        )>
            <span class=format!("size-1.5 rounded-full {dot}") aria-hidden="true"></span>
            {label}
        </span>
    }
}
