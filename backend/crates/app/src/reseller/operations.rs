//! Admin operations overview and finance aggregates (`application/operations.go`).

use std::sync::Arc;

use zs_domain::Result;
use zs_domain::reseller::operations::{
    FinanceResponse, OverviewResponse, ReportQuery, finance_response, overview_response,
    resolve_window,
};
use zs_domain::reseller::ports::OperationsRepo;
use zs_shared::clock::Clock;

/// Operations dashboards.
#[derive(Clone)]
pub struct OperationsService {
    repo: Arc<dyn OperationsRepo>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for OperationsService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OperationsService")
    }
}

impl OperationsService {
    pub fn new(repo: Arc<dyn OperationsRepo>, clock: Arc<dyn Clock>) -> Self {
        Self { repo, clock }
    }

    /// Invalid ranges are `error.bad_request`.
    pub async fn overview(&self, query: &ReportQuery) -> Result<OverviewResponse> {
        let window = resolve_window(query, self.clock.now())?;
        let rows = self.repo.overview(window.start, window.end).await?;
        Ok(overview_response(&window, rows))
    }

    pub async fn finance(&self, query: &ReportQuery) -> Result<FinanceResponse> {
        let window = resolve_window(query, self.clock.now())?;
        let (period, current) = self.repo.finance(window.start, window.end).await?;
        Ok(finance_response(&window, period, current))
    }
}
