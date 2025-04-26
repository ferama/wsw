use prettytable::{Table, row};

use crate::pkg::service::list_services_with_status;

pub fn handle() {
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
