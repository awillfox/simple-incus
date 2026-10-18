//! Primary navigation. Collapsible to an icon rail on desktop; an off-canvas
//! panel (opened from the topbar) below the `md` breakpoint.
use leptos::prelude::*;
use leptos_router::hooks::use_location;

use super::icons::{Icon, IconName};

/// Sidebar UI state shared with the topbar.
#[derive(Clone, Copy)]
pub struct NavState {
    /// Desktop: icon-only rail.
    pub collapsed: RwSignal<bool>,
    /// Mobile: off-canvas panel visible.
    pub mobile_open: RwSignal<bool>,
}

pub fn provide_nav_state() -> NavState {
    let s = NavState { collapsed: RwSignal::new(false), mobile_open: RwSignal::new(false) };
    provide_context(s);
    s
}

fn nav_state() -> NavState {
    use_context::<NavState>().expect("provide_nav_state() must be called from App")
}

/// `(href, label, icon)` of every top-level page, in sidebar order.
pub const NAV_ITEMS: [(&str, &str, IconName); 3] = [
    ("/", "Overview", IconName::Overview),
    ("/instances", "Instances", IconName::Instances),
    ("/images", "Images", IconName::Images),
];

/// Whether `href` is the active section for `path`.
pub fn is_active(href: &str, path: &str) -> bool {
    if href == "/" {
        path == "/" || path.is_empty()
    } else {
        path == href || path.starts_with(&format!("{href}/"))
    }
}

#[component]
pub fn Sidebar() -> impl IntoView {
    let nav = nav_state();
    let location = use_location();
    let collapsed = nav.collapsed;
    let mobile_open = nav.mobile_open;
    let hide_when_collapsed = move || if collapsed.get() { "md:hidden" } else { "" };

    view! {
        <Show when=move || mobile_open.get()>
            <div class="anim-fade-in fixed inset-0 z-30 bg-overlay md:hidden" on:click=move |_| mobile_open.set(false)></div>
        </Show>
        <aside
            class=move || {
                format!(
                    "fixed inset-y-0 left-0 z-40 flex w-[248px] shrink-0 flex-col border-r border-border bg-surface transition-[width,transform] duration-150 ease-out md:static md:translate-x-0 {} {}",
                    if collapsed.get() { "md:w-[56px]" } else { "md:w-[232px]" },
                    if mobile_open.get() { "translate-x-0" } else { "-translate-x-full" },
                )
            }
            aria-label="Primary"
        >
            <div class="flex h-12 shrink-0 items-center gap-2.5 border-b border-border px-4">
                <span class="flex size-6 shrink-0 items-center justify-center rounded-md bg-accent text-accent-fg">
                    <Icon name=IconName::Logo class="size-3.5"/>
                </span>
                <span class=move || format!("truncate text-[14px] font-semibold tracking-[-0.01em] text-text {}", hide_when_collapsed())>
                    "simple-incus"
                </span>
                <button
                    type="button"
                    class="btn btn-ghost btn-icon ml-auto size-7 md:hidden"
                    aria-label="Close navigation"
                    on:click=move |_| mobile_open.set(false)
                >
                    <Icon name=IconName::Close/>
                </button>
            </div>
            <nav class="flex-1 overflow-y-auto px-2 py-3">
                <div class=move || format!("px-2 pb-1.5 text-[11px] font-medium uppercase tracking-[0.06em] text-muted {}", hide_when_collapsed())>
                    "Compute"
                </div>
                <ul class="flex flex-col gap-0.5">
                    {NAV_ITEMS
                        .into_iter()
                        .map(|(href, label, icon)| {
                            let active = move || is_active(href, &location.pathname.get());
                            view! {
                                <li>
                                    <a
                                        href=href
                                        title=label
                                        aria-current=move || active().then_some("page")
                                        on:click=move |_| mobile_open.set(false)
                                        class=move || {
                                            format!(
                                                "group relative flex h-8 items-center gap-2.5 rounded-md px-2 text-[13px] font-medium transition-colors {}",
                                                if active() {
                                                    "bg-accent-soft text-text"
                                                } else {
                                                    "text-text-2 hover:bg-hover hover:text-text"
                                                },
                                            )
                                        }
                                    >
                                        <span class=move || if active() { "text-accent" } else { "text-muted group-hover:text-text-2" }>
                                            <Icon name=icon/>
                                        </span>
                                        <span class=move || format!("truncate {}", hide_when_collapsed())>{label}</span>
                                    </a>
                                </li>
                            }
                        })
                        .collect_view()}
                </ul>
            </nav>
            <div class="hidden shrink-0 items-center border-t border-border px-2 py-2 md:flex">
                <button
                    type="button"
                    class="btn btn-ghost h-8 w-full justify-start gap-2.5 px-2 text-[12px] text-muted"
                    aria-label=move || if collapsed.get() { "Expand sidebar" } else { "Collapse sidebar" }
                    title=move || if collapsed.get() { "Expand sidebar" } else { "Collapse sidebar" }
                    on:click=move |_| collapsed.update(|c| *c = !*c)
                >
                    <span class=move || if collapsed.get() { "rotate-180 transition-transform" } else { "transition-transform" }>
                        <Icon name=IconName::CollapseLeft/>
                    </span>
                    <span class=hide_when_collapsed>"Collapse"</span>
                </button>
            </div>
        </aside>
    }
}

#[cfg(test)]
mod tests {
    use super::is_active;

    #[test]
    fn active_matching() {
        assert!(is_active("/", "/"));
        assert!(!is_active("/", "/instances"));
        assert!(is_active("/instances", "/instances"));
        assert!(is_active("/instances", "/instances/web/console"));
        assert!(!is_active("/images", "/imagesx"));
    }
}
