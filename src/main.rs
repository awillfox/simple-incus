#[cfg(feature = "ssr")]
fn bind(addr: std::net::SocketAddr) -> anyhow::Result<tokio::net::TcpListener> {
    use socket2::{Domain, Socket, Type};
    let socket = Socket::new(Domain::for_address(addr), Type::STREAM, None)?;
    if addr.is_ipv6() {
        socket.set_only_v6(true)?;
    }
    #[cfg(unix)]
    socket.set_reuse_address(true)?;
    socket.set_nonblocking(true)?;
    socket.bind(&addr.into())?;
    socket.listen(1024)?;
    Ok(tokio::net::TcpListener::from_std(socket.into())?)
}

#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use axum::{routing::post, Router};
    use leptos::logging::log;
    use leptos::prelude::*;
    use leptos_axum::{generate_route_list, LeptosRoutes};
    use simple_incus::app::*;
    use simple_incus::config::{Config, DEFAULT_LISTEN};
    use simple_incus::incus::IncusClient;
    use simple_incus::state::AppState;

    let cfg = Config::from_env()?;
    let conf = get_configuration(None)?;
    let leptos_options = conf.leptos_options;
    let routes = generate_route_list(App);

    let incus = IncusClient::new(cfg.incus.clone());
    let host_threads = match incus.host_threads().await {
        Ok(n) => n,
        Err(e) => {
            log!("could not read host CPU count from Incus ({e}); using local parallelism");
            std::thread::available_parallelism().map_or(1, |n| n.get() as u32)
        }
    };
    let state = AppState::new(incus, host_threads);

    let app = Router::new()
        .route(
            "/api/{*fn_name}",
            post({
                let state = state.clone();
                move |req: axum::extract::Request| {
                    let state = state.clone();
                    async move {
                        leptos_axum::handle_server_fns_with_context(
                            move || provide_context(state.clone()),
                            req,
                        )
                        .await
                    }
                }
            }),
        )
        .leptos_routes_with_context(
            &leptos_options,
            routes,
            {
                let state = state.clone();
                move || provide_context(state.clone())
            },
            {
                let leptos_options = leptos_options.clone();
                move || shell(leptos_options.clone())
            },
        )
        .fallback(leptos_axum::file_and_error_handler(shell))
        .with_state(leptos_options);

    let mut listeners = vec![bind(cfg.listen)?];
    if cfg.listen == DEFAULT_LISTEN.parse::<std::net::SocketAddr>()? {
        // Dual-stack: separate v6-only socket so both families are served.
        let v6 = std::net::SocketAddr::from(([0u16; 8], cfg.listen.port()));
        match bind(v6) {
            Ok(l) => listeners.push(l),
            Err(e) => log!("IPv6 listener on {v6} unavailable: {e}"),
        }
    }

    let mut tasks = tokio::task::JoinSet::new();
    for l in listeners {
        log!("listening on http://{}", l.local_addr()?);
        let app = app.clone();
        tasks.spawn(async move { axum::serve(l, app.into_make_service()).await });
    }
    while let Some(res) = tasks.join_next().await {
        res??;
    }
    Ok(())
}

#[cfg(not(feature = "ssr"))]
pub fn main() {
    // no client-side main function; see lib.rs for the hydration entry point
}
