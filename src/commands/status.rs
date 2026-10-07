use windows_service::Error;
use windows_service::service::ServiceState;

use crate::pkg::logs::log_dir;
use crate::pkg::scm::{dependency_names, query_installed_service};
use crate::pkg::service::{get_service_command_line, get_service_status};
use prettytable::{Table, row};
use windows_sys::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_SERVICE_DOES_NOT_EXIST};

pub fn handle(name: &str, verbose: bool) {
    let result = get_service_status(name);
    let failed = result.is_err();
    match result {
        Ok(status) => {
            let mut table = Table::new();

            table.add_row(row!["Service Name", &name]);
            table.add_row(row!["Status", format!("{:?}", status.current_state)]);

            match status.process_id {
                Some(pid) => {
                    table.add_row(prettytable::Row::new(vec![
                        prettytable::Cell::new("PID"),
                        prettytable::Cell::new(&pid.to_string()),
                    ]));
                }
                None => {
                    table.add_row(prettytable::Row::new(vec![
                        prettytable::Cell::new("PID"),
                        prettytable::Cell::new("Not running"),
                    ]));
                }
            }

            if let Ok(commandline) = get_service_command_line(name) {
                table.add_row(row!["FullCmd", commandline.to_string()]);
            }

            if status.current_state == ServiceState::Stopped {
                table.add_row(row!["Exit Code", format!("{:?}", status.exit_code)]);
            } else {
                table.add_row(row!["Exit Code", "N/A"]);
            }

            if verbose {
                add_configuration(&mut table, name);
            }

            table.printstd();
        }
        Err(Error::Winapi(e)) => match e.raw_os_error() {
            Some(code) if code as u32 == ERROR_SERVICE_DOES_NOT_EXIST => {
                eprintln!("Service '{}' is not installed.", name);
            }
            Some(code) if code as u32 == ERROR_ACCESS_DENIED => {
                eprintln!("Access denied — run as Administrator or add the privilege.");
            }
            _ => {
                eprintln!("Failed to get service status '{}': {:?}", name, e);
            }
        },
        Err(e) => {
            eprintln!("Failed to get service status '{}': {:?}", name, e);
        }
    }
    if failed {
        std::process::exit(1);
    }
}

/// Adds the effective configuration of the service to the table.
fn add_configuration(table: &mut Table, name: &str) {
    let installed = match query_installed_service(name) {
        Ok(installed) => installed,
        Err(e) => {
            table.add_row(row!["Configuration", format!("unavailable: {}", e)]);
            return;
        }
    };
    let scm = &installed.scm;

    table.add_row(row!["Display Name", scm.display_name.to_string_lossy()]);
    if let Some(description) = &installed.description {
        table.add_row(row!["Description", description]);
    }
    table.add_row(row![
        "Start Type",
        installed
            .start_type
            .map_or("other".to_string(), |t| format!("{:?}", t))
    ]);
    table.add_row(row![
        "Account",
        installed
            .account()
            .unwrap_or_else(|| "LocalSystem".to_string())
    ]);
    let dependencies = dependency_names(&scm.dependencies);
    if !dependencies.is_empty() {
        table.add_row(row!["Depends On", dependencies.join("\n")]);
    }
    if let Some(failure_actions) = &installed.failure_actions
        && let Some(actions) = &failure_actions.actions
        && !actions.is_empty()
    {
        let actions: Vec<String> = actions
            .iter()
            .map(|action| format!("{:?} after {:?}", action.action_type, action.delay))
            .collect();
        table.add_row(row![
            "SCM Failure Actions",
            format!(
                "{}\nreset after {:?}",
                actions.join("\n"),
                failure_actions.reset_period
            )
        ]);
    }
    table.add_row(row![
        "Config Source",
        if installed.legacy {
            "ImagePath (installed by an older wsw, migrated by 'wsw update')"
        } else {
            "registry (Parameters\\Config)"
        }
    ]);

    let run = match installed.runtime.resolve() {
        Ok(run) => run,
        Err(e) => {
            table.add_row(row!["Configuration", format!("invalid: {}", e)]);
            return;
        }
    };
    table.add_row(row!["Command", run.command.to_string()]);
    table.add_row(row![
        "Working Dir",
        run.working_dir
            .clone()
            .unwrap_or_else(|| "(directory of the executable)".to_string())
    ]);
    if !run.env.is_empty() {
        table.add_row(row!["Environment", run.env.join("\n")]);
    }
    if let Some(env_file) = &run.env_file {
        table.add_row(row!["Env File", env_file]);
    }

    let restart = &run.restart;
    let mut restart_text = format!(
        "{:?}, delay {:?} up to {:?}, reset after {:?}",
        restart.policy, restart.delay, restart.max_delay, restart.reset_after
    );
    if restart.max_restarts > 0 {
        restart_text.push_str(&format!(
            ", give up after {} restarts",
            restart.max_restarts
        ));
    }
    table.add_row(row!["Restart", restart_text]);
    table.add_row(row![
        "Stop",
        match &run.stop_cmd {
            Some(stop_cmd) => format!("run '{}', timeout {:?}", stop_cmd, run.stop_timeout),
            None => format!("Ctrl+C, timeout {:?}", run.stop_timeout),
        }
    ]);

    if let Some(pre_start) = &run.pre_start {
        table.add_row(row!["Pre-start Hook", pre_start]);
    }
    if let Some(post_stop) = &run.post_stop {
        table.add_row(row!["Post-stop Hook", post_stop]);
    }
    if run.pre_start.is_some() || run.post_stop.is_some() {
        table.add_row(row!["Hook Timeout", format!("{:?}", run.hook_timeout)]);
    }

    let logs = &run.logs;
    let mut logs_text = if logs.disabled {
        "output not captured".to_string()
    } else {
        format!("rotation {}, keep {} files", logs.rotation, logs.max_files)
    };
    if let Some(max_size) = logs.max_size {
        logs_text.push_str(&format!(", max {} MB", max_size / 1024 / 1024));
    }
    if logs.split {
        logs_text.push_str(", stderr in a separate file");
    }
    table.add_row(row!["Logs", logs_text]);
    table.add_row(row![
        "Log Dir",
        log_dir(logs.dir.as_deref()).display().to_string()
    ]);
}
