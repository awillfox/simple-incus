//! Typed client for the Incus REST API (default project only).
mod client;
pub mod map;
pub mod raw;
mod transport;

pub use client::{IncusClient, IncusError, Response};
pub use hyper::Method;
pub use raw::Operation;
pub use transport::BoxedIo;
