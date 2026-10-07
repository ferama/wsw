pub mod log_writer;
pub mod logs;
pub mod restart;
#[cfg(windows)]
pub mod runner;
#[cfg(windows)]
pub mod service;
pub mod stop_signal;

pub const SERVICE_DESCRIPTION_PREFIX: &str = "wsw";
