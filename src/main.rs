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
        Some(Commands::Install {
            cmd,
            working_dir,
            name,
            disable_logs,
            log_rotation,
            max_log_files,
            account_name,
            account_password,
        }) => commands::install::handle(
            &cmd,
            working_dir,
            &name,
            disable_logs,
            log_rotation,
            max_log_files,
            account_name,
            account_password,
        ),

        Some(Commands::Uninstall { name }) => commands::uninstall::handle(&name),
        Some(Commands::Run {
            cmd,
            working_dir,
            name,
            disable_logs,
            log_rotation,
            max_log_files,
        }) => commands::run::handle(
            &cmd,
            working_dir,
            &name,
            disable_logs,
            log_rotation,
            max_log_files,
        ),
        None => {
            Cli::command().print_help().unwrap();
            std::process::exit(0);
        }
    }
}
