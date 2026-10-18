//! Shared server state, provided to server functions via `provide_context`.
use std::sync::{Arc, Mutex};

use crate::incus::catalog::Catalog;
use crate::incus::map::CpuTracker;
use crate::incus::{EventHub, IncusClient};

#[derive(Clone)]
pub struct AppState {
    pub incus: IncusClient,
    pub cpu: Arc<Mutex<CpuTracker>>,
    pub hub: EventHub,
    pub catalog: Catalog,
}

impl AppState {
    /// Starts the event hub task, so it must run inside a tokio runtime.
    pub fn new(incus: IncusClient, host_threads: u32) -> AppState {
        let hub = EventHub::start(incus.clone());
        AppState::with_hub(incus, host_threads, hub)
    }

    /// Builds state around an existing hub (tests inject one with a fast backoff).
    pub fn with_hub(incus: IncusClient, host_threads: u32, hub: EventHub) -> AppState {
        AppState { incus, cpu: Arc::new(Mutex::new(CpuTracker::new(host_threads))), hub, catalog: Catalog::new() }
    }

    /// Replaces the image catalog (tests point it at a local index).
    pub fn with_catalog(mut self, catalog: Catalog) -> AppState {
        self.catalog = catalog;
        self
    }
}
