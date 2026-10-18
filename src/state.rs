//! Shared server state, provided to server functions via `provide_context`.
use std::sync::{Arc, Mutex};

use crate::incus::map::CpuTracker;
use crate::incus::IncusClient;

#[derive(Clone)]
pub struct AppState {
    pub incus: IncusClient,
    pub cpu: Arc<Mutex<CpuTracker>>,
}

impl AppState {
    pub fn new(incus: IncusClient, host_threads: u32) -> AppState {
        AppState { incus, cpu: Arc::new(Mutex::new(CpuTracker::new(host_threads))) }
    }
}
