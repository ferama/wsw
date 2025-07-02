use clap::CommandFactory;
use clap::Parser;

mod cli;
mod commands;
mod pkg;

use cli::*;

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
            account_password
        }) => commands::install::handle(
            &cmd,
            working_dir,
            &name,
            disable_logs,
            log_rotation,
            max_log_files,
            account_name,
            account_password
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
