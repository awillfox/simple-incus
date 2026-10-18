//! Horizontal usage meter (label, percentage, bar).
use leptos::prelude::*;

/// Percentage of `value` in `max`, clamped to 0..=100 (0 when `max` <= 0).
pub fn percent(value: f64, max: f64) -> f64 {
    if max > 0.0 && value.is_finite() {
        (value / max * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    }
}

#[component]
pub fn Meter(value: f64, max: f64, #[prop(into)] label: String) -> impl IntoView {
    let pct = percent(value, max);
    let bar = if pct >= 90.0 {
        "bg-danger"
    } else if pct >= 75.0 {
        "bg-warn"
    } else {
        "bg-accent"
    };
    view! {
        <div class="min-w-0">
            <div class="mb-1.5 flex items-baseline justify-between gap-3 text-[12px]">
                <span class="truncate text-muted">{label.clone()}</span>
                <span class="num font-medium text-text-2">{format!("{pct:.0}%")}</span>
            </div>
            <div
                class="h-1.5 overflow-hidden rounded-full bg-border"
                role="meter"
                aria-label=label
                aria-valuemin="0"
                aria-valuemax=max.to_string()
                aria-valuenow=value.to_string()
            >
                <div
                    class=format!("h-full rounded-full transition-[width] duration-300 {bar}")
                    style=format!("width: {pct:.1}%")
                ></div>
            </div>
        </div>
    }
}
