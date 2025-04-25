use clap::CommandFactory;
use clap::Parser;
use runner::run_command;
use tracing::{error, info};

use prettytable::{Table, row};
use windows_service::define_windows_service;
use windows_service::service_dispatcher;

mod cli;
mod logs;
mod runner;
mod service;

use cli::*;
use logs::*;
use service::*;

fn main() {
    let cli = Cli::parse();
    // If parsing fails, clap will print the error and exit
    match cli.command {
        Some(Commands::List) => {
            let services = list_services_with_status();
            if services.is_empty() {
                println!("No services found.");
            } else {
                let mut table = Table::new();
                table.add_row(row!["Service Name", "Status"]);

                for service in services {
                    table.add_row(row![service.0, service.1]);
                }

                table.printstd();
            }
        }
        Some(Commands::Install {
            cmd,
            working_dir,
            name,
        }) => {
            let svc_name = get_service_name(&name);
            let _guard = setup_logging(&svc_name);

            install_service(&svc_name, working_dir, &cmd);
            info!("Service '{}' installed successfully.", svc_name);
        }
        Some(Commands::Uninstall { name }) => {
            let svc_name = get_service_name(&name);
            let _guard = setup_logging(&svc_name);

            let res = uninstall_service(&svc_name);
            if res.is_ok() {
                info!("Service '{}' uninstalled successfully.", svc_name);
            } else {
                error!(
                    "Failed to uninstall service '{}': {}",
                    name,
                    res.unwrap_err()
                );
            }
        }
        Some(Commands::Run {
            cmd,
            working_dir,
            name,
        }) => {
            define_windows_service!(ffi_service_main, service_main);
            let _guard = setup_logging(&name);
            if let Err(_e) = service_dispatcher::start(name, ffi_service_main) {
                if let Ok(mut child) = run_command(&cmd, working_dir) {
                    if let Err(e) = child.wait() {
                        error!("Failed to wait for child process: {}", e);
                    }
                }
            }
        }
        None => {
            let help = Cli::command().render_help();
            println!("{}", help.ansi());
            std::process::exit(1);
        }
    }
}
