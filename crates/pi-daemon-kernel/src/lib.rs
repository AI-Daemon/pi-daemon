//! Agent kernel — registry, event bus, config, lifecycle management.

pub mod event_bus;
pub mod kernel;
pub mod registry;

pub use kernel::PiDaemonKernel;
