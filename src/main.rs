// Only the platform independent parts of wsw are compiled on other hosts,
// so that their unit tests can run anywhere.
#![cfg_attr(not(windows), allow(dead_code))]

#[cfg(windows)]
use clap::CommandFactory;
#[cfg(windows)]
use clap::Parser;

mod cli;
#[cfg(windows)]
mod commands;
mod pkg;

#[cfg(windows)]
use cli::*;

#[cfg(not(windows))]
fn main() {
    eprintln!("wsw only runs on Windows.");
    std::process::exit(1);
}

#[cfg(windows)]
fn main() {
    let cli = Cli::parse();
    // If parsing fails, clap will print the error and exit
    match cli.command {
        Some(Commands::Logs { name, follow, full }) => commands::logs::handle(&name, follow, full),
        Some(Commands::List) => commands::list::handle(),
        Some(Commands::Start { name }) => commands::start::handle(&name),
        Some(Commands::Stop { name }) => commands::stop::handle(&name),
        Some(Commands::Status { name }) => commands::status::handle(&name),
        Some(Commands::Restart { name }) => commands::restart::handle(&name),
        Some(Commands::Install { config }) => commands::install::handle(config),
        Some(Commands::Uninstall { name }) => commands::uninstall::handle(&name),
        Some(Commands::Run { config }) => commands::run::handle(config),
        None => {
            Cli::command().print_help().unwrap();
            std::process::exit(0);
        }
    }
}
