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
    let app = OrderApp::with_transport(Arc::new(transport)).await;
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
