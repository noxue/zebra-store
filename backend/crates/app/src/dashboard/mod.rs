//! `dashboard` use cases: operational reports, the admin user directory and
//! system information.

pub mod report;
pub mod system;
pub mod users;

/// Services of the `dashboard` group.
#[derive(Debug, Clone)]
pub struct DashboardServices {
    pub reports: report::ReportService,
    pub users: users::AdminUserService,
    pub system: system::SystemService,
}
