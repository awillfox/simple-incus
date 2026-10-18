//! Inline SVG icons (stroke style, 24x24 grid). No icon font or CDN.
use leptos::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconName {
    Overview,
    Instances,
    Images,
    Sun,
    Moon,
    Menu,
    Close,
    Search,
    CollapseLeft,
    ChevronUp,
    ChevronDown,
    ChevronsUpDown,
    CheckCircle,
    AlertCircle,
    AlertTriangle,
    Info,
    Inbox,
    Logo,
    Plus,
    Play,
    Stop,
    Restart,
    Pause,
    Trash,
    Spinner,
    Terminal,
    Camera,
    ArrowLeft,
}

fn paths(name: IconName) -> &'static str {
    match name {
        IconName::Overview => r#"<rect x="3" y="3" width="7" height="9" rx="1.5"/><rect x="14" y="3" width="7" height="5" rx="1.5"/><rect x="14" y="12" width="7" height="9" rx="1.5"/><rect x="3" y="16" width="7" height="5" rx="1.5"/>"#,
        IconName::Instances => r#"<rect x="3" y="4" width="18" height="7" rx="1.5"/><rect x="3" y="13" width="18" height="7" rx="1.5"/><path d="M7 7.5h.01M7 16.5h.01M11 7.5h2M11 16.5h2"/>"#,
        IconName::Images => r#"<path d="M12 3 3 7.5l9 4.5 9-4.5L12 3Z"/><path d="m3 12 9 4.5 9-4.5"/><path d="m3 16.5 9 4.5 9-4.5"/>"#,
        IconName::Sun => r#"<circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41"/>"#,
        IconName::Moon => r#"<path d="M20.5 14.1A8.5 8.5 0 1 1 9.9 3.5a6.6 6.6 0 0 0 10.6 10.6Z"/>"#,
        IconName::Menu => r#"<path d="M4 6h16M4 12h16M4 18h16"/>"#,
        IconName::Close => r#"<path d="M18 6 6 18M6 6l12 12"/>"#,
        IconName::Search => r#"<circle cx="11" cy="11" r="7"/><path d="m20 20-3.5-3.5"/>"#,
        IconName::CollapseLeft => r#"<rect x="3" y="3" width="18" height="18" rx="2"/><path d="M9 3v18M16 9l-3 3 3 3"/>"#,
        IconName::ChevronUp => r#"<path d="m7 14 5-5 5 5"/>"#,
        IconName::ChevronDown => r#"<path d="m7 10 5 5 5-5"/>"#,
        IconName::ChevronsUpDown => r#"<path d="m8 9 4-4 4 4M8 15l4 4 4-4"/>"#,
        IconName::CheckCircle => r#"<circle cx="12" cy="12" r="9"/><path d="m8.5 12.5 2.5 2.5 4.5-5"/>"#,
        IconName::AlertCircle => r#"<circle cx="12" cy="12" r="9"/><path d="M12 7.5v5M12 16.5h.01"/>"#,
        IconName::AlertTriangle => r#"<path d="M10.3 3.9 2.4 17.5A2 2 0 0 0 4.1 20.5h15.8a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0Z"/><path d="M12 9v4M12 17h.01"/>"#,
        IconName::Info => r#"<circle cx="12" cy="12" r="9"/><path d="M12 11v5.5M12 7.5h.01"/>"#,
        IconName::Inbox => r#"<path d="M22 12h-6l-2 3h-4l-2-3H2"/><path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11Z"/>"#,
        IconName::Logo => r#"<path d="M12 2.8 20 7.4v9.2l-8 4.6-8-4.6V7.4l8-4.6Z"/><path d="M4 7.4l8 4.6 8-4.6M12 12v9.2"/>"#,
        IconName::Plus => r#"<path d="M12 5v14M5 12h14"/>"#,
        IconName::Play => r#"<path d="M7 4.5v15l12-7.5-12-7.5Z"/>"#,
        IconName::Stop => r#"<rect x="6" y="6" width="12" height="12" rx="1.5"/>"#,
        IconName::Restart => r#"<path d="M20 11a8 8 0 1 0-2.3 5.7"/><path d="M20 4v7h-7"/>"#,
        IconName::Pause => r#"<path d="M8 5v14M16 5v14"/>"#,
        IconName::Trash => r#"<path d="M4 7h16M10 11v6M14 11v6M6 7l1 12a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2l1-12M9 7V4h6v3"/>"#,
        IconName::Terminal => r#"<rect x="3" y="4" width="18" height="16" rx="2"/><path d="m7 9 3 3-3 3M13 15h4"/>"#,
        IconName::Camera => r#"<path d="M4 8a2 2 0 0 1 2-2h1.5l1.2-2h6.6l1.2 2H18a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V8Z"/><circle cx="12" cy="13" r="3.5"/>"#,
        IconName::ArrowLeft => r#"<path d="M19 12H5M11 6l-6 6 6 6"/>"#,
        IconName::Spinner => r#"<path d="M12 3a9 9 0 1 0 9 9"/>"#,
    }
}

/// An inline SVG icon. `class` sets size/colour (defaults to 16 px, currentColor).
#[component]
pub fn Icon(name: IconName, #[prop(optional, into)] class: Option<String>) -> impl IntoView {
    let class = class.unwrap_or_else(|| "size-4".into());
    view! {
        <svg
            class=format!("shrink-0 {class}")
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.75"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
            inner_html=paths(name)
        ></svg>
    }
}
