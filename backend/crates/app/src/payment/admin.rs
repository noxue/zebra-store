//! Admin payment records: list, detail and CSV export (`admin_handler.go`).

use std::collections::HashMap;
use std::sync::Arc;

use zs_domain::payment::channel::ChannelRepo;
use zs_domain::payment::errors::keys;
use zs_domain::payment::model::{
    AdminPayment, AdminPaymentFilter, EXPORT_HEADER, Payment, PaymentRepo, admin_payment,
    csv_record, export_row,
};
use zs_domain::{Error, Id, Result};
use zs_shared::page::{Page, PageRequest};

/// Export batch size (`adminPaymentExportBatchSize`).
pub const EXPORT_BATCH_SIZE: u64 = 500;

/// Admin payment queries.
#[derive(Clone)]
pub struct PaymentAdminService {
    payments: Arc<dyn PaymentRepo>,
    channels: Arc<dyn ChannelRepo>,
}

impl std::fmt::Debug for PaymentAdminService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PaymentAdminService")
    }
}

impl PaymentAdminService {
    pub fn new(payments: Arc<dyn PaymentRepo>, channels: Arc<dyn ChannelRepo>) -> Self {
        Self { payments, channels }
    }

    async fn channel_names(&self, payments: &[Payment]) -> Result<HashMap<Id, String>> {
        let mut ids: Vec<Id> = payments
            .iter()
            .map(|p| p.channel_id)
            .filter(|id| *id != 0)
            .collect();
        ids.sort_unstable();
        ids.dedup();
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        Ok(self
            .channels
            .list_by_ids(&ids)
            .await?
            .into_iter()
            .map(|c| (c.id, c.name))
            .collect())
    }

    async fn enrich(&self, payments: &[Payment]) -> Result<Vec<AdminPayment>> {
        let names = self.channel_names(payments).await?;
        let refs = self.payments.refs(payments).await?;
        Ok(payments
            .iter()
            .map(|p| {
                admin_payment(
                    p,
                    names.get(&p.channel_id).map_or("", String::as_str),
                    &refs,
                )
            })
            .collect())
    }

    /// `GET /admin/payments`.
    pub async fn list(&self, filter: &AdminPaymentFilter) -> Result<Page<AdminPayment>> {
        let run = async {
            let (payments, total) = self.payments.list_admin(filter).await?;
            Ok::<_, Error>(Page {
                items: self.enrich(&payments).await?,
                total,
            })
        };
        run.await
            .map_err(|e| e.or_internal(keys::PAYMENT_FETCH_FAILED))
    }

    /// `GET /admin/payments/:id`.
    pub async fn get(&self, id: Id) -> Result<AdminPayment> {
        let run = async {
            let payment = self
                .payments
                .get(id)
                .await?
                .ok_or_else(|| Error::not_found(keys::PAYMENT_NOT_FOUND))?;
            let mut items = self.enrich(std::slice::from_ref(&payment)).await?;
            items
                .pop()
                .ok_or_else(|| Error::not_found(keys::PAYMENT_NOT_FOUND))
        };
        run.await
            .map_err(|e| e.or_internal(keys::PAYMENT_FETCH_FAILED))
    }

    /// `GET /admin/payments/export`: the full CSV (fetched in batches of 500, no count query).
    pub async fn export_csv(&self, filter: &AdminPaymentFilter) -> Result<String> {
        let header: Vec<String> = EXPORT_HEADER.iter().map(|s| (*s).to_owned()).collect();
        let mut out = csv_record(&header);
        let mut filter = AdminPaymentFilter {
            skip_count: true,
            page: PageRequest {
                page: 1,
                page_size: EXPORT_BATCH_SIZE,
            },
            ..filter.clone()
        };
        loop {
            let (batch, _) = self
                .payments
                .list_admin(&filter)
                .await
                .map_err(|e| e.or_internal(keys::PAYMENT_FETCH_FAILED))?;
            if !batch.is_empty() {
                let refs = self
                    .payments
                    .refs(&batch)
                    .await
                    .map_err(|e| e.or_internal(keys::PAYMENT_FETCH_FAILED))?;
                for p in &batch {
                    out.push_str(&csv_record(&export_row(p, &refs)));
                }
            }
            if (batch.len() as u64) < EXPORT_BATCH_SIZE {
                break;
            }
            filter.page.page += 1;
        }
        Ok(out)
    }
}
