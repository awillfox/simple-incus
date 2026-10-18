//! Instance lifecycle actions shared by the list and the detail page.
use std::sync::Arc;

use leptos::prelude::*;

use crate::model::{Action, InstanceStatus};
use crate::server::instances::{delete_instance, instance_action};
use crate::ui::components::{ConfirmModal, IconName};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowAction {
    Start,
    Stop,
    Restart,
    Freeze,
    Unfreeze,
    Delete,
}

impl RowAction {
    /// Actions offered for an instance in `status`, in display order.
    pub fn available(status: &InstanceStatus) -> &'static [RowAction] {
        use RowAction::*;
        match status {
            InstanceStatus::Running => &[Stop, Restart, Freeze, Delete],
            InstanceStatus::Stopped => &[Start, Delete],
            InstanceStatus::Frozen => &[Unfreeze, Stop, Delete],
            _ => &[Delete],
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            RowAction::Start => "Start",
            RowAction::Stop => "Stop",
            RowAction::Restart => "Restart",
            RowAction::Freeze => "Freeze",
            RowAction::Unfreeze => "Unfreeze",
            RowAction::Delete => "Delete",
        }
    }

    /// Past tense used in the success toast.
    pub fn verb(self) -> &'static str {
        match self {
            RowAction::Start => "started",
            RowAction::Stop => "stopped",
            RowAction::Restart => "restarted",
            RowAction::Freeze => "frozen",
            RowAction::Unfreeze => "unfrozen",
            RowAction::Delete => "deleted",
        }
    }

    pub fn icon(self) -> IconName {
        match self {
            RowAction::Start | RowAction::Unfreeze => IconName::Play,
            RowAction::Stop => IconName::Stop,
            RowAction::Restart => IconName::Restart,
            RowAction::Freeze => IconName::Pause,
            RowAction::Delete => IconName::Trash,
        }
    }

    /// Stop, restart and delete ask for confirmation (with a Force option).
    pub fn needs_confirm(self) -> bool {
        matches!(self, RowAction::Stop | RowAction::Restart | RowAction::Delete)
    }

    /// The server action; `None` for delete, which has its own server fn.
    pub fn model(self, force: bool) -> Option<Action> {
        match self {
            RowAction::Start => Some(Action::Start),
            RowAction::Stop => Some(Action::Stop { force }),
            RowAction::Restart => Some(Action::Restart { force }),
            RowAction::Freeze => Some(Action::Freeze),
            RowAction::Unfreeze => Some(Action::Unfreeze),
            RowAction::Delete => None,
        }
    }
}

/// Runs `action` on the server (delete has its own server fn).
pub async fn perform(name: &str, action: RowAction, force: bool) -> Result<(), ServerFnError> {
    match action.model(force) {
        Some(a) => instance_action(name.to_string(), a).await.map(|_| ()),
        None => delete_instance(name.to_string(), force).await,
    }
}

pub fn err_msg(e: &ServerFnError) -> String {
    match e {
        ServerFnError::ServerError(m) => m.clone(),
        other => other.to_string(),
    }
}

/// Runs `f` once after `ms` (browser only; the server never needs it).
pub fn after_ms(ms: u64, f: impl FnOnce() + 'static) {
    #[cfg(feature = "hydrate")]
    set_timeout(f, std::time::Duration::from_millis(ms));
    #[cfg(not(feature = "hydrate"))]
    let _ = (ms, f);
}

#[cfg(feature = "hydrate")]
pub fn page_visible() -> bool {
    use leptos::web_sys::VisibilityState;
    leptos::prelude::document().visibility_state() == VisibilityState::Visible
}

/// Confirmation dialog for `pending` (stop / restart / delete, each with a
/// "force" option); calls `run(name, action, force)` on confirm.
#[component]
pub fn ActionConfirm(
    pending: RwSignal<Option<(String, RowAction)>>,
    run: Callback<(String, RowAction, bool)>,
) -> impl IntoView {
    move || {
        pending.get().map(|(name, action)| {
            let open = RwSignal::new(true);
            let force = RwSignal::new(false);
            Effect::new(move |_| {
                if !open.get() {
                    pending.set(None);
                }
            });
            let (title, body, label, force_label, danger) = match action {
                RowAction::Delete => (
                    format!("Delete {name}?"),
                    "This permanently removes the instance and all of its data. This cannot be undone.",
                    "Delete",
                    "Force (stop first)",
                    true,
                ),
                RowAction::Restart => (
                    format!("Restart {name}?"),
                    "The instance will be shut down and started again.",
                    "Restart",
                    "Force (do not wait for a clean shutdown)",
                    false,
                ),
                _ => (
                    format!("Stop {name}?"),
                    "The instance will be shut down.",
                    "Stop",
                    "Force (do not wait for a clean shutdown)",
                    false,
                ),
            };
            let extra: ChildrenFn = Arc::new(move || {
                view! {
                    <label class="flex cursor-pointer items-center gap-2 text-[13px] text-text-2">
                        <input
                            type="checkbox"
                            class="size-3.5 accent-[var(--accent)]"
                            prop:checked=move || force.get()
                            on:change=move |e| force.set(event_target_checked(&e))
                        />
                        {force_label}
                    </label>
                }
                .into_any()
            });
            view! {
                <ConfirmModal
                    open=open
                    title=title
                    body=body
                    confirm_label=label
                    danger=danger
                    extra=extra
                    on_confirm=Callback::new(move |_| run.run((name.clone(), action, force.get_untracked())))
                />
            }
        })
    }
}
