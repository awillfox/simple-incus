use leptos::prelude::*;
use leptos_meta::{provide_meta_context, MetaTags, Stylesheet, Title};
use leptos_router::{
    components::{Route, Router, Routes},
    path,
};

use crate::ui::components::{
    provide_create_drawer, provide_nav_state, provide_toasts, IncusBanner, Sidebar, ToastHost, Topbar,
};
use crate::ui::pages::{ImagesPage, InstancePage, InstancesPage, NotFoundPage, OverviewPage};
use crate::ui::theme::BOOTSTRAP_SCRIPT;

pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en" data-theme="dark">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <meta name="color-scheme" content="dark light"/>
                // Apply the saved theme before first paint (no flash).
                <script inner_html=BOOTSTRAP_SCRIPT></script>
                <link rel="preload" href="/fonts/InterVariable.woff2" r#as="font" r#type="font/woff2" crossorigin=""/>
                <AutoReload options=options.clone() />
                <HydrationScripts options/>
                <MetaTags/>
            </head>
            <body>
                <App/>
            </body>
        </html>
    }
}

#[component]
pub fn App() -> impl IntoView {
    // Provides context that manages stylesheets, titles, meta tags, etc.
    provide_meta_context();
    // One shared /ws/events connection for the whole app (see ui::events).
    crate::ui::events::provide_event_stream();
    provide_toasts();
    provide_nav_state();
    // Opened by later pages (e.g. "Create from image"); content comes later.
    provide_create_drawer();

    view! {
        // id=leptos means cargo-leptos will hot-reload this stylesheet
        <Stylesheet id="leptos" href="/pkg/simple-incus.css"/>
        <Title text="simple-incus"/>

        <Router>
            <div class="flex h-dvh flex-col overflow-hidden">
                <IncusBanner/>
                <div class="flex min-h-0 flex-1">
                    <Sidebar/>
                    <div class="flex min-w-0 flex-1 flex-col">
                        <Topbar/>
                        <main class="min-h-0 flex-1 overflow-y-auto overflow-x-hidden">
                            <Routes fallback=|| view! { <NotFoundPage/> }>
                                <Route path=path!("/") view=OverviewPage/>
                                <Route path=path!("/instances") view=InstancesPage/>
                                <Route path=path!("/instances/:name/:tab?") view=InstancePage/>
                                <Route path=path!("/images/:tab?") view=ImagesPage/>
                            </Routes>
                        </main>
                    </div>
                </div>
            </div>
            <ToastHost/>
        </Router>
    }
}
