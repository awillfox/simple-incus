//! Theme persistence. The active theme lives in `<html data-theme>`; the
//! choice is stored in `localStorage["si-theme"]`. Every browser access is
//! fallible and falls back to dark.
use leptos::prelude::*;

pub const STORAGE_KEY: &str = "si-theme";

/// Runs in `<head>` before first paint so the page never flashes the wrong
/// theme. `?theme=light|dark` overrides for one page view (not persisted).
pub const BOOTSTRAP_SCRIPT: &str = "(function(){var t='dark';\
try{if(localStorage.getItem('si-theme')==='light')t='light'}catch(e){}\
try{var q=new URLSearchParams(location.search).get('theme');if(q==='light'||q==='dark')t=q}catch(e){}\
document.documentElement.setAttribute('data-theme',t)})()";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Dark,
    Light,
}

impl Theme {
    pub fn as_str(self) -> &'static str {
        match self {
            Theme::Dark => "dark",
            Theme::Light => "light",
        }
    }
    pub fn toggled(self) -> Theme {
        match self {
            Theme::Dark => Theme::Light,
            Theme::Light => Theme::Dark,
        }
    }
}

/// The theme currently applied to the document (dark on the server).
pub fn current() -> Theme {
    #[cfg(feature = "hydrate")]
    {
        let attr = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.document_element())
            .and_then(|e| e.get_attribute("data-theme"));
        if attr.as_deref() == Some("light") {
            return Theme::Light;
        }
    }
    Theme::Dark
}

/// Applies `theme` to the document and persists it (errors ignored).
pub fn apply(theme: Theme) {
    #[cfg(feature = "hydrate")]
    {
        if let Some(w) = web_sys::window() {
            if let Some(el) = w.document().and_then(|d| d.document_element()) {
                let _ = el.set_attribute("data-theme", theme.as_str());
            }
            if let Ok(Some(storage)) = w.local_storage() {
                let _ = storage.set_item(STORAGE_KEY, theme.as_str());
            }
        }
    }
    #[cfg(not(feature = "hydrate"))]
    let _ = theme;
}

/// A theme signal initialised from the document once hydrated (the server
/// renders as dark; the bootstrap script has already set the real theme).
pub fn use_theme() -> RwSignal<Theme> {
    let t = RwSignal::new(Theme::Dark);
    Effect::new(move |_| t.set(current()));
    t
}
