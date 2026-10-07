pub mod log_writer;
pub mod logs;
#[cfg(windows)]
pub mod runner;
#[cfg(windows)]
pub mod service;

pub const SERVICE_DESCRIPTION_PREFIX: &str = "wsw";
