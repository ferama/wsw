pub mod log_writer;
pub mod logs;
#[cfg(windows)]
pub mod runner;
#[cfg(windows)]
pub mod service;
pub mod stop_signal;

pub const SERVICE_DESCRIPTION_PREFIX: &str = "wsw";
