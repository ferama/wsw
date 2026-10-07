pub mod account;
pub mod cmdline;
pub mod config;
#[cfg(windows)]
pub mod console;
pub mod env;
pub mod log_writer;
pub mod logs;
#[cfg(windows)]
pub mod registry;
pub mod restart;
pub mod rolling;
#[cfg(windows)]
pub mod runner;
#[cfg(windows)]
pub mod scm;
#[cfg(windows)]
pub mod security;
#[cfg(windows)]
pub mod service;
pub mod stop_signal;
#[cfg(windows)]
pub mod supervisor;

pub const SERVICE_DESCRIPTION_PREFIX: &str = "wsw";
