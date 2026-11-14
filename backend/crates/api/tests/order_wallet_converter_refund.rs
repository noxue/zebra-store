//! RFD-08 / WAL / DB-01: real wallet checkout, converter HTTP and atomic refund ledger.
#![expect(
    clippy::unwrap_used,
    reason = "isolated integration fixtures assert setup"
)]
mod order_common;
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use order_common::{Auth, OrderApp, dec};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, IntoActiveModel, PaginatorTrait,
    QueryFilter, Set,
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use zs_infra::{
    db::entity::{
        gateway_refund_attempts, order_refund_records, payments, users, wallet_accounts,
        wallet_transactions,
    },
    payment::http::MockTransport,
};
#[derive(Default)]
struct Converter {
    states: Mutex<HashMap<String, String>>,
    prepare_calls: AtomicUsize,
    confirm_calls: AtomicUsize,
    cancel_calls: AtomicUsize,
    used: AtomicBool,
    reject_second: AtomicBool,
    lost_prepare: AtomicBool,
    cancel_failure: AtomicBool,
    confirm_failure: AtomicBool,
}
fn auth(headers: &HeaderMap) {
    assert_eq!(
        headers["authorization"],
        "Bearer local-wallet-converter-token-1234567890"
    );
}
async fn exchange(headers: HeaderMap, Json(body): Json<Value>) -> Json<Value> {
    auth(&headers);
    Json(
        json!({"items":body["items"].as_array().unwrap().iter().map(|i|json!({"index":i["index"],"card":format!("CONVERTED-{}",i["upstream_card"].as_str().unwrap())})).collect::<Vec<_>>()}),
    )
}
async fn prepare(
    State(s): State<Arc<Converter>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    auth(&headers);
    s.prepare_calls.fetch_add(1, Ordering::SeqCst);
    assert_eq!(body["cards"].as_array().unwrap().len(), 1);
    if s.used.load(Ordering::SeqCst)
        || (s.reject_second.load(Ordering::SeqCst)
            && body["order_no"].as_str().unwrap().ends_with("-02"))
    {
        return Json(json!({"status":"rejected","reason":"card_used"})).into_response();
    }
    let key = body["refund_no"].as_str().unwrap().to_owned();
    let phase = {
        let mut m = s.states.lock().unwrap();
        m.entry(key).or_insert_with(|| "prepared".into()).clone()
    };
    if s.lost_prepare.load(Ordering::SeqCst) {
        return (StatusCode::OK, "<html>lost prepared reply</html>").into_response();
    }
    Json(json!({"status":phase,"reason":"unused"})).into_response()
}
async fn confirm(
    State(s): State<Arc<Converter>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    auth(&headers);
    s.confirm_calls.fetch_add(1, Ordering::SeqCst);
    if s.confirm_failure.load(Ordering::SeqCst) {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let key = body["refund_no"].as_str().unwrap().to_owned();
    let mut m = s.states.lock().unwrap();
    assert!(matches!(
        m.get(&key).map(String::as_str),
        Some("prepared" | "confirmed")
    ));
    m.insert(key, "confirmed".into());
    Json(json!({"status":"confirmed","reason":"payment_refund"})).into_response()
}
async fn cancel(
    State(s): State<Arc<Converter>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    auth(&headers);
    s.cancel_calls.fetch_add(1, Ordering::SeqCst);
    if s.cancel_failure.load(Ordering::SeqCst) {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    s.states.lock().unwrap().insert(
        body["refund_no"].as_str().unwrap().into(),
        "cancelled".into(),
    );
    Json(json!({"status":"cancelled"})).into_response()
}
async fn install_failure(db: &sea_orm::DatabaseConnection, currency: bool) {
    let sql = match (db.get_database_backend(), currency) {
        (sea_orm::DbBackend::Sqlite, false) => {
            "CREATE TRIGGER refuse_wallet_refund BEFORE INSERT ON order_refund_records BEGIN SELECT RAISE(ABORT,'injected refund transaction failure'); END"
        }
        (sea_orm::DbBackend::Postgres, false) => {
            "CREATE FUNCTION refuse_wallet_refund_fn() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected refund transaction failure'; END $$; CREATE TRIGGER refuse_wallet_refund BEFORE INSERT ON order_refund_records FOR EACH ROW EXECUTE FUNCTION refuse_wallet_refund_fn()"
        }
        (sea_orm::DbBackend::Sqlite, true) => {
            "CREATE TRIGGER corrupt_wallet_currency AFTER UPDATE OF status ON gateway_refund_attempts WHEN NEW.status='pending' BEGIN UPDATE payments SET currency='USD' WHERE id=NEW.payment_id; END"
        }
        (sea_orm::DbBackend::Postgres, true) => {
            "CREATE FUNCTION corrupt_wallet_currency_fn() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.status='pending' THEN UPDATE payments SET currency='USD' WHERE id=NEW.payment_id; END IF; RETURN NEW; END $$; CREATE TRIGGER corrupt_wallet_currency AFTER UPDATE OF status ON gateway_refund_attempts FOR EACH ROW EXECUTE FUNCTION corrupt_wallet_currency_fn()"
        }
        _ => panic!("fault fixture only supports isolated SQLite/PostgreSQL"),
    };
    db.execute_unprepared(sql).await.unwrap();
}
async fn remove_failure(db: &sea_orm::DatabaseConnection, currency: bool) {
    let sql = match (db.get_database_backend(), currency) {
        (sea_orm::DbBackend::Sqlite, false) => "DROP TRIGGER refuse_wallet_refund",
        (sea_orm::DbBackend::Postgres, false) => {
            "DROP TRIGGER refuse_wallet_refund ON order_refund_records; DROP FUNCTION refuse_wallet_refund_fn()"
        }
        (sea_orm::DbBackend::Sqlite, true) => "DROP TRIGGER corrupt_wallet_currency",
        (sea_orm::DbBackend::Postgres, true) => {
            "DROP TRIGGER corrupt_wallet_currency ON gateway_refund_attempts; DROP FUNCTION corrupt_wallet_currency_fn()"
        }
        _ => panic!("fault fixture only supports isolated SQLite/PostgreSQL"),
    };
    db.execute_unprepared(sql).await.unwrap();
}
struct Fixture {
    app: OrderApp,
    user: i64,
    token: String,
    parent: i64,
    child: i64,
    children: Vec<i64>,
    total: String,
    order_no: String,
    converter: Arc<Converter>,
    gateway: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Fixture {
    async fn new() -> Self {
        Box::pin(Self::cart(1)).await
    }
    async fn cart(lines: usize) -> Self {
        let converter = Arc::new(Converter::default());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let router = Router::new()
            .route("/health", get(|| async { Json(json!({"ok":true})) }))
            .route("/integration/v1/exchanges", post(exchange))
            .route("/integration/v1/refunds/prepare", post(prepare))
            .route("/integration/v1/refunds/confirm", post(confirm))
            .route("/integration/v1/refunds/cancel", post(cancel))
            .with_state(converter.clone());
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let gateway = Arc::new(AtomicUsize::new(0));
        let calls = gateway.clone();
        let app = OrderApp::with_converter_policy(
            Arc::new(MockTransport::new(move |_| {
                calls.fetch_add(1, Ordering::SeqCst);
                Err("wallet must not call a gateway".into())
            })),
            true,
        )
        .await;
        let now = chrono::Utc::now();
        let row = zs_infra::db::entity::extra::card_converters::ActiveModel {
            name: Set("local wallet converter".into()),
            base_url: Set(format!("http://{address}")),
            token_enc: Set(app
                .ctx
                .cipher
                .encrypt("local-wallet-converter-token-1234567890")
                .unwrap()),
            enabled: Set(true),
            health: Set("unknown".into()),
            consecutive_failures: Set(0),
            last_checked_at: Set(None),
            last_error: Set(String::new()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&app.db)
        .await
        .unwrap();
        let mut items = Vec::new();
        for index in 0..lines {
            let (product, sku) = app
                .product(
                    &format!("wallet-converted-{index}"),
                    json!({"price_amount":"10.00"}),
                )
                .await;
            app.secrets(product, sku, 1).await;
            app.services
                .integration
                .card_converters
                .save_binding(zs_domain::integration::card_converter::Binding {
                    id: 0,
                    product_id: product,
                    sku_id: sku,
                    converter_id: row.id,
                    type_id: String::new(),
                    extra_template: json!({"provider":"local"}),
                    issues: vec![],
                    created_at: now,
                    updated_at: now,
                })
                .await
                .unwrap();
            items.push(json!({"product_id":product,"sku_id":sku,"quantity":1}));
        }
        let total = format!("{}.00", lines * 10);
        let (user, token) = app.user("wallet-refund@fixture.invalid", 0).await;
        app.fund(user, &total).await;
        let result = app
            .call(
                "POST",
                "/api/v1/orders/create-and-pay",
                Some(json!({"items":items,"use_balance":true,"channel_id":0})),
                &Auth::Bearer(token.clone()),
            )
            .await;
        assert_eq!(result["status_code"], 0, "{result}");
        assert_eq!(result["data"]["order_paid"], true);
        let order_no = result["data"]["order_no"].as_str().unwrap().to_owned();
        let parent = app.order_by_no(&order_no).await.id;
        let children: Vec<_> = app.children(parent).await.iter().map(|c| c.id).collect();
        let child = children[0];
        for id in &children {
            app.services.order.service.auto_fulfill(*id).await.unwrap();
        }
        assert!(
            app.services
                .order
                .service
                .admin_order_detail(child)
                .await
                .unwrap()
                .order
                .fulfillment
                .unwrap()
                .payload
                .starts_with("CONVERTED-")
        );
        Self {
            app,
            user,
            token,
            parent,
            child,
            children,
            total,
            order_no,
            converter,
            gateway,
            task,
        }
    }
    async fn refund(&self, amount: &str) -> Value {
        self.app
            .admin_call(
                "POST",
                &format!("/api/v1/admin/orders/{}/refund-to-wallet", self.parent),
                Some(json!({"amount":amount,"remark":"original-wallet-refund"})),
            )
            .await
    }
    async fn attempt(&self) -> zs_infra::db::entity::gateway_refund_attempts::Model {
        gateway_refund_attempts::Entity::find()
            .one(&self.app.db)
            .await
            .unwrap()
            .unwrap()
    }
    async fn balance(&self) -> sea_orm::prelude::Decimal {
        wallet_accounts::Entity::find()
            .filter(wallet_accounts::Column::UserId.eq(self.user))
            .one(&self.app.db)
            .await
            .unwrap()
            .unwrap()
            .balance
    }
    async fn credited_once(&self) {
        assert_eq!(self.balance().await, dec(&self.total));
        assert_eq!(
            order_refund_records::Entity::find()
                .count(&self.app.db)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            wallet_transactions::Entity::find()
                .filter(wallet_transactions::Column::Type.eq("admin_refund"))
                .count(&self.app.db)
                .await
                .unwrap(),
            1
        );
        assert_eq!(self.gateway.load(Ordering::SeqCst), 0);
        assert_eq!(self.app.order(self.parent).await.status, "refunded");
        assert_eq!(self.app.order(self.child).await.status, "refunded");
    }
}
#[tokio::test]
async fn rfd08_wallet_converted_refund_is_atomic_idempotent_and_concurrent() {
    let f = Fixture::new().await;
    let (a, b) = tokio::join!(f.refund("10"), f.refund("10"));
    assert_eq!(a["status_code"], 0, "{a}");
    assert_eq!(b["status_code"], 0, "{b}");
    assert_eq!(f.refund("10").await["status_code"], 0);
    f.credited_once().await;
    let r = f.attempt().await;
    assert!(r.refund_no.starts_with("WR"));
    assert_eq!(r.channel_id, 0);
    assert_eq!(r.status, "succeeded");
    assert!(r.card_confirmed);
}
#[tokio::test]
async fn rfd08_confirmation_failure_recovers_without_second_wallet_credit() {
    let f = Fixture::new().await;
    f.converter.confirm_failure.store(true, Ordering::SeqCst);
    let r = f.refund("10").await;
    assert_eq!(r["status_code"], 0, "{r}");
    assert_eq!(r["data"]["refund_state"], "syncing");
    assert!(!f.attempt().await.card_confirmed);
    f.credited_once().await;
    let original = f.attempt().await.refund_no;
    f.converter.confirm_failure.store(false, Ordering::SeqCst);
    f.app
        .services
        .order
        .service
        .reconcile_gateway_refunds()
        .await
        .unwrap();
    f.credited_once().await;
    assert!(f.attempt().await.card_confirmed);
    assert_eq!(f.attempt().await.refund_no, original);
}
#[tokio::test]
async fn rfd08_used_card_refuses_credit_and_only_retry_after_durable_cancel() {
    let f = Fixture::new().await;
    f.converter.used.store(true, Ordering::SeqCst);
    assert_ne!(f.refund("10").await["status_code"], 0);
    assert_eq!(f.balance().await, dec("0"));
    assert_eq!(f.attempt().await.status, "failed");
    let old = f.attempt().await.refund_no;
    assert_eq!(f.converter.states.lock().unwrap()[&old], "cancelled");
    f.converter.used.store(false, Ordering::SeqCst);
    assert_eq!(f.refund("10").await["status_code"], 0);
    f.credited_once().await;
    assert_eq!(
        gateway_refund_attempts::Entity::find()
            .count(&f.app.db)
            .await
            .unwrap(),
        2
    );
}
#[tokio::test]
async fn rfd08_lost_prepare_and_unknown_cancel_keep_money_and_original_intent() {
    let f = Fixture::new().await;
    f.converter.lost_prepare.store(true, Ordering::SeqCst);
    f.converter.cancel_failure.store(true, Ordering::SeqCst);
    assert_ne!(f.refund("10").await["status_code"], 0);
    let original = f.attempt().await;
    assert_eq!(original.status, "wallet_canceling");
    assert_eq!(f.balance().await, dec("0"));
    // RFD-05: even another service entry cannot bypass the in-flight cancellation fence.
    use zs_domain::order::ports::{OrderStore, RefundRequest};
    let store = zs_infra::db::repo::order::SeaOrderStore::new(
        f.app.db.clone(),
        "local-unused-guest-secret",
    );
    let competing = store
        .refund(&RefundRequest {
            order_id: f.parent,
            amount: "10".parse().unwrap(),
            remark: "competing direct credit".into(),
            to_wallet: true,
            payment_fee_refunded: false,
            gateway_refund_attempt_id: None,
            gateway_refund_provider_ref: String::new(),
            gateway_refund_payload: Default::default(),
            max_refund_days: 30,
            reseller_confirm_days: 7,
            now: chrono::Utc::now(),
        })
        .await
        .unwrap_err();
    assert_eq!(competing.key(), "error.gateway_refund_pending");
    assert_eq!(f.balance().await, dec("0"));

    f.app
        .services
        .order
        .service
        .reconcile_gateway_refunds()
        .await
        .unwrap();
    assert_eq!(f.converter.cancel_calls.load(Ordering::SeqCst), 1);
    gateway_refund_attempts::Entity::update_many()
        .col_expr(
            gateway_refund_attempts::Column::UpdatedAt,
            sea_orm::sea_query::Expr::value(chrono::Utc::now() - chrono::Duration::seconds(31)),
        )
        .exec(&f.app.db)
        .await
        .unwrap();
    f.converter.cancel_failure.store(false, Ordering::SeqCst);
    f.app
        .services
        .order
        .service
        .reconcile_gateway_refunds()
        .await
        .unwrap();
    assert_eq!(f.attempt().await.status, "failed");
    assert_eq!(f.balance().await, dec("0"));
    assert_eq!(f.attempt().await.refund_no, original.refund_no);
    assert_eq!(f.converter.prepare_calls.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn rfd08_wallet_transaction_failure_rolls_back_then_resumes_original_prepared_refund() {
    let f = Fixture::new().await;
    install_failure(&f.app.db, false).await;
    assert_ne!(f.refund("10").await["status_code"], 0);
    assert_eq!(f.balance().await, dec("0"));
    assert_eq!(f.attempt().await.status, "pending");
    assert_eq!(f.app.order(f.parent).await.refunded_amount, dec("0"));
    remove_failure(&f.app.db, false).await;
    f.app
        .services
        .order
        .service
        .reconcile_gateway_refunds()
        .await
        .unwrap();
    f.credited_once().await;
    assert_eq!(f.converter.prepare_calls.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn rfd08_payment_revalidated_inside_wallet_credit_transaction() {
    let f = Fixture::new().await;
    install_failure(&f.app.db, true).await;
    assert_ne!(f.refund("10").await["status_code"], 0);
    assert_eq!(f.balance().await, dec("0"));
    assert_eq!(
        order_refund_records::Entity::find()
            .count(&f.app.db)
            .await
            .unwrap(),
        0
    );
    assert_eq!(f.attempt().await.status, "pending");
    remove_failure(&f.app.db, true).await;
    payments::Entity::update_many()
        .col_expr(
            payments::Column::Currency,
            sea_orm::sea_query::Expr::value("CNY"),
        )
        .exec(&f.app.db)
        .await
        .unwrap();
    f.app
        .services
        .order
        .service
        .reconcile_gateway_refunds()
        .await
        .unwrap();
    f.credited_once().await;
}
#[tokio::test]
async fn rfd08_invalid_partial_mixed_child_and_expired_requests_do_not_prepare_or_credit() {
    let f = Fixture::new().await;
    for amount in ["0", "-1", "11", "5"] {
        assert_ne!(f.refund(amount).await["status_code"], 0, "{amount}");
    }
    let r = f
        .app
        .admin_call(
            "POST",
            &format!("/api/v1/admin/orders/{}/refund-to-wallet", f.child),
            Some(json!({"amount":"10"})),
        )
        .await;
    assert_ne!(r["status_code"], 0);
    let mut order = f.app.order(f.parent).await.into_active_model();
    order.wallet_paid_amount = Set(dec("5"));
    order.online_paid_amount = Set(dec("5"));
    let order = order.update(&f.app.db).await.unwrap();
    assert_ne!(f.refund("10").await["status_code"], 0);
    let mut order = order.into_active_model();
    order.wallet_paid_amount = Set(dec("10"));
    order.online_paid_amount = Set(dec("0"));
    order.paid_at = Set(Some(chrono::Utc::now() - chrono::Duration::days(31)));
    order.update(&f.app.db).await.unwrap();
    assert_ne!(f.refund("10").await["status_code"], 0);
    assert_eq!(f.balance().await, dec("0"));
    assert_eq!(f.converter.prepare_calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        gateway_refund_attempts::Entity::find()
            .count(&f.app.db)
            .await
            .unwrap(),
        0
    );
}
#[tokio::test]
async fn rfd08_owner_refund_route_and_disabled_owner_admin_refund() {
    let f = Fixture::new().await;
    let detail = f
        .app
        .call(
            "GET",
            &format!("/api/v1/orders/{}", f.order_no),
            None,
            &Auth::Bearer(f.token.clone()),
        )
        .await;
    assert_eq!(detail["data"]["refund_mode"], "automatic");
    let (_, other) = f.app.user("other@fixture.invalid", 0).await;
    assert_ne!(
        f.app
            .call(
                "POST",
                &format!("/api/v1/orders/{}/refund", f.order_no),
                Some(json!({})),
                &Auth::Bearer(other)
            )
            .await["status_code"],
        0
    );
    let result = f
        .app
        .call(
            "POST",
            &format!("/api/v1/orders/{}/refund", f.order_no),
            Some(json!({})),
            &Auth::Bearer(f.token.clone()),
        )
        .await;
    assert_eq!(result["status_code"], 0, "{result}");
    assert_eq!(result["data"]["status"], "succeeded");
    f.credited_once().await;
    let disabled = Fixture::new().await;
    users::Entity::update_many()
        .col_expr(
            users::Column::Status,
            sea_orm::sea_query::Expr::value("disabled"),
        )
        .filter(users::Column::Id.eq(disabled.user))
        .exec(&disabled.app.db)
        .await
        .unwrap();
    assert_eq!(disabled.refund("10").await["status_code"], 0);
    disabled.credited_once().await;
}

#[tokio::test]
async fn rfd08_accepted_refund_survives_window_expiry_and_pending_cannot_be_cancelled() {
    use zs_domain::order::ports::OrderStore;
    let f = Fixture::new().await;
    install_failure(&f.app.db, false).await;
    assert_ne!(f.refund("10").await["status_code"], 0);
    let original = f.attempt().await;
    assert_eq!(original.status, "pending");
    let store = zs_infra::db::repo::order::SeaOrderStore::new(
        f.app.db.clone(),
        "local-unused-guest-secret",
    );
    assert!(
        !store
            .claim_wallet_refund_cancellation(
                original.id,
                chrono::Utc::now() + chrono::Duration::seconds(60)
            )
            .await
            .unwrap()
    );
    assert_eq!(f.attempt().await.status, "pending");
    remove_failure(&f.app.db, false).await;
    let mut row = f.app.order(f.parent).await.into_active_model();
    row.paid_at = Set(Some(chrono::Utc::now() - chrono::Duration::days(31)));
    row.update(&f.app.db).await.unwrap();
    f.app
        .services
        .order
        .service
        .reconcile_gateway_refunds()
        .await
        .unwrap();
    f.credited_once().await;
    assert_eq!(f.attempt().await.refund_no, original.refund_no);
    let record = order_refund_records::Entity::find()
        .one(&f.app.db)
        .await
        .unwrap()
        .unwrap();
    assert!(record.created_at >= original.created_at);
}

#[tokio::test]
async fn rfd08_cart_refund_cancels_all_child_locks_before_retrying_one_wallet_credit() {
    let f = Fixture::cart(2).await;
    assert_eq!(f.children.len(), 2);
    f.converter.reject_second.store(true, Ordering::SeqCst);
    assert_ne!(f.refund("20").await["status_code"], 0);
    assert_eq!(f.balance().await, dec("0"));
    assert_eq!(f.converter.cancel_calls.load(Ordering::SeqCst), 2);
    assert_eq!(f.attempt().await.status, "failed");
    f.converter.reject_second.store(false, Ordering::SeqCst);
    assert_eq!(f.refund("20").await["status_code"], 0);
    f.credited_once().await;
    for child in &f.children {
        assert_eq!(f.app.order(*child).await.status, "refunded");
    }
    assert_eq!(f.converter.confirm_calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn rfd08_new_intent_rechecks_remaining_amount_under_the_family_lock() {
    use zs_domain::{
        order::ports::OrderStore,
        payment::refund::{AdminRefundOptions, NewGatewayRefundAttempt},
    };
    let f = Fixture::new().await;
    let payment = payments::Entity::find()
        .filter(payments::Column::OrderId.eq(f.parent))
        .one(&f.app.db)
        .await
        .unwrap()
        .unwrap();
    // Represents a sibling refund completing after the caller's initial preflight.
    let mut order = f.app.order(f.parent).await.into_active_model();
    order.refunded_amount = Set(dec("1"));
    order.status = Set("partially_refunded".into());
    order.update(&f.app.db).await.unwrap();
    let store = zs_infra::db::repo::order::SeaOrderStore::new(
        f.app.db.clone(),
        "local-unused-guest-secret",
    );
    let rejected = store
        .reserve_gateway_refund(&NewGatewayRefundAttempt {
            order_id: f.parent,
            payment_id: payment.id,
            channel_id: 0,
            request_date: String::new(),
            refund_no: "WR-local-stale-preflight".into(),
            provider_ref: String::new(),
            amount: "10".parse().unwrap(),
            remark: "original-wallet-refund".into(),
            payment_fee_refunded: false,
            now: chrono::Utc::now(),
            initial_status: "preparing".into(),
            admin_options: AdminRefundOptions::default(),
        })
        .await;
    assert!(rejected.is_err());
    assert_eq!(f.converter.prepare_calls.load(Ordering::SeqCst), 0);
    assert_eq!(f.balance().await, dec("0"));
    assert_eq!(
        gateway_refund_attempts::Entity::find()
            .count(&f.app.db)
            .await
            .unwrap(),
        0
    );
}
