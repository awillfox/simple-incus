//! Create-instance drawer. Mounted once in `App`; opened through the shared
//! `CreateDrawerState` (Instances "Create instance", Images "Launch").
//!
//! Submitting returns as soon as Incus accepts the request. The drawer then
//! closes and a progress toast follows the Incus operation on the event
//! stream, ending in navigation to the new instance or a sticky error.
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_navigate;

use super::actions::err_msg;
use super::create_image::{ImagePicker, Picked};
use crate::model::{validate_name, CreateRequest, ImageSource, InstanceKind, UiEvent, UiEventKind};
use crate::server::create::{create_instance, create_options};
use crate::ui::components::{
    toast, toast_busy, toast_finish, toast_set_text, use_create_drawer, Drawer, Icon, IconName, ToastKind,
};
use crate::ui::events::use_events;

/// What a user must leave in the textarea for it to count as "unchanged".
pub const CLOUD_INIT_TEMPLATE: &str = "#cloud-config\n";

/// Operation events kept so a result that beats the server fn response is not lost.
const SEEN_MAX: usize = 32;

/// How often running creates are checked directly. Events can be missed (an
/// outage, or a slow browser lagging the broadcast), so they alone can't be
/// relied on to end a create.
#[cfg(feature = "hydrate")]
const STATUS_POLL_MS: u64 = 5000;

/// How an Incus operation event reads for the create toast.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Progress(String),
    Done,
    Failed(String),
}

pub fn outcome(ev: &UiEvent) -> Outcome {
    match ev.status.as_deref() {
        Some("Success") => return Outcome::Done,
        Some("Failure") | Some("Cancelled") => {
            let why = ev.message.split_once(" failed: ").map(|(_, e)| e.to_string());
            return Outcome::Failed(why.unwrap_or_else(|| ev.message.clone()));
        }
        _ => {}
    }
    if let Some((_, why)) = ev.message.split_once(" failed: ") {
        return Outcome::Failed(why.to_string());
    }
    Outcome::Progress(ev.progress.clone().unwrap_or_else(|| ev.message.clone()))
}

/// Raw form values, as typed.
#[derive(Debug, Clone)]
pub struct FormValues {
    pub name: String,
    pub image: Option<ImageSource>,
    pub kind: InstanceKind,
    pub cpu: String,
    pub memory: String,
    pub disk: String,
    pub profiles: Vec<String>,
    pub network: String,
    pub pool: String,
    pub cloud_init: String,
    pub start: bool,
}

/// `Ok(None)` for blank, an error for anything that is not a positive whole number.
pub fn parse_positive(s: &str) -> Result<Option<u64>, String> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(None);
    }
    match s.parse::<u64>() {
        Ok(n) if n > 0 => Ok(Some(n)),
        _ => Err("Enter a whole number greater than 0".to_string()),
    }
}

pub fn build_request(v: &FormValues) -> Result<CreateRequest, String> {
    validate_name(&v.name)?;
    let image = v.image.clone().ok_or("Choose an image")?;
    if v.profiles.is_empty() {
        return Err("Select at least one profile".to_string());
    }
    let cpu = parse_positive(&v.cpu)?;
    let cpu = match cpu {
        Some(n) => Some(u32::try_from(n).map_err(|_| "CPU count is too large".to_string())?),
        None => None,
    };
    let some = |s: &str| Some(s.trim().to_string()).filter(|s| !s.is_empty());
    let changed = v.cloud_init.trim() != CLOUD_INIT_TEMPLATE.trim();
    Ok(CreateRequest {
        name: v.name.clone(),
        image,
        kind: v.kind,
        cpu,
        memory_mib: parse_positive(&v.memory)?,
        disk_gib: parse_positive(&v.disk)?,
        profiles: v.profiles.clone(),
        network: some(&v.network),
        pool: some(&v.pool),
        cloud_init: changed.then(|| v.cloud_init.clone()),
        start: v.start,
    })
}

/// An operation we are following.
#[derive(Clone)]
struct Tracked {
    op_id: String,
    name: String,
    // Read only by the browser's status polling.
    #[cfg_attr(not(feature = "hydrate"), allow(dead_code))]
    start: bool,
    toast: Option<u64>,
}

/// Applies one operation event to a tracked create; returns true when finished.
fn apply(t: &Tracked, ev: &UiEvent, navigate: Callback<String>) -> bool {
    apply_outcome(t, outcome(ev), navigate)
}

fn apply_outcome(t: &Tracked, result: Outcome, navigate: Callback<String>) -> bool {
    let id = t.toast;
    match result {
        Outcome::Progress(text) => {
            if let Some(id) = id {
                toast_set_text(id, format!("Creating {}: {text}", t.name));
            }
            false
        }
        Outcome::Done => {
            let msg = format!("Created {}", t.name);
            match id {
                Some(id) => toast_finish(id, ToastKind::Success, msg),
                None => toast(ToastKind::Success, msg),
            }
            navigate.run(format!("/instances/{}", t.name));
            true
        }
        Outcome::Failed(why) => {
            let msg = format!("Creating {} failed: {why}", t.name);
            match id {
                Some(id) => toast_finish(id, ToastKind::Error, msg),
                None => toast(ToastKind::Error, msg),
            }
            true
        }
    }
}

/// The drawer plus the tracker of running creates. Mount once, inside the router.
#[component]
pub fn CreateDrawer() -> impl IntoView {
    let state = use_create_drawer();
    let tracked = StoredValue::new(Vec::<Tracked>::new());
    let seen = StoredValue::new(Vec::<UiEvent>::new());
    let navigate = use_navigate();
    let go = Callback::new(move |path: String| navigate(&path, Default::default()));

    let events = use_events();
    Effect::new(move |_| {
        let Some(ev) = events.get() else { return };
        if ev.kind != UiEventKind::Operation || ev.op_id.is_none() {
            return;
        }
        seen.try_update_value(|s| {
            s.retain(|e| e.op_id != ev.op_id);
            s.push(ev.clone());
            if s.len() > SEEN_MAX {
                s.remove(0);
            }
        });
        let hit = tracked.try_with_value(|l| l.iter().find(|t| Some(&t.op_id) == ev.op_id.as_ref()).cloned());
        if let Some(Some(t)) = hit {
            if apply(&t, &ev, go) {
                tracked.try_update_value(|l| l.retain(|x| x.op_id != t.op_id));
            }
        }
    });

    // Events can be missed, so also ask where each running create stands.
    #[cfg(feature = "hydrate")]
    if let Ok(handle) = set_interval_with_handle(
        move || {
            for t in tracked.try_get_value().unwrap_or_default() {
                spawn_local(async move {
                    use crate::model::CreateStatus;
                    let status = crate::server::create::create_status(t.op_id.clone(), t.name.clone(), t.start);
                    let result = match status.await {
                        Ok(CreateStatus::Done) => Outcome::Done,
                        Ok(CreateStatus::Failed(why)) => Outcome::Failed(why),
                        // Still running, or the check failed: try again on the next tick.
                        Ok(CreateStatus::Running) | Err(_) => return,
                    };
                    // An event may have finished it while we were asking.
                    let live = tracked.try_with_value(|l| l.iter().any(|x| x.op_id == t.op_id)).unwrap_or(false);
                    if live {
                        tracked.try_update_value(|l| l.retain(|x| x.op_id != t.op_id));
                        apply_outcome(&t, result, go);
                    }
                });
            }
        },
        std::time::Duration::from_millis(STATUS_POLL_MS),
    ) {
        on_cleanup(move || handle.clear());
    }

    // Called by the form once Incus accepted the request.
    let started = Callback::new(move |(op_id, name, start): (String, String, bool)| {
        let t = Tracked { toast: toast_busy(format!("Creating {name}...")), op_id: op_id.clone(), name, start };
        // The operation may already have reported before we knew its id.
        let early = seen.try_with_value(|s| s.iter().find(|e| e.op_id.as_deref() == Some(&op_id)).cloned());
        match early.flatten() {
            Some(ev) if apply(&t, &ev, go) => {}
            _ => {
                tracked.try_update_value(|l| l.push(t));
            }
        }
    });

    view! {
        <Drawer open=state.open title="Create instance" width="sm:w-[700px]">
            <CreateForm started=started/>
        </Drawer>
    }
}

fn section(title: &'static str) -> impl IntoView {
    view! { <h3 class="mb-3 text-[11px] font-semibold uppercase tracking-wider text-muted">{title}</h3> }
}

#[component]
fn Field(
    label: &'static str,
    #[prop(optional, into)] error: Option<Signal<Option<String>>>,
    #[prop(optional)] hint: Option<&'static str>,
    children: Children,
) -> impl IntoView {
    view! {
        <label class="block min-w-0">
            <span class="mb-1 block text-[12px] font-medium text-text-2">{label}</span>
            {children()}
            {move || {
                match error.and_then(|e| e.get()) {
                    Some(m) => view! { <span class="mt-1 block text-[12px] text-danger" role="alert">{m}</span> }.into_any(),
                    None => hint
                        .map(|h| view! { <span class="mt-1 block text-[12px] text-muted">{h}</span> }.into_any())
                        .unwrap_or_else(|| ().into_any()),
                }
            }}
        </label>
    }
}

fn kind_class(active: bool) -> &'static str {
    if active {
        "h-8 flex-1 rounded px-3 text-[13px] font-medium bg-surface text-text shadow-sm"
    } else {
        "h-8 flex-1 rounded px-3 text-[13px] text-text-2 hover:text-text disabled:opacity-40 disabled:hover:text-text-2"
    }
}

#[component]
fn CreateForm(started: Callback<(String, String, bool)>) -> impl IntoView {
    let state = use_create_drawer();
    let prefill = state.prefill.get_untracked();

    let name = RwSignal::new(String::new());
    let touched = RwSignal::new(false);
    let picked = RwSignal::new(None::<Picked>);
    let kind = RwSignal::new(InstanceKind::Container);
    let cpu = RwSignal::new(String::new());
    let memory = RwSignal::new(String::new());
    let disk = RwSignal::new(String::new());
    let profiles = RwSignal::new(vec!["default".to_string()]);
    let network = RwSignal::new(String::new());
    let pool = RwSignal::new(String::new());
    let cloud = RwSignal::new(CLOUD_INIT_TEMPLATE.to_string());
    let start = RwSignal::new(true);
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

    // An image that only runs as one kind fixes the type toggle.
    Effect::new(move |_| {
        if let Some(p) = picked.get() {
            let cur = kind.get_untracked();
            let next = p.default_kind(cur);
            if next != cur {
                kind.set(next);
            }
        }
    });

    let options = LocalResource::new(create_options);

    let name_error = Signal::derive(move || {
        let n = name.get();
        if n.is_empty() && !touched.get() {
            None
        } else {
            validate_name(&n).err()
        }
    });
    let num_error = move |s: RwSignal<String>| Signal::derive(move || parse_positive(&s.get()).err());
    let (cpu_err, mem_err, disk_err) = (num_error(cpu), num_error(memory), num_error(disk));

    let values = move || FormValues {
        name: name.get(),
        image: picked.get().map(|p| p.source),
        kind: kind.get(),
        cpu: cpu.get(),
        memory: memory.get(),
        disk: disk.get(),
        profiles: profiles.get(),
        network: network.get(),
        pool: pool.get(),
        cloud_init: cloud.get(),
        start: start.get(),
    };
    let valid = Memo::new(move |_| build_request(&values()).is_ok());

    let submit = move |e: leptos::ev::SubmitEvent| {
        e.prevent_default();
        touched.set(true);
        let Ok(req) = build_request(&values()) else { return };
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        error.set(None);
        let (name, wants_start) = (req.name.clone(), req.start);
        spawn_local(async move {
            match create_instance(req).await {
                Ok(op_id) => {
                    started.run((op_id, name, wants_start));
                    // Only if this form is still mounted: the user may have
                    // cancelled and reopened the drawer for another instance.
                    if busy.try_get_untracked().is_some() {
                        state.open.try_set(false);
                    }
                }
                Err(e) => {
                    error.try_set(Some(err_msg(&e)));
                }
            }
            busy.try_set(false);
        });
    };

    let kind_btn = move |label: &'static str, k: InstanceKind| {
        let supported = move || picked.with(|p| p.as_ref().is_none_or(|p| p.supports(k)));
        view! {
            <button
                type="button"
                class=move || kind_class(kind.get() == k)
                aria-pressed=move || (kind.get() == k).to_string()
                disabled=move || !supported()
                on:click=move |_| kind.set(k)
            >
                {label}
            </button>
        }
    };

    let select = move |value: RwSignal<String>, first: &'static str, items: Vec<String>| {
        view! {
            <select class="input w-full" prop:value=move || value.get() on:change=move |e| value.set(event_target_value(&e))>
                <option value="">{first}</option>
                {items.into_iter().map(|i| view! { <option value=i.clone()>{i.clone()}</option> }).collect_view()}
            </select>
        }
    };

    let profile_chips = move || {
        options.get().and_then(Result::ok).map(|o| {
            o.profiles
                .into_iter()
                .map(|p| {
                    let on = {
                        let p = p.clone();
                        Memo::new(move |_| profiles.with(|l| l.contains(&p)))
                    };
                    let toggle = {
                        let p = p.clone();
                        move |_| {
                            profiles.update(|l| match l.iter().position(|x| *x == p) {
                                Some(i) => {
                                    l.remove(i);
                                }
                                None => l.push(p.clone()),
                            })
                        }
                    };
                    view! {
                        <button
                            type="button"
                            aria-pressed=move || on.get().to_string()
                            class=move || {
                                if on.get() {
                                    "h-7 rounded-full border border-accent bg-accent-soft px-3 text-[12px] font-medium text-text"
                                } else {
                                    "h-7 rounded-full border border-border-strong px-3 text-[12px] text-text-2 hover:bg-hover"
                                }
                            }
                            on:click=toggle
                        >
                            {p}
                        </button>
                    }
                })
                .collect_view()
        })
    };

    view! {
        <form class="flex min-h-full flex-col" on:submit=submit novalidate>
            <div class="flex-1 space-y-7">
                <section aria-label="Basics">
                    {section("Basics")}
                    <div class="space-y-4">
                        <Field label="Name" error=name_error hint="Letters, digits and hyphens; starts with a letter.">
                            <input
                                class="input w-full"
                                type="text"
                                autocomplete="off"
                                spellcheck="false"
                                placeholder="e.g. web1"
                                aria-invalid=move || name_error.get().is_some().to_string()
                                prop:value=move || name.get()
                                on:input=move |e| {
                                    name.set(event_target_value(&e));
                                    touched.set(true);
                                }
                            />
                        </Field>
                        <div>
                            <span class="mb-1 block text-[12px] font-medium text-text-2">"Image"</span>
                            <ImagePicker picked=picked initial=prefill/>
                            <p class="mt-1.5 text-[12px] text-muted">
                                {move || match picked.get() {
                                    Some(p) => format!("Selected: {}", p.label),
                                    None => "No image selected.".to_string(),
                                }}
                            </p>
                        </div>
                        <div>
                            <span class="mb-1 block text-[12px] font-medium text-text-2">"Type"</span>
                            <div class="flex gap-0.5 rounded-md border border-border bg-surface-2 p-0.5">
                                {kind_btn("Container", InstanceKind::Container)}
                                {kind_btn("Virtual machine", InstanceKind::Vm)}
                            </div>
                        </div>
                    </div>
                </section>

                <section aria-label="Resources">
                    {section("Resources")}
                    <div class="grid grid-cols-3 gap-4">
                        <Field label="CPU cores" error=cpu_err>
                            <input class="input num w-full" inputmode="numeric" placeholder="Profile default"
                                prop:value=move || cpu.get() on:input=move |e| cpu.set(event_target_value(&e))/>
                        </Field>
                        <Field label="Memory (MiB)" error=mem_err>
                            <input class="input num w-full" inputmode="numeric" placeholder="Profile default"
                                prop:value=move || memory.get() on:input=move |e| memory.set(event_target_value(&e))/>
                        </Field>
                        <Field label="Disk (GiB)" error=disk_err>
                            <input class="input num w-full" inputmode="numeric" placeholder="Profile default"
                                prop:value=move || disk.get() on:input=move |e| disk.set(event_target_value(&e))/>
                        </Field>
                    </div>
                </section>

                <section aria-label="Network and storage">
                    {section("Network & Storage")}
                    {move || {
                        options.get().and_then(Result::err).map(|e| {
                            view! {
                                <p class="mb-3 text-[12px] text-danger" role="alert">
                                    {format!("Could not load profiles, networks and pools: {}. Close and reopen this panel to retry.", err_msg(&e))}
                                </p>
                            }
                        })
                    }}
                    <div class="space-y-4">
                        <div>
                            <span class="mb-1 block text-[12px] font-medium text-text-2">"Profiles"</span>
                            <div class="flex flex-wrap gap-2">{profile_chips}</div>
                            {move || {
                                profiles.with(|l| l.is_empty()).then(|| {
                                    view! { <p class="mt-1.5 text-[12px] text-danger" role="alert">"Select at least one profile."</p> }
                                })
                            }}
                        </div>
                        <div class="grid grid-cols-2 gap-4">
                            <Field label="Network" hint="Managed networks only.">
                                {move || select(network, "Profile default", options.get().and_then(Result::ok).map(|o| o.networks).unwrap_or_default())}
                            </Field>
                            <Field label="Storage pool">
                                {move || select(pool, "Profile default", options.get().and_then(Result::ok).map(|o| o.pools).unwrap_or_default())}
                            </Field>
                        </div>
                    </div>
                </section>

                <section aria-label="Cloud-init">
                    {section("Cloud-init")}
                    <Field label="User data" hint="Sent only when changed from the template.">
                        <textarea
                            class="input mono h-40 w-full resize-y py-2 leading-relaxed"
                            spellcheck="false"
                            prop:value=move || cloud.get()
                            on:input=move |e| cloud.set(event_target_value(&e))
                        ></textarea>
                    </Field>
                </section>
            </div>

            <div class="sticky -bottom-5 -mx-5 -mb-5 mt-6 border-t border-border bg-surface px-5 py-3">
                {move || {
                    error
                        .get()
                        .map(|m| {
                            view! {
                                <p class="mb-2 flex items-start gap-2 text-[13px] text-danger" role="alert">
                                    <Icon name=IconName::AlertCircle class="mt-0.5 size-4 shrink-0"/>
                                    <span class="min-w-0 break-words">{m}</span>
                                </p>
                            }
                        })
                }}
                <div class="flex items-center justify-between gap-3">
                    <label class="flex items-center gap-2 text-[13px] text-text-2">
                        <input type="checkbox" prop:checked=move || start.get() on:change=move |e| start.set(event_target_checked(&e))/>
                        "Start after creation"
                    </label>
                    <div class="flex gap-2">
                        <button type="button" class="btn" on:click=move |_| state.open.set(false)>"Cancel"</button>
                        <button type="submit" class="btn btn-primary" disabled=move || !valid.get() || busy.get()>
                            {move || if busy.get() { "Creating..." } else { "Create instance" }}
                        </button>
                    </div>
                </div>
            </div>
        </form>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values() -> FormValues {
        FormValues {
            name: "si-test-c".into(),
            image: Some(ImageSource::Catalog { alias: "alpine/3.21".into() }),
            kind: InstanceKind::Container,
            cpu: String::new(),
            memory: String::new(),
            disk: String::new(),
            profiles: vec!["default".into()],
            network: String::new(),
            pool: String::new(),
            cloud_init: CLOUD_INIT_TEMPLATE.into(),
            start: true,
        }
    }

    fn ev(status: Option<&str>, progress: Option<&str>, message: &str) -> UiEvent {
        UiEvent {
            kind: UiEventKind::Operation,
            instance: Some("si-test-c".into()),
            op_id: Some("op".into()),
            progress: progress.map(str::to_string),
            status: status.map(str::to_string),
            message: message.into(),
        }
    }

    #[test]
    fn invalid_names_block_the_request() {
        let mut v = values();
        assert!(build_request(&v).is_ok());
        for bad in ["", "1abc", "abc-", "a_b", "123"] {
            v.name = bad.into();
            assert!(build_request(&v).is_err(), "{bad}");
        }
    }

    #[test]
    fn needs_an_image_and_sane_numbers() {
        let mut v = values();
        v.image = None;
        assert!(build_request(&v).is_err());
        let mut v = values();
        v.cpu = "0".into();
        assert!(build_request(&v).is_err());
        v.cpu = "two".into();
        assert!(build_request(&v).is_err());
        v.cpu = " 2 ".into();
        v.memory = "2048".into();
        v.disk = "10".into();
        let r = build_request(&v).unwrap();
        assert_eq!((r.cpu, r.memory_mib, r.disk_gib), (Some(2), Some(2048), Some(10)));
    }

    #[test]
    fn at_least_one_profile_is_required() {
        let mut v = values();
        v.profiles.clear();
        assert!(build_request(&v).is_err());
    }

    #[test]
    fn cloud_init_only_when_changed() {
        let mut v = values();
        assert_eq!(build_request(&v).unwrap().cloud_init, None);
        v.cloud_init = "#cloud-config\npackages: [htop]\n".into();
        assert_eq!(build_request(&v).unwrap().cloud_init.as_deref(), Some("#cloud-config\npackages: [htop]\n"));
    }

    #[test]
    fn blank_overrides_are_none() {
        let mut v = values();
        v.network = "  ".into();
        v.pool = "tank".into();
        let r = build_request(&v).unwrap();
        assert_eq!((r.network, r.pool.as_deref()), (None, Some("tank")));
    }

    #[test]
    fn operation_outcomes() {
        assert_eq!(
            outcome(&ev(Some("Running"), Some("rootfs: 45% (12.3MB/s)"), "Creating instance")),
            Outcome::Progress("rootfs: 45% (12.3MB/s)".into())
        );
        assert_eq!(outcome(&ev(Some("Running"), None, "Creating instance")), Outcome::Progress("Creating instance".into()));
        assert_eq!(outcome(&ev(Some("Success"), None, "Creating instance")), Outcome::Done);
        assert_eq!(
            outcome(&ev(Some("Failure"), None, "Creating instance failed: Image not found")),
            Outcome::Failed("Image not found".into())
        );
    }
}
