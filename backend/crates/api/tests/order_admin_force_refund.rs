//! RFD-ADMIN-01: an explicit administrator override for delivered cards uses the real gateway path.
#![expect(
    clippy::unwrap_used,
    reason = "Integration fixtures assert each setup operation"
)]
mod order_common;
mod payment_common;
use order_common::{Auth, OrderApp};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, IntoActiveModel, PaginatorTrait, QueryFilter, Set,
};
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use zs_infra::db::entity::{
    card_secrets, fulfillments, gateway_refund_attempts, order_refund_records,
};
use zs_infra::payment::http::{HttpResponse, MockTransport};

struct Fixture {
    app: OrderApp,
    order_id: i64,
    product_id: i64,
    sku_id: i64,
    child_id: i64,
    calls: Arc<AtomicUsize>,
    pending: Arc<AtomicBool>,
    failed: Arc<AtomicBool>,
}
async fn fixture() -> Fixture {
    fixture_with_policy(false).await
}

async fn fixture_with_policy(allow_private: bool) -> Fixture {
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let pending = Arc::new(AtomicBool::new(false));
    let pending_ref = pending.clone();
    let failed = Arc::new(AtomicBool::new(false));
    let failed_ref = failed.clone();
    let transport = MockTransport::new(move |request| {
        let path = request.url.as_str();
        let data = if path.ends_with(huifu_pay::PREORDER_PATH) {
            json!({"resp_code":"00000000","jump_url":"https://pay.example.test/fixture"})
        } else {
            if path.ends_with(huifu_pay::REFUND_PATH) {
                counter.fetch_add(1, Ordering::SeqCst);
            } else {
                assert!(path.ends_with(huifu_pay::REFUND_QUERY_PATH));
            }
            json!({"resp_code":"00000000","trans_stat":if failed_ref.load(Ordering::SeqCst) {"F"} else if pending_ref.load(Ordering::SeqCst) {"P"} else {"S"}})
        };
        let sign = huifu_pay::sign_value(payment_common::PRIV_PEM, &data).unwrap();
        Ok(HttpResponse::new(
            200,
            json!({"data":data,"sign":sign}).to_string(),
        ))
    });
    let app = OrderApp::with_converter_policy(Arc::new(transport), allow_private).await;
    let (product_id, sku_id) = app
        .product("force-refund-card", json!({"price_amount":"125.00"}))
        .await;
    app.secrets(product_id, sku_id, 1).await;
    let channel=app.admin_call("POST","/api/v1/admin/payment-channels",Some(json!({
        "name":"Huifu fixture","provider_type":"huifu","channel_type":"alipay","interaction_mode":"redirect","payment_roles":["guest"],"payment_types":["order"],
        "config_json":{"sys_id":"SYS-1","product_id":"PROD-1","huifu_id":"HU-1","merchant_private_key":payment_common::PRIV_PEM,"huifu_public_key":payment_common::PUB_PEM,"project_id":"PROJECT-1","notify_url":"https://shop.example.test/api/v1/payments/callback","return_url":"https://shop.example.test/pay","supported_channel_types":["alipay"]}
    }))).await["data"]["id"].as_i64().unwrap();
    let created=app.call("POST","/api/v1/guest/orders/create-and-pay",Some(json!({"email":"buyer@example.test","order_password":"secret-pass","items":[{"product_id":product_id,"sku_id":sku_id,"quantity":1}],"channel_id":channel,"channel_type":"alipay"})),&Auth::None).await;
    assert_eq!(created["status_code"], 0, "{created}");
    let payment = app
        .payment(created["data"]["payment_id"].as_i64().unwrap())
        .await;
    let order_id = payment.order_id;
    let now = chrono::Utc::now();
    let mut payment = payment.into_active_model();
    payment.status = Set("success".into());
    payment.paid_at = Set(Some(now));
    payment.update(&app.db).await.unwrap();
    let child = app.children(order_id).await[0].id;
    for id in [order_id, child] {
        let mut order = app.order(id).await.into_active_model();
        order.status = Set("paid".into());
        order.paid_at = Set(Some(now));
        if id == order_id {
            order.online_paid_amount = Set(order_common::dec("125.00"));
        }
        order.update(&app.db).await.unwrap();
    }
    app.services
        .order
        .service
        .auto_fulfill(child)
        .await
        .unwrap();
    Fixture {
        app,
        order_id,
        product_id,
        sku_id,
        child_id: child,
        calls,
        pending,
        failed,
    }
}

#[tokio::test]
async fn rfd_admin_01_failed_provider_refund_keeps_order_and_cards_unchanged() {
    let f = fixture().await;
    f.failed.store(true, Ordering::SeqCst);
    let result = f.refund(true).await;
    assert_eq!(result["status_code"], 0, "{result}");
    assert_eq!(result["data"]["attempt"]["status"], "failed");
    assert_eq!(
        f.app.order(f.order_id).await.refunded_amount,
        order_common::dec("0")
    );
    assert_eq!(f.app.secrets_with(f.product_id, "used").await, 1);
    assert_eq!(f.app.secrets_with(f.product_id, "available").await, 0);
    assert_eq!(
        order_refund_records::Entity::find()
            .count(&f.app.db)
            .await
            .unwrap(),
        0
    );
    assert_eq!(f.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn rfd_admin_01_concurrent_force_requests_submit_and_restock_once() {
    let f = fixture().await;
    let (first, second) = tokio::join!(f.refund(true), f.refund(true));
    assert_eq!(first["status_code"], 0, "{first}");
    assert_eq!(second["status_code"], 0, "{second}");
    assert_eq!(f.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        f.app.order(f.order_id).await.refunded_amount,
        order_common::dec("125")
    );
    assert_eq!(f.app.secrets_with(f.product_id, "available").await, 1);
    assert_eq!(
        order_refund_records::Entity::find()
            .count(&f.app.db)
            .await
            .unwrap(),
        1
    );
}
impl Fixture {
    fn path(&self) -> String {
        format!("/api/v1/admin/orders/{}/original-refund", self.order_id)
    }
    async fn refund(&self, restock: bool) -> serde_json::Value {
        self.app
            .admin_call(
                "POST",
                &self.path(),
                Some(json!({"amount":"125.00","force_refund":true,"restock_cards":restock})),
            )
            .await
    }
}

#[tokio::test]
async fn rfd_admin_01_direct_cards_offer_original_refund_and_default_to_no_restock() {
    let f = fixture().await;
    let detail = f
        .app
        .admin_call("GET", &format!("/api/v1/admin/orders/{}", f.order_id), None)
        .await;
    assert_eq!(detail["data"]["refund_mode"], "contact_support");
    assert_eq!(detail["data"]["original_refund_supported"], true);
    let guarded = f
        .app
        .admin_call("POST", &f.path(), Some(json!({"amount":"125.00"})))
        .await;
    assert_ne!(guarded["status_code"], 0);
    assert_eq!(f.calls.load(Ordering::SeqCst), 0);
    let done = f.refund(false).await;
    assert_eq!(done["status_code"], 0, "{done}");
    assert_eq!(done["data"]["attempt"]["status"], "succeeded");
    assert_eq!(f.app.secrets_with(f.product_id, "used").await, 1);
    assert_eq!(f.app.secrets_with(f.product_id, "available").await, 0);
    let again = f.refund(false).await;
    assert_eq!(again["status_code"], 0, "{again}");
    assert_eq!(f.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        order_refund_records::Entity::find()
            .count(&f.app.db)
            .await
            .unwrap(),
        1
    );
    let attempt = gateway_refund_attempts::Entity::find()
        .one(&f.app.db)
        .await
        .unwrap()
        .unwrap();
    assert!(attempt.force_refund);
    assert!(!attempt.restock_cards);
    assert!(attempt.admin_refund_by.is_some());
    assert!(attempt.card_confirmed);
}

#[tokio::test]
async fn rfd_admin_01_local_cards_return_once_only_after_gateway_success_and_restart_reconciliation()
 {
    let f = fixture().await;
    f.pending.store(true, Ordering::SeqCst);
    let first = f.refund(true).await;
    assert_eq!(first["status_code"], 0, "{first}");
    assert_eq!(first["data"]["attempt"]["status"], "pending");
    assert_eq!(f.app.secrets_with(f.product_id, "available").await, 0);
    let change = f.refund(false).await;
    assert_ne!(change["status_code"], 0);
    assert_eq!(f.calls.load(Ordering::SeqCst), 1);
    gateway_refund_attempts::Entity::update_many()
        .col_expr(
            gateway_refund_attempts::Column::UpdatedAt,
            sea_orm::sea_query::Expr::value(chrono::Utc::now() - chrono::Duration::seconds(31)),
        )
        .exec(&f.app.db)
        .await
        .unwrap();
    f.pending.store(false, Ordering::SeqCst);
    f.app
        .services
        .order
        .service
        .reconcile_gateway_refunds()
        .await
        .unwrap();
    assert_eq!(
        f.app.order(f.order_id).await.refunded_amount,
        order_common::dec("125.00")
    );
    assert_eq!(f.app.secrets_with(f.product_id, "available").await, 1);
    let card = card_secrets::Entity::find()
        .one(&f.app.db)
        .await
        .unwrap()
        .unwrap();
    assert!(card.order_id.is_none());
    assert!(card.used_at.is_none());
    f.app
        .services
        .order
        .service
        .reconcile_gateway_refunds()
        .await
        .unwrap();
    assert_eq!(f.calls.load(Ordering::SeqCst), 1);
    assert_eq!(f.app.secrets_with(f.product_id, "available").await, 1);
}

#[tokio::test]
async fn rfd_admin_01_upstream_cards_return_to_matching_local_sku_without_duplicates() {
    let f = fixture().await;
    // A direct supplier delivery has no pre-existing local card row.
    card_secrets::Entity::delete_many()
        .exec(&f.app.db)
        .await
        .unwrap();
    fulfillments::Entity::update_many()
        .col_expr(
            fulfillments::Column::Type,
            sea_orm::sea_query::Expr::value("upstream"),
        )
        .filter(fulfillments::Column::OrderId.eq(f.child_id))
        .exec(&f.app.db)
        .await
        .unwrap();
    let result = f.refund(true).await;
    assert_eq!(result["status_code"], 0, "{result}");
    let card = card_secrets::Entity::find()
        .one(&f.app.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(card.product_id, f.product_id);
    assert_eq!(card.sku_id, f.sku_id);
    assert_eq!(card.status, "available");
    assert!(card.batch_id.is_some());
    let result = f.refund(true).await;
    assert_eq!(result["status_code"], 0, "{result}");
    assert_eq!(
        card_secrets::Entity::find().count(&f.app.db).await.unwrap(),
        1
    );
    assert_eq!(f.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn rfd_admin_01_rejects_invalid_amount_restock_and_unauthorized_requests_before_gateway() {
    let f = fixture().await;
    for body in [
        json!({"amount":"126","force_refund":true}),
        json!({"amount":"10","force_refund":true,"restock_cards":true}),
        json!({"amount":"125","restock_cards":true}),
    ] {
        let result = f.app.admin_call("POST", &f.path(), Some(body)).await;
        assert_ne!(result["status_code"], 0, "{result}");
    }
    let result = f
        .app
        .call(
            "POST",
            &f.path(),
            Some(json!({"amount":"125","force_refund":true})),
            &Auth::None,
        )
        .await;
    assert_eq!(result["status_code"], 401);
    assert_eq!(f.calls.load(Ordering::SeqCst), 0);
}

/// RFD-05: convert a completed local fixture into an undelivered upstream order.
async fn upstream_waiting(f: &Fixture) {
    use zs_infra::db::entity::{order_items, orders};
    fulfillments::Entity::delete_many()
        .exec(&f.app.db)
        .await
        .unwrap();
    for id in [f.order_id, f.child_id] {
        let mut row = f.app.order(id).await.into_active_model();
        row.status = Set("fulfilling".into());
        row.update(&f.app.db).await.unwrap();
    }
    order_items::Entity::update_many()
        .col_expr(
            order_items::Column::FulfillmentType,
            sea_orm::sea_query::Expr::value("upstream"),
        )
        .filter(order_items::Column::OrderId.eq(f.child_id))
        .exec(&f.app.db)
        .await
        .unwrap();
    assert_eq!(
        orders::Entity::find_by_id(f.child_id)
            .one(&f.app.db)
            .await
            .unwrap()
            .unwrap()
            .status,
        "fulfilling"
    );
}

async fn procurement(f: &Fixture, status: &str) -> i64 {
    use zs_domain::integration::procurement::{NewProcurement, ProcurementRepo};
    use zs_infra::db::entity::procurement_orders;
    let repo =
        zs_infra::db::repo::integration::procurement::SeaProcurementRepo::new(f.app.db.clone());
    let row = repo
        .create_once(
            &NewProcurement {
                connection_id: 1,
                local_order_id: f.child_id,
                local_order_no: f.app.order(f.child_id).await.order_no,
                local_sell_amount: zs_shared::money::Amount::new(order_common::dec("125")),
                currency: "CNY".into(),
                trace_id: "rfd05-fixture".into(),
            },
            chrono::Utc::now(),
        )
        .await
        .unwrap()
        .unwrap();
    let mut stored = procurement_orders::Entity::find_by_id(row.id)
        .one(&f.app.db)
        .await
        .unwrap()
        .unwrap()
        .into_active_model();
    stored.status = Set(status.into());
    stored.update(&f.app.db).await.unwrap();
    row.id
}

#[tokio::test]
async fn rfd05_unresolved_procurement_blocks_force_and_buyer_refunds_before_gateway() {
    for status in [
        "submitted",
        "accepted",
        "manual_review",
        "partially_refunded",
        "fulfilled",
        "completed",
        "future_status",
    ] {
        let f = fixture().await;
        upstream_waiting(&f).await;
        procurement(&f, status).await;
        let detail = f
            .app
            .admin_call("GET", &format!("/api/v1/admin/orders/{}", f.order_id), None)
            .await;
        assert_eq!(
            detail["data"]["refund_mode"], "processing",
            "{status}: {detail}"
        );
        assert_eq!(detail["data"]["original_refund_supported"], false);
        let result = f.refund(false).await;
        assert_ne!(result["status_code"], 0, "{status}: {result}");
        assert_eq!(f.calls.load(Ordering::SeqCst), 0, "{status}");
        assert_eq!(
            gateway_refund_attempts::Entity::find()
                .count(&f.app.db)
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            f.app.order(f.order_id).await.refunded_amount,
            order_common::dec("0")
        );
    }
}

#[tokio::test]
async fn rfd05_missing_procurement_and_unconfirmed_cancellation_stay_processing() {
    let f = fixture().await;
    upstream_waiting(&f).await;
    assert_ne!(f.refund(false).await["status_code"], 0);
    procurement(&f, "canceled").await;
    assert_ne!(f.refund(false).await["status_code"], 0);
    assert_eq!(f.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn rfd05_refunded_order_rejects_late_delivery_and_new_purchase() {
    use zs_domain::integration::{
        hooks::UpstreamDelivery,
        procurement::{ProcurementChange, ProcurementRepo, ProcurementStatus},
    };
    use zs_domain::order::ports::OrderStore;
    let f = fixture().await;
    upstream_waiting(&f).await;
    let id = procurement(&f, "rejected").await;
    assert_eq!(f.refund(false).await["status_code"], 0);
    let store = zs_infra::db::repo::order::SeaOrderStore::new(f.app.db.clone(), "fixture-secret");
    let result = store
        .upstream_deliver(
            f.child_id,
            &UpstreamDelivery {
                payload: "SYNTHETIC-NONREDEEMABLE-TEST-CARD".into(),
                delivery_data: Default::default(),
                delivered_at: None,
                ..Default::default()
            },
            chrono::Utc::now(),
        )
        .await;
    assert!(result.is_err());
    assert_eq!(
        fulfillments::Entity::find().count(&f.app.db).await.unwrap(),
        0
    );
    let repo =
        zs_infra::db::repo::integration::procurement::SeaProcurementRepo::new(f.app.db.clone());
    assert!(
        repo.update(
            id,
            &[ProcurementStatus::Rejected],
            &ProcurementChange {
                status: Some(ProcurementStatus::Pending),
                ..Default::default()
            },
            chrono::Utc::now()
        )
        .await
        .is_err()
    );
    assert_eq!(f.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn rfd05_refund_and_procurement_retry_share_one_transaction_barrier() {
    use zs_domain::integration::procurement::{
        ProcurementChange, ProcurementRepo, ProcurementStatus,
    };
    let f = fixture().await;
    upstream_waiting(&f).await;
    let id = procurement(&f, "rejected").await;
    let repo =
        zs_infra::db::repo::integration::procurement::SeaProcurementRepo::new(f.app.db.clone());
    let change = ProcurementChange {
        status: Some(ProcurementStatus::Submitted),
        ..Default::default()
    };
    let (refund, retry) = tokio::join!(
        f.refund(false),
        repo.update(
            id,
            &[ProcurementStatus::Rejected],
            &change,
            chrono::Utc::now()
        )
    );
    if retry.is_ok_and(|changed| changed) {
        assert_ne!(refund["status_code"], 0, "{refund}");
        assert_eq!(f.calls.load(Ordering::SeqCst), 0);
        assert_eq!(
            f.app.order(f.order_id).await.refunded_amount,
            order_common::dec("0")
        );
    } else {
        assert_eq!(refund["status_code"], 0, "{refund}");
        assert_eq!(f.app.order(f.order_id).await.status, "refunded");
    }
}

async fn refunded_purchase(f: &Fixture, quantity: i32, status: &str) -> i64 {
    use zs_infra::db::entity::{order_items, procurement_orders};
    upstream_waiting(f).await;
    let id = procurement(f, "rejected").await;
    assert_eq!(f.refund(false).await["status_code"], 0);
    order_items::Entity::update_many()
        .col_expr(
            order_items::Column::Quantity,
            sea_orm::sea_query::Expr::value(quantity),
        )
        .filter(order_items::Column::OrderId.eq(f.child_id))
        .exec(&f.app.db)
        .await
        .unwrap();
    let mut purchase = procurement_orders::Entity::find_by_id(id)
        .one(&f.app.db)
        .await
        .unwrap()
        .unwrap()
        .into_active_model();
    purchase.status = Set(status.into());
    purchase.upstream_order_no = Set("SYNTHETIC-SUPPLIER-RECEIPT".into());
    purchase.update(&f.app.db).await.unwrap();
    id
}

async fn late_receipt(f: &Fixture, id: i64, payload: &str) {
    use zs_domain::integration::{procurement::UpstreamEvent, protocol::RemoteFulfillment};
    f.app
        .services
        .integration
        .procurement
        .handle_event(
            id,
            &UpstreamEvent::Delivered,
            Some(&RemoteFulfillment {
                kind: "auto".into(),
                status: "delivered".into(),
                payload: payload.into(),
                delivery_data: None,
                delivered_at: None,
            }),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn rfd06_late_receipts_for_canceled_and_refunded_tasks_enter_stock_without_buyer_delivery() {
    use zs_infra::db::entity::{
        card_secret_batches, procurement_orders, procurement_stock_recoveries,
    };
    for status in ["accepted", "canceled", "refunded"] {
        let f = fixture().await;
        let id = refunded_purchase(&f, 2, status).await;
        late_receipt(&f, id, "SYNTHETIC-RECOVERY-A\nSYNTHETIC-RECOVERY-B").await;
        let purchase = procurement_orders::Entity::find_by_id(id)
            .one(&f.app.db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(purchase.status, "stock_recovered", "{status}");
        use zs_domain::integration::procurement::ProcurementRepo;
        let repo =
            zs_infra::db::repo::integration::procurement::SeaProcurementRepo::new(f.app.db.clone());
        assert!(repo.get(id).await.unwrap().unwrap().held_delivery.is_some());
        assert_eq!(f.app.order(f.child_id).await.status, "refunded");
        assert_eq!(f.app.order(f.order_id).await.status, "refunded");
        assert_eq!(
            fulfillments::Entity::find().count(&f.app.db).await.unwrap(),
            0
        );
        assert_eq!(f.app.secrets_with(f.product_id, "available").await, 2);
        assert_eq!(
            procurement_stock_recoveries::Entity::find()
                .count(&f.app.db)
                .await
                .unwrap(),
            2
        );
        let batch = card_secret_batches::Entity::find()
            .filter(card_secret_batches::Column::BatchNo.eq(format!("UPSTREAM-RECOVERY-{id}")))
            .one(&f.app.db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(batch.source, "manual");
        assert_eq!(batch.sku_id, f.sku_id);
        // Duplicate delivery events never create another batch or inventory entry.
        late_receipt(&f, id, "SYNTHETIC-RECOVERY-A\nSYNTHETIC-RECOVERY-B").await;
        assert_eq!(f.app.secrets_with(f.product_id, "available").await, 2);
        assert_eq!(
            procurement_stock_recoveries::Entity::find()
                .count(&f.app.db)
                .await
                .unwrap(),
            2
        );
    }
}

#[tokio::test]
async fn rfd06_recovered_card_is_sold_locally_and_restart_replay_never_reactivates_it() {
    use zs_infra::db::entity::procurement_orders;
    let f = fixture().await;
    let id = refunded_purchase(&f, 1, "accepted").await;
    late_receipt(&f, id, "SYNTHETIC-RECOVERY-RESALE").await;
    // A second buyer's real checkout reserves the recovered SKU; no second purchase is needed.
    let created = f.app.call("POST", "/api/v1/guest/orders", Some(json!({
        "email":"next-buyer@example.test", "order_password":"next-secret", "items":[{"product_id":f.product_id,"sku_id":f.sku_id,"quantity":1}]
    })), &Auth::None).await;
    assert_eq!(created["status_code"], 0, "{created}");
    let root = f
        .app
        .order_by_no(created["data"]["order_no"].as_str().unwrap())
        .await
        .id;
    let child = f.app.children(root).await[0].id;
    for target in [root, child] {
        let mut row = f.app.order(target).await.into_active_model();
        row.status = Set("paid".into());
        row.paid_at = Set(Some(chrono::Utc::now()));
        row.update(&f.app.db).await.unwrap();
    }
    f.app
        .services
        .order
        .service
        .auto_fulfill(child)
        .await
        .unwrap();
    let delivery = fulfillments::Entity::find()
        .filter(fulfillments::Column::OrderId.eq(child))
        .one(&f.app.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(delivery.payload, "SYNTHETIC-RECOVERY-RESALE");
    assert_eq!(f.app.secrets_with(f.product_id, "available").await, 0);
    // Simulate a crash after committing stock but before committing procurement status.
    let mut purchase = procurement_orders::Entity::find_by_id(id)
        .one(&f.app.db)
        .await
        .unwrap()
        .unwrap()
        .into_active_model();
    purchase.status = Set("accepted".into());
    purchase.update(&f.app.db).await.unwrap();
    late_receipt(&f, id, "SYNTHETIC-RECOVERY-RESALE").await;
    assert_eq!(f.app.secrets_with(f.product_id, "available").await, 0);
    assert_eq!(
        procurement_orders::Entity::find()
            .count(&f.app.db)
            .await
            .unwrap(),
        1
    );
    assert!(f.app.order(f.child_id).await.status == "refunded");
}

#[tokio::test]
async fn rfd06_ambiguous_or_previously_disclosed_cards_are_held_for_review() {
    use zs_infra::db::entity::procurement_orders;
    for case in [
        "wrong-count",
        "duplicate-lines",
        "existing-stock",
        "disclosed",
    ] {
        let f = fixture().await;
        let id = refunded_purchase(
            &f,
            if case == "duplicate-lines" { 2 } else { 1 },
            "accepted",
        )
        .await;
        let payload = match case {
            "wrong-count" => "SYNTHETIC-A\nSYNTHETIC-B".to_owned(),
            "duplicate-lines" => "SYNTHETIC-A\nSYNTHETIC-A".to_owned(),
            "existing-stock" => {
                card_secrets::Entity::find()
                    .one(&f.app.db)
                    .await
                    .unwrap()
                    .unwrap()
                    .secret
            }
            _ => {
                let now = chrono::Utc::now();
                fulfillments::ActiveModel {
                    order_id: Set(f.child_id),
                    type_: Set("upstream".into()),
                    status: Set("delivered".into()),
                    payload: Set("SYNTHETIC-DISCLOSED".into()),
                    created_at: Set(now),
                    updated_at: Set(now),
                    ..Default::default()
                }
                .insert(&f.app.db)
                .await
                .unwrap();
                "SYNTHETIC-DISCLOSED".into()
            }
        };
        late_receipt(&f, id, &payload).await;
        assert_eq!(
            procurement_orders::Entity::find_by_id(id)
                .one(&f.app.db)
                .await
                .unwrap()
                .unwrap()
                .status,
            "manual_review",
            "{case}"
        );
        assert_eq!(
            f.app.secrets_with(f.product_id, "available").await,
            0,
            "{case}"
        );
        assert_eq!(f.app.order(f.child_id).await.status, "refunded");
    }
}

#[tokio::test]
async fn rfd05_safe_unsubmitted_tasks_stop_before_force_and_buyer_refunds() {
    use zs_infra::db::entity::procurement_orders;
    for (status, buyer) in [("pending", false), ("failed", false), ("pending", true)] {
        let f = fixture().await;
        upstream_waiting(&f).await;
        let id = procurement(&f, status).await;
        let detail = f
            .app
            .admin_call("GET", &format!("/api/v1/admin/orders/{}", f.order_id), None)
            .await;
        assert_eq!(detail["data"]["refund_mode"], "automatic");
        let result = if buyer {
            let order = f.app.order(f.order_id).await;
            f.app
                .call(
                    "POST",
                    &format!("/api/v1/guest/orders/{}/refund", order.order_no),
                    Some(json!({})),
                    &Auth::Guest("buyer@example.test".into(), "secret-pass".into()),
                )
                .await
        } else {
            f.refund(false).await
        };
        assert_eq!(result["status_code"], 0, "{status}/{buyer}: {result}");
        assert_eq!(f.calls.load(Ordering::SeqCst), 1);
        let stopped = procurement_orders::Entity::find_by_id(id)
            .one(&f.app.db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stopped.status, "canceled");
        assert!(stopped.next_retry_at.is_none());
        assert_eq!(f.app.order(f.order_id).await.status, "refunded");
    }
}

#[tokio::test]
async fn rfd05_accepted_task_requires_supplier_stop_confirmation_before_any_money_returns() {
    use axum::{Json, Router, http::StatusCode, routing::post};
    use zs_infra::db::entity::procurement_orders;
    for (confirmed, buyer) in [(true, false), (true, true), (false, false), (false, true)] {
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let server = Router::new().route("/api/v1/zs/orders/{id}/cancel", post(move || {
            let counter = counter.clone();
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                if confirmed { (StatusCode::OK, Json(json!({"ok":true,"data":{"status":"canceled"}}))) }
                else { (StatusCode::CONFLICT, Json(json!({"ok":false,"error":{"code":"order_not_cancelable","message":"processing","retryable":false}}))) }
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(listener, server).await.unwrap();
        });
        let f = fixture_with_policy(true).await;
        upstream_waiting(&f).await;
        let connection = f.app.admin_call("POST", "/api/v1/admin/site-connections", Some(json!({
            "name":"Cancellation fixture", "protocol":"zebra-store", "base_url":format!("http://{address}"),
            "api_key":"local-test-key", "api_secret":"local-test-secret", "callback_url":"https://buyer.example.test/events"
        }))).await;
        assert_eq!(connection["status_code"], 0, "{connection}");
        let connection = connection["data"]["id"].as_i64().unwrap();
        let id = procurement(&f, "accepted").await;
        let mut row = procurement_orders::Entity::find_by_id(id)
            .one(&f.app.db)
            .await
            .unwrap()
            .unwrap()
            .into_active_model();
        row.connection_id = Set(connection);
        row.upstream_order_no = Set("HY-LOCAL-QUEUED".into());
        row.update(&f.app.db).await.unwrap();
        let result = if buyer {
            let no = f.app.order(f.order_id).await.order_no;
            f.app
                .call(
                    "POST",
                    &format!("/api/v1/guest/orders/{no}/refund"),
                    Some(json!({})),
                    &Auth::Guest("buyer@example.test".into(), "secret-pass".into()),
                )
                .await
        } else {
            f.refund(false).await
        };
        assert_eq!(
            result["status_code"] == 0,
            confirmed,
            "confirmed={confirmed} buyer={buyer}: {result}"
        );
        assert_eq!(f.calls.load(Ordering::SeqCst), usize::from(confirmed));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let row = procurement_orders::Entity::find_by_id(id)
            .one(&f.app.db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.status, if confirmed { "canceled" } else { "accepted" });
        task.abort();
    }
}

#[tokio::test]
async fn rfd05_wallet_refund_also_stops_unsubmitted_procurement() {
    use zs_infra::db::entity::{procurement_orders, wallet_accounts};
    let f = fixture().await;
    upstream_waiting(&f).await;
    let id = procurement(&f, "pending").await;
    let (uid, _) = f.app.user("wallet-refund@example.test", 0).await;
    for order_id in [f.order_id, f.child_id] {
        let mut row = f.app.order(order_id).await.into_active_model();
        row.user_id = Set(uid);
        row.online_paid_amount = Set(order_common::dec("0"));
        row.wallet_paid_amount = Set(order_common::dec("125"));
        row.update(&f.app.db).await.unwrap();
    }
    f.app
        .services
        .order
        .service
        .refund_to_wallet(f.order_id, "125", "local wallet fixture")
        .await
        .unwrap();
    assert_eq!(
        procurement_orders::Entity::find_by_id(id)
            .one(&f.app.db)
            .await
            .unwrap()
            .unwrap()
            .status,
        "canceled"
    );
    assert_eq!(
        wallet_accounts::Entity::find()
            .filter(wallet_accounts::Column::UserId.eq(uid))
            .one(&f.app.db)
            .await
            .unwrap()
            .unwrap()
            .balance,
        order_common::dec("125")
    );
    assert_eq!(f.calls.load(Ordering::SeqCst), 0);
}
