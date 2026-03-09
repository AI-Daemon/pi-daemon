//! Agent kernel — registry, event bus, config, lifecycle management.

pub mod config;
pub mod event_bus;
pub mod github;
pub mod kernel;
pub mod registry;

pub use kernel::PiDaemonKernel;
