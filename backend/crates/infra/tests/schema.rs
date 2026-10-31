//! Schema synchronisation creates every table and is idempotent.

use sea_orm::{ConnectionTrait, Statement};
use zs_app::config::DatabaseConfig;

/// Old profiles must survive adding nullable administrator metadata columns.
#[tokio::test]
async fn sync_upgrades_existing_reseller_profiles() {
    use zs_domain::reseller::ports::ProfileRepo;
    use zs_infra::db::repo::reseller::SeaResellerStore;

    let cfg = DatabaseConfig {
        url: "sqlite::memory:".into(),
        max_connections: 1,
        ..Default::default()
    };
    let db = zs_infra::db::connect(&cfg).await.unwrap();
    zs_infra::db::sync_schema(&db).await.unwrap();
    let repo = SeaResellerStore::new(db.clone());
    let profile = repo
        .create_profile(123, "existing application", chrono::Utc::now())
        .await
        .unwrap();
    // Simulate the previous schema, then use the real startup upgrade path.
    for column in ["tags", "admin_note"] {
        db.execute_raw(Statement::from_string(
            db.get_database_backend(),
            format!("ALTER TABLE reseller_profiles DROP COLUMN {column}"),
        ))
        .await
        .unwrap();
    }
    zs_infra::db::sync_schema(&db).await.unwrap();
    zs_infra::db::sync_schema(&db).await.unwrap();
    let upgraded = repo.profile_by_id(profile.id).await.unwrap().unwrap();
    assert_eq!(upgraded.apply_reason, "existing application");
    assert!(upgraded.tags.is_empty());
    assert_eq!(upgraded.admin_note, "");
}

#[tokio::test]
async fn sync_creates_all_tables_idempotently() {
    let cfg = DatabaseConfig {
        url: "sqlite::memory:".into(),
        max_connections: 1,
        ..Default::default()
    };
    let db = zs_infra::db::connect(&cfg).await.unwrap();
    zs_infra::db::sync_schema(&db).await.unwrap();
    zs_infra::db::sync_schema(&db).await.unwrap();
    let rows = db
        .query_all_raw(Statement::from_string(
            db.get_database_backend(),
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
        ))
        .await
        .unwrap();
    // 60 original tables + casbin_rule + jobs + integration / zebra-store extras,
    // including converter profiles, bindings, event history, and gateway refund attempts.
    let names: Vec<String> = rows
        .iter()
        .map(|row| row.try_get("", "name").unwrap())
        .collect();
    assert_eq!(rows.len(), 80, "tables: {names:?}");
    assert!(names.iter().any(|name| name == "gateway_refund_attempts"));
    assert!(names.iter().any(|name| name == "personal_api_keys"));
    assert!(
        names
            .iter()
            .any(|name| name == "procurement_stock_recoveries")
    );
}

// RSL-13: historical paid withdrawals get one debit at their original payout
// time; pending/rejected/deleted requests and balance caches remain untouched.
#[tokio::test]
async fn backfills_paid_withdrawal_debits_without_moving_balances() {
    use sea_orm::{ActiveModelTrait, EntityTrait, Set};
    use zs_infra::db::entity::{
        reseller_ledger_entries as ledger, reseller_withdraw_requests as withdraws,
    };
    use zs_shared::money::Amount;

    let db = zs_infra::db::connect(&DatabaseConfig {
        url: "sqlite::memory:".into(),
        max_connections: 1,
        ..Default::default()
    })
    .await
    .unwrap();
    zs_infra::db::sync_schema(&db).await.unwrap();
    let paid_at = chrono::DateTime::parse_from_rfc3339("2026-10-27T12:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    for (status, deleted) in [
        ("paid", false),
        ("pending", false),
        ("rejected", false),
        ("paid", true),
    ] {
        withdraws::ActiveModel {
            reseller_id: Set(7),
            amount: Set("25.00".parse().unwrap()),
            currency: Set("CNY".into()),
            channel: Set("alipay".into()),
            account: Set("test".into()),
            payee_name: Set(String::new()),
            qr_code_url: Set(String::new()),
            status: Set(status.into()),
            reject_reason: Set(String::new()),
            processed_by: Set(None),
            processed_at: Set(Some(paid_at)),
            created_at: Set(paid_at - chrono::Duration::days(1)),
            updated_at: Set(paid_at + chrono::Duration::days(1)),
            deleted_at: Set(deleted.then_some(paid_at)),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
    }
    zs_infra::db::sync_schema(&db).await.unwrap();
    zs_infra::db::sync_schema(&db).await.unwrap();
    let rows = ledger::Entity::find().all(&db).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].type_, "withdraw_paid");
    assert_eq!(Amount::new(rows[0].amount).to_string(), "-25.00");
    assert_eq!(rows[0].status, "withdrawn");
    assert_eq!(rows[0].withdraw_request_id, Some(1));
    assert_eq!(rows[0].created_at, paid_at);
    assert_eq!(rows[0].idempotency_key, "withdraw_paid:1");
}

/// RFD-ADMIN-01: existing refund attempts gain conservative defaults when upgrading.
#[tokio::test]
async fn sync_upgrades_refund_approval_without_changing_existing_attempts() {
    use sea_orm::{ActiveModelTrait, EntityTrait, Set};
    use zs_infra::db::entity::gateway_refund_attempts as attempts;
    let db = zs_infra::db::connect(&DatabaseConfig {
        url: "sqlite::memory:".into(),
        max_connections: 1,
        ..Default::default()
    })
    .await
    .unwrap();
    zs_infra::db::sync_schema(&db).await.unwrap();
    let now = chrono::Utc::now();
    let attempt = attempts::ActiveModel {
        order_id: Set(1),
        payment_id: Set(1),
        channel_id: Set(1),
        request_date: Set("20261030".into()),
        refund_no: Set("EXISTING-REFUND".into()),
        provider_ref: Set("HF-EXISTING".into()),
        amount: Set("125.00".parse().unwrap()),
        status: Set("pending".into()),
        remark: Set(String::new()),
        payment_fee_refunded: Set(false),
        card_confirmed: Set(false),
        payload: Set(None),
        error: Set(String::new()),
        refund_record_id: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();
    for column in ["force_refund", "restock_cards", "admin_refund_by"] {
        db.execute_raw(Statement::from_string(
            db.get_database_backend(),
            format!("ALTER TABLE gateway_refund_attempts DROP COLUMN {column}"),
        ))
        .await
        .unwrap();
    }
    zs_infra::db::sync_schema(&db).await.unwrap();
    zs_infra::db::sync_schema(&db).await.unwrap();
    let upgraded = attempts::Entity::find_by_id(attempt.id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(upgraded.status, "pending");
    assert_eq!(upgraded.amount, "125".parse().unwrap());
    assert!(!upgraded.force_refund);
    assert!(!upgraded.restock_cards);
    assert_eq!(upgraded.admin_refund_by, None);
}
