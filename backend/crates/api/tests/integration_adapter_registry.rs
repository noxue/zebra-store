//! The integration core is protocol-agnostic: a third supplier system registered only
//! in this test (an in-memory "ACME ERP" adapter, no HTTP at all) goes through the same
//! admin API, negotiation, import, change-feed sync, quote + idempotent procurement and
//! inbound event handling (dedupe, ownership, state machine) as the built-in adapters,
//! plus the neutral synchronous-delivery / manual-review outcomes of `place_order` and
//! the keep-on-empty rule of `secret` configuration fields.

#![expect(clippy::unwrap_used, reason = "integration tests")]

mod integration_common;

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use integration_common::{
    FakeLifecycle, FakeOrdering, IntApp, data, jobs_of, seed_item, seed_order_row,
};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde_json::{Value, json};
use zs_domain::Id;
use zs_domain::integration::adapter::{
    AdapterMeta, Capabilities, Capability, CatalogChange, ChangeKind, ChangePage, ConfigField,
    FieldKind, HandshakeInfo, InboundError, InboundEvent, InboundKind, InboundRequest, OrderLine,
    OrderNotice, OrderRef, PlaceOrder, Quote, QuoteLine, SupplierAdapter, text3,
};
use zs_domain::integration::connection::Endpoint;
use zs_domain::integration::procurement::RESULT_UNKNOWN;
use zs_domain::integration::protocol::{
    CategoryList, CreateOrderResponse, Download, PingInfo, ProductPage, ProductQuery,
    RemoteFulfillment, RemoteOrder, RemoteProduct, RemoteSku, UpstreamClient, UpstreamError,
};
use zs_domain::queue::kinds;
use zs_infra::db::entity::{
    integration_connection_states, procurement_deliveries, procurement_orders, product_skus,
    sku_mappings,
};
use zs_infra::integration::http::AddressPolicy;
use zs_infra::wire::integration::Adapters;

const ID: &str = "acme-erp";

/// How the fake supplier answers `place_order`.
#[derive(Debug, Clone, Default, PartialEq)]
enum Placing {
    /// Accepted, delivered later (callback / poll).
    #[default]
    Accepted,
    /// Accepted with the goods in the answer.
    Delivered(String),
    /// Outcome unknown (lost answer of a non-idempotent call).
    Unknown,
    /// Charged but needs a human (paid without goods).
    Review,
}

/// What the fake supplier knows and records.
#[derive(Default)]
struct Acme {
    stock: Mutex<i64>,
    changes: Mutex<Vec<CatalogChange>>,
    placed: Mutex<Vec<PlaceOrder>>,
    quotes: Mutex<u32>,
    placing: Mutex<Placing>,
}

struct AcmeAdapter(Arc<Acme>);

impl AcmeAdapter {
    fn caps() -> Capabilities {
        Capabilities::of(&[
            Capability::Categories,
            Capability::IncrementalChanges,
            Capability::Quote,
        ])
    }
}

impl SupplierAdapter for AcmeAdapter {
    fn meta(&self) -> AdapterMeta {
        AdapterMeta {
            id: ID,
            name: text3("ACME ERP", "ACME ERP", "ACME ERP"),
            description: text3("测试系统", "測試系統", "Test system"),
            fields: vec![
                ConfigField {
                    key: "base_url",
                    label: text3("地址", "地址", "URL"),
                    kind: FieldKind::Url,
                    required: true,
                    placeholder: text3("", "", ""),
                    options: Vec::new(),
                },
                ConfigField {
                    key: "tenant",
                    label: text3("租户", "租戶", "Tenant"),
                    kind: FieldKind::Select,
                    required: false,
                    placeholder: text3("", "", ""),
                    options: vec![json!("eu"), json!("us")],
                },
                ConfigField {
                    key: "token",
                    label: text3("令牌", "令牌", "Token"),
                    kind: FieldKind::Secret,
                    required: false,
                    placeholder: text3("", "", ""),
                    options: Vec::new(),
                },
            ],
            capabilities: Self::caps(),
            supports_connection_code: false,
            inbound_path: "/api/v1/acme/hook",
        }
    }

    fn open(&self, endpoint: &Endpoint) -> zs_domain::Result<Arc<dyn UpstreamClient>> {
        Ok(Arc::new(AcmeClient {
            acme: self.0.clone(),
            secret: endpoint.api_secret.clone(),
            tenant: endpoint
                .extra
                .get("tenant")
                .and_then(Value::as_str)
                .unwrap_or("?")
                .to_owned(),
        }))
    }

    fn inbound_key(
        &self,
        req: &InboundRequest<'_>,
        _now: DateTime<Utc>,
    ) -> Result<String, InboundError> {
        let key = req.header("x-acme-key");
        if key.is_empty() {
            return Err(InboundError::MissingHeaders);
        }
        Ok(key)
    }

    fn parse_inbound(
        &self,
        req: &InboundRequest<'_>,
        secret: &str,
        _now: DateTime<Utc>,
    ) -> Result<Vec<InboundEvent>, InboundError> {
        if req.header("x-acme-sig") != secret {
            return Err(InboundError::InvalidSignature);
        }
        let v: Value = serde_json::from_slice(req.body).map_err(|_| InboundError::InvalidBody)?;
        let s = |k: &str| v[k].as_str().unwrap_or_default().to_owned();
        Ok(vec![InboundEvent {
            event_id: Some(s("id")),
            kind: InboundKind::Order(OrderNotice {
                downstream_order_no: s("ref"),
                upstream: OrderRef {
                    id: 0,
                    no: s("order"),
                },
                status: s("state"),
                fulfillment: Some(RemoteFulfillment {
                    kind: "auto".into(),
                    status: "delivered".into(),
                    payload: s("code"),
                    ..RemoteFulfillment::default()
                }),
            }),
        }])
    }
}

struct AcmeClient {
    acme: Arc<Acme>,
    secret: String,
    /// Adapter-specific configuration from the connection's `extra`.
    tenant: String,
}

impl AcmeClient {
    fn product(&self) -> RemoteProduct {
        let mut title = serde_json::Map::new();
        title.insert("en-US".into(), json!("Widget"));
        RemoteProduct {
            id: 7,
            title,
            price_amount: "3.00".into(),
            fulfillment_type: "auto".into(),
            is_active: true,
            skus: vec![RemoteSku {
                id: 70,
                sku_code: "W".into(),
                price_amount: "3.00".into(),
                stock_quantity: *self.acme.stock.lock().unwrap(),
                is_active: true,
                ..RemoteSku::default()
            }],
            ..RemoteProduct::default()
        }
    }
}

#[async_trait]
impl UpstreamClient for AcmeClient {
    fn capabilities(&self) -> Capabilities {
        AcmeAdapter::caps()
    }

    fn addressable(&self, order: &OrderRef) -> bool {
        !order.no.is_empty()
    }

    async fn handshake(&self) -> Result<HandshakeInfo, UpstreamError> {
        if self.secret != "acme-secret" {
            return Err(UpstreamError::Http {
                status: 401,
                code: "denied".into(),
                message: "bad secret".into(),
            });
        }
        Ok(HandshakeInfo {
            protocol: ID.into(),
            version: "7".into(),
            site_name: format!("ACME {}", self.tenant),
            currency: "EUR".into(),
            features: AcmeAdapter::caps()
                .names()
                .into_iter()
                .map(str::to_owned)
                .collect(),
            balance: "50.00".into(),
            account_currency: "EUR".into(),
            change_head: Some("100".into()),
            ..HandshakeInfo::default()
        })
    }

    async fn ping(&self) -> Result<PingInfo, UpstreamError> {
        Err(UpstreamError::Unsupported)
    }

    async fn list_categories(&self) -> Result<CategoryList, UpstreamError> {
        Ok(CategoryList::default())
    }

    async fn list_products(&self, _q: &ProductQuery) -> Result<ProductPage, UpstreamError> {
        Ok(ProductPage {
            total: 1,
            items: vec![self.product()],
            includes_inactive: true,
            ..ProductPage::default()
        })
    }

    async fn get_product(&self, id: Id) -> Result<RemoteProduct, UpstreamError> {
        if id == 7 {
            Ok(self.product())
        } else {
            Err(UpstreamError::ProductDeleted)
        }
    }

    async fn place_order(&self, req: &PlaceOrder) -> Result<CreateOrderResponse, UpstreamError> {
        let n = {
            let mut placed = self.acme.placed.lock().unwrap();
            placed.push(req.clone());
            placed.len()
        };
        let accepted = CreateOrderResponse {
            ok: true,
            order_no: format!("ACME-{n}"),
            status: "paid".into(),
            amount: "3.00".into(),
            currency: "EUR".into(),
            ..CreateOrderResponse::default()
        };
        let placing = self.acme.placing.lock().unwrap().clone();
        Ok(match placing {
            Placing::Accepted => accepted,
            Placing::Delivered(code) => CreateOrderResponse {
                fulfillment: Some(RemoteFulfillment {
                    kind: "auto".into(),
                    status: "delivered".into(),
                    payload: code,
                    ..RemoteFulfillment::default()
                }),
                ..accepted
            },
            Placing::Review => CreateOrderResponse {
                review: Some("charged without goods".into()),
                ..accepted
            },
            Placing::Unknown => CreateOrderResponse {
                ok: false,
                error_code: RESULT_UNKNOWN.into(),
                error_message: "answer lost, check manually".into(),
                ..CreateOrderResponse::default()
            },
        })
    }

    async fn get_order(&self, order: &OrderRef) -> Result<RemoteOrder, UpstreamError> {
        Ok(RemoteOrder {
            order_no: order.no.clone(),
            status: "paid".into(),
            ..RemoteOrder::default()
        })
    }

    async fn cancel_order(&self, _order: &OrderRef) -> Result<(), UpstreamError> {
        Ok(())
    }

    async fn download(&self, _url: &str) -> Result<Download, UpstreamError> {
        Err(UpstreamError::Unsupported)
    }

    async fn changes(&self, since: Option<&str>, _limit: i64) -> Result<ChangePage, UpstreamError> {
        let since: i64 = since.unwrap_or("0").parse().unwrap_or(0);
        let changes: Vec<CatalogChange> = self
            .acme
            .changes
            .lock()
            .unwrap()
            .iter()
            .filter(|c| c.cursor.parse::<i64>().unwrap_or(0) > since)
            .cloned()
            .collect();
        let next = changes
            .last()
            .map_or_else(|| since.to_string(), |c| c.cursor.clone());
        Ok(ChangePage {
            changes,
            next_cursor: next,
            has_more: false,
        })
    }

    async fn change_head(&self) -> Result<String, UpstreamError> {
        Ok("100".into())
    }

    async fn quote(&self, lines: &[OrderLine]) -> Result<Quote, UpstreamError> {
        *self.acme.quotes.lock().unwrap() += 1;
        Ok(Quote {
            quote_id: "acme-q".into(),
            lines: lines
                .iter()
                .map(|l| QuoteLine {
                    sku_id: l.sku_id,
                    quantity: l.quantity,
                    unit_price: "3.00".into(),
                    available: true,
                    reason: None,
                })
                .collect(),
            total: "3.00".into(),
            sufficient_balance: true,
            ..Quote::default()
        })
    }
}

/// A site with the ACME adapter registered, logged in as admin.
async fn boot(acme: &Arc<Acme>) -> (IntApp, Arc<FakeLifecycle>) {
    let cfg = integration_common::config();
    let db = zs_infra::db::connect(&cfg.database).await.unwrap();
    zs_infra::db::sync_schema(&db).await.unwrap();
    let ctx = zs_infra::wire::WireCtx::new(&db, &cfg);
    let mut services = zs_infra::wire::services(&ctx);
    let lifecycle = Arc::new(FakeLifecycle::default());
    let ordering = Arc::new(FakeOrdering {
        db: db.clone(),
        fail_payment: Mutex::new(false),
        placed: Mutex::new(0),
    });
    services.integration = zs_infra::wire::integration::build_with(
        &ctx,
        &Adapters {
            address_policy: AddressPolicy::PublicOnly,
            ordering: Some(ordering.clone()),
            lifecycle: Some(lifecycle.clone()),
            extra_adapters: vec![Arc::new(AcmeAdapter(acme.clone()))],
        },
    );
    zs_infra::wire::bootstrap(&ctx, &services).await.unwrap();
    let router = zs_api::build(zs_api::AppState::new(services.clone(), cfg)).router;
    let mut app = IntApp {
        router,
        db: db.clone(),
        services,
        admin_token: String::new(),
        lifecycle: lifecycle.clone(),
        ordering,
    };
    let login = app
        .call(
            "POST",
            "/api/v1/admin/login",
            Some(json!({"username": "admin", "password": "Admin12345"})),
            None,
        )
        .await;
    app.admin_token = login["data"]["token"].as_str().unwrap().to_owned();
    app.acknowledge_compliance().await;
    (app, lifecycle)
}

#[tokio::test]
async fn third_adapter_runs_through_the_neutral_core() {
    let acme = Arc::new(Acme::default());
    *acme.stock.lock().unwrap() = 9;
    let (app, lifecycle) = boot(&acme).await;

    // the registry lists the built-in systems and the test one with its own form
    let res = app
        .admin("GET", "/api/v1/admin/site-connections/protocols", None)
        .await;
    let list = data(&res).as_array().unwrap().clone();
    let ids: Vec<&str> = list.iter().map(|p| p["id"].as_str().unwrap()).collect();
    assert_eq!(
        ids,
        vec!["dujiao-next", "zebra-store", "acg-faka", "mcy-shop", ID]
    );
    let acme_meta = &list[4];
    assert_eq!(acme_meta["fields"][1]["key"], "tenant");
    assert_eq!(acme_meta["fields"][1]["kind"], "select");
    assert_eq!(
        acme_meta["capabilities"],
        json!(["categories", "incremental_changes", "quote"])
    );
    assert_eq!(list[1]["supports_connection_code"], true);
    assert_eq!(list[0]["capabilities"], json!(["categories"]));

    // unknown protocols are refused
    let bad = app
        .admin(
            "POST",
            "/api/v1/admin/site-connections",
            Some(json!({"name": "x", "base_url": "https://x.example", "api_key": "k", "api_secret": "s", "protocol": "nope"})),
        )
        .await;
    assert_eq!(bad["status_code"], 400);

    // create: negotiated through the adapter (no HTTP), activated
    let res = app
        .admin(
            "POST",
            "/api/v1/admin/site-connections",
            Some(json!({"name": "ACME", "base_url": "https://acme.example", "api_key": "acme-key", "api_secret": "acme-secret", "protocol": ID, "auto_sync_price": true, "extra": {"tenant": "eu", "token": "t0p"}})),
        )
        .await;
    let conn = data(&res)["id"].as_i64().unwrap();
    // adapter-specific configuration reaches the adapter; secret fields are masked
    let got = app
        .admin(
            "GET",
            &format!("/api/v1/admin/site-connections/{conn}"),
            None,
        )
        .await;
    assert_eq!(data(&got)["extra"], json!({"tenant": "eu", "token": ""}));
    // editing: the masked (empty) or omitted secret field keeps the stored value, a new
    // value replaces it, and it is never returned in clear
    let stored_extra = || async {
        integration_connection_states::Entity::find()
            .filter(integration_connection_states::Column::ConnectionId.eq(conn))
            .one(&app.db)
            .await
            .unwrap()
            .unwrap()
            .extra
            .unwrap()
    };
    for (sent, token) in [
        (json!({"tenant": "eu", "token": ""}), "t0p"),
        (json!({"tenant": "eu"}), "t0p"),
        (json!({"tenant": "eu", "token": null}), "t0p"),
        (json!({"tenant": "eu", "token": "n3w"}), "n3w"),
    ] {
        let res = app
            .admin(
                "PUT",
                &format!("/api/v1/admin/site-connections/{conn}"),
                Some(json!({"extra": sent})),
            )
            .await;
        assert_eq!(data(&res)["extra"], json!({"tenant": "eu", "token": ""}));
        assert_eq!(
            stored_extra().await,
            json!({"tenant": "eu", "token": token}),
            "{sent}"
        );
    }
    let ping = app
        .admin(
            "POST",
            &format!("/api/v1/admin/site-connections/{conn}/ping"),
            None,
        )
        .await;
    assert_eq!(data(&ping)["site_name"], "ACME eu");
    assert_eq!(data(&ping)["balance"], "50.00");
    let got = app
        .admin(
            "GET",
            &format!("/api/v1/admin/site-connections/{conn}"),
            None,
        )
        .await;
    let c = data(&got);
    assert_eq!(c["status"], "active");
    assert_eq!(c["supplier_currency"], "EUR");
    assert_eq!(c["sync_mode"], "incremental");
    assert_eq!(c["webhook_status"], "unsupported");
    let hs = app
        .admin(
            "POST",
            "/api/v1/admin/site-connections/handshake",
            Some(json!({"base_url": "https://acme.example", "api_key": "acme-key", "api_secret": "acme-secret", "protocol": ID})),
        )
        .await;
    assert_eq!(data(&hs)["ok"], true);
    assert_eq!(
        data(&hs)["suggested_exchange_rate"],
        Value::Null,
        "EUR ≠ CNY"
    );
    assert!(
        data(&hs)["suggested_callback_url"]
            .as_str()
            .unwrap()
            .ends_with("/api/v1/acme/hook")
    );

    // import + full sync sets the cursor to the adapter's head
    let res = app
        .admin(
            "POST",
            "/api/v1/admin/product-mappings/import",
            Some(json!({"connection_id": conn, "upstream_product_id": 7})),
        )
        .await;
    let product_id = data(&res)["local_product_id"].as_i64().unwrap();
    let sku = product_skus::Entity::find()
        .filter(product_skus::Column::ProductId.eq(product_id))
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    let mappings = &app.services.integration.mappings;
    mappings.sync_connection_now(conn).await.unwrap();
    let state = app
        .services
        .integration
        .connections
        .get(conn)
        .await
        .unwrap();
    assert_eq!(state.state.last_change_seq, 100);

    // change feed: stock change → re-read → applied
    *acme.stock.lock().unwrap() = 2;
    acme.changes.lock().unwrap().push(CatalogChange {
        cursor: "101".into(),
        kind: ChangeKind::SkuStock,
        product_id: 7,
        sku_id: Some(70),
        product: None,
    });
    mappings.sync_connection_now(conn).await.unwrap();
    let m = sku_mappings::Entity::find()
        .filter(sku_mappings::Column::LocalSkuId.eq(sku.id))
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(m.upstream_stock, 2);
    let state = app
        .services
        .integration
        .connections
        .get(conn)
        .await
        .unwrap();
    assert_eq!(state.state.last_change_seq, 101);

    // procurement: quote first (capability), idempotency key, order number reference
    let oid = seed_order_row(&app.db, "L-1", 1, "paid", "3.00", None).await;
    seed_item(&app.db, oid, product_id, sku.id, 1, "upstream").await;
    app.services
        .integration
        .order_events
        .order_paid(oid)
        .await
        .unwrap();
    let p = procurement_orders::Entity::find()
        .filter(procurement_orders::Column::LocalOrderId.eq(oid))
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    app.services
        .integration
        .procurement
        .submit(p.id)
        .await
        .unwrap();
    assert_eq!(*acme.quotes.lock().unwrap(), 1);
    let placed = acme.placed.lock().unwrap().clone();
    assert_eq!(placed.len(), 1);
    assert_eq!(placed[0].idempotency_key, format!("procurement:{}", p.id));
    assert_eq!(placed[0].quote_id.as_deref(), Some("acme-q"));
    assert_eq!(placed[0].lines[0].sku_id, 70);
    let p = procurement_orders::Entity::find_by_id(p.id)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(p.status, "accepted");
    assert_eq!(p.upstream_order_no, "ACME-1");

    // inbound: adapter verifies, the neutral core dedupes and applies once
    let event = |id: &str, order: &str| {
        json!({"id": id, "ref": "L-1", "order": order, "state": "delivered", "code": "ACME-CODE"})
            .to_string()
    };
    let inbound = |secret: &'static str, body: String| {
        let services = app.services.clone();
        async move {
            let lookup = move |name: &str| match name {
                "x-acme-key" => Some("acme-key".to_owned()),
                "x-acme-sig" => Some(secret.to_owned()),
                _ => None,
            };
            services
                .integration
                .inbound
                .handle(
                    ID,
                    &InboundRequest {
                        method: "POST",
                        path: "/api/v1/acme/hook",
                        query: "",
                        headers: &lookup,
                        body: body.as_bytes(),
                    },
                )
                .await
        }
    };
    use zs_app::integration::inbound::InboundFailure;
    assert_eq!(
        inbound("wrong", event("e1", "ACME-1")).await,
        Err(InboundFailure::Adapter(InboundError::InvalidSignature))
    );
    // another supplier order number: ownership refused (UPS-03)
    assert_eq!(
        inbound("acme-secret", event("e0", "ACME-999")).await,
        Err(InboundFailure::NotFound)
    );
    assert_eq!(inbound("acme-secret", event("e1", "ACME-1")).await, Ok(1));
    assert_eq!(inbound("acme-secret", event("e1", "ACME-1")).await, Ok(0));
    // a new event id for the same delivery is refused by the state machine (UPS-02)
    assert_eq!(inbound("acme-secret", event("e2", "ACME-1")).await, Ok(1));
    let deliveries = lifecycle.deliveries.lock().unwrap().clone();
    assert_eq!(deliveries.len(), 1);
    assert_eq!(deliveries[0].1.payload, "ACME-CODE");
    let p = procurement_orders::Entity::find_by_id(p.id)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(p.status, "fulfilled");
}

/// Place a paid order of the ACME widget and submit its purchase once.
async fn procure(app: &IntApp, order_no: &str, product_id: Id, sku_id: Id) -> (Id, Id) {
    let oid = seed_order_row(&app.db, order_no, 1, "paid", "3.00", None).await;
    seed_item(&app.db, oid, product_id, sku_id, 1, "upstream").await;
    app.services
        .integration
        .order_events
        .order_paid(oid)
        .await
        .unwrap();
    let p = procurement_orders::Entity::find()
        .filter(procurement_orders::Column::LocalOrderId.eq(oid))
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    app.services
        .integration
        .procurement
        .submit(p.id)
        .await
        .unwrap();
    (oid, p.id)
}

async fn row(app: &IntApp, id: Id) -> procurement_orders::Model {
    procurement_orders::Entity::find_by_id(id)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap()
}

/// UPS-07 / ACG-02 / MCY-01 / MCY-03: synchronous deliveries survive a restart (they are
/// persisted with the supplier order); an unknown or "charged without goods" outcome is
/// held for manual review without refunding the customer, and resolved by an admin.
#[tokio::test]
async fn synchronous_delivery_and_manual_review_are_protocol_neutral() {
    let acme = Arc::new(Acme::default());
    *acme.stock.lock().unwrap() = 9;
    let (app, lifecycle) = boot(&acme).await;
    let res = app
        .admin(
            "POST",
            "/api/v1/admin/site-connections",
            Some(json!({"name": "ACME", "base_url": "https://acme.example", "api_key": "acme-key", "api_secret": "acme-secret", "protocol": ID, "extra": {"tenant": "eu", "token": "t0p"}})),
        )
        .await;
    let conn = data(&res)["id"].as_i64().unwrap();
    let res = app
        .admin(
            "POST",
            "/api/v1/admin/product-mappings/import",
            Some(json!({"connection_id": conn, "upstream_product_id": 7})),
        )
        .await;
    let product_id = data(&res)["local_product_id"].as_i64().unwrap();
    let sku = product_skus::Entity::find()
        .filter(product_skus::Column::ProductId.eq(product_id))
        .one(&app.db)
        .await
        .unwrap()
        .unwrap()
        .id;
    let procurement = app.services.integration.procurement.clone();

    // ---- goods in the order answer: stored with the order number, delivered at once
    *acme.placing.lock().unwrap() = Placing::Delivered("CODE-SYNC".into());
    let (o1, p1) = procure(&app, "S-1", product_id, sku).await;
    let r = row(&app, p1).await;
    assert_eq!(r.status, "fulfilled", "{}", r.error_message);
    assert_eq!(r.upstream_order_no, "ACME-1");
    let held = procurement_deliveries::Entity::find()
        .filter(procurement_deliveries::Column::ProcurementOrderId.eq(p1))
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(held.payload, "CODE-SYNC");
    let delivered = lifecycle.deliveries.lock().unwrap().clone();
    assert_eq!(delivered, vec![(o1, delivered[0].1.clone())]);
    assert_eq!(delivered[0].1.payload, "CODE-SYNC");

    // ---- the local delivery fails right after acceptance (≈ crash / restart): the
    // stored delivery is applied by the next poll without asking the supplier again
    *acme.placing.lock().unwrap() = Placing::Delivered("CODE-LATE".into());
    *lifecycle.fail_delivery.lock().unwrap() = true;
    let (o2, p2) = procure(&app, "S-2", product_id, sku).await;
    assert_eq!(row(&app, p2).await.status, "accepted");
    *lifecycle.fail_delivery.lock().unwrap() = false;
    procurement.poll(p2).await.unwrap();
    assert_eq!(row(&app, p2).await.status, "fulfilled");
    let delivered = lifecycle.deliveries.lock().unwrap().clone();
    assert_eq!(delivered.last().unwrap().0, o2);
    assert_eq!(delivered.last().unwrap().1.payload, "CODE-LATE");

    // ---- outcome unknown: held for review, no rollback, admin alerted
    *acme.placing.lock().unwrap() = Placing::Unknown;
    let alerts_before = jobs_of(&app.db, kinds::NOTIFICATION_DISPATCH).await.len();
    let (o3, p3) = procure(&app, "S-3", product_id, sku).await;
    let r = row(&app, p3).await;
    assert_eq!(r.status, "manual_review");
    assert_eq!(
        r.error_message,
        "[result unknown] answer lost, check manually"
    );
    assert!(!lifecycle.calls().contains(&format!("rollback:{o3}")));
    let alerts = jobs_of(&app.db, kinds::NOTIFICATION_DISPATCH).await;
    assert_eq!(alerts.len(), alerts_before + 1);
    assert_eq!(alerts.last().unwrap()["data"]["local_order_no"], "S-3");
    // workers leave it alone
    let placed = acme.placed.lock().unwrap().len();
    procurement.submit(p3).await.unwrap();
    procurement.poll(p3).await.unwrap();
    procurement.sync_accepted().await.unwrap();
    assert_eq!(acme.placed.lock().unwrap().len(), placed, "never re-bought");
    assert_eq!(row(&app, p3).await.status, "manual_review");
    // surfaced in the admin list and its stats
    let list = app
        .admin(
            "GET",
            "/api/v1/admin/procurement-orders?status=manual_review",
            None,
        )
        .await;
    assert_eq!(list["pagination"]["total"], 1, "{list}");
    assert_eq!(data(&list)[0]["status"], "manual_review");
    assert_eq!(data(&list)[0]["local_order_no"], "S-3");
    // embedded connections never carry adapter configuration (secret fields)
    assert!(!list.to_string().contains("t0p"), "{list}");
    let stats = app
        .admin("GET", "/api/v1/admin/procurement-orders/stats", None)
        .await;
    assert_eq!(data(&stats)["by_status"]["manual_review"], 1, "{stats}");
    // admin retry: submitted again with the same idempotency key / order number
    *acme.placing.lock().unwrap() = Placing::Delivered("CODE-RETRY".into());
    let retried = app
        .admin(
            "POST",
            &format!("/api/v1/admin/procurement-orders/{p3}/retry"),
            None,
        )
        .await;
    data(&retried);
    procurement.submit(p3).await.unwrap();
    let r = row(&app, p3).await;
    assert_eq!(r.status, "fulfilled", "{}", r.error_message);
    let placed_now = acme.placed.lock().unwrap().clone();
    let first = &placed_now[placed - 1];
    let again = placed_now.last().unwrap();
    assert_eq!(again.idempotency_key, first.idempotency_key);
    assert_eq!(again.downstream_order_no, "S-3");
    assert!(!lifecycle.calls().contains(&format!("rollback:{o3}")));

    // ---- unknown again, then the admin marks it failed: rolled back only now
    *acme.placing.lock().unwrap() = Placing::Unknown;
    let (o4, p4) = procure(&app, "S-4", product_id, sku).await;
    assert_eq!(row(&app, p4).await.status, "manual_review");
    assert!(!lifecycle.calls().contains(&format!("rollback:{o4}")));
    let canceled = app
        .admin(
            "POST",
            &format!("/api/v1/admin/procurement-orders/{p4}/cancel"),
            None,
        )
        .await;
    data(&canceled);
    assert_eq!(row(&app, p4).await.status, "canceled");
    assert!(lifecycle.calls().contains(&format!("rollback:{o4}")));
    // a second cancel is refused (and never rolls back twice)
    let again = app
        .admin(
            "POST",
            &format!("/api/v1/admin/procurement-orders/{p4}/cancel"),
            None,
        )
        .await;
    assert_ne!(again["status_code"], 0);
    let rollbacks = lifecycle
        .calls()
        .iter()
        .filter(|c| **c == format!("rollback:{o4}"))
        .count();
    assert_eq!(rollbacks, 1);

    // ---- executed but needs a human: order number kept, review reason stored,
    // admin retry re-checks the supplier order instead of buying again
    *acme.placing.lock().unwrap() = Placing::Review;
    let (o5, p5) = procure(&app, "S-5", product_id, sku).await;
    let r = row(&app, p5).await;
    assert_eq!(r.status, "manual_review");
    assert_eq!(r.error_message, "charged without goods");
    assert!(!r.upstream_order_no.is_empty());
    assert!(!lifecycle.calls().contains(&format!("rollback:{o5}")));
    let placed = acme.placed.lock().unwrap().len();
    let retried = app
        .admin(
            "POST",
            &format!("/api/v1/admin/procurement-orders/{p5}/retry"),
            None,
        )
        .await;
    data(&retried);
    assert_eq!(row(&app, p5).await.status, "accepted");
    procurement.poll(p5).await.unwrap();
    assert_eq!(
        acme.placed.lock().unwrap().len(),
        placed,
        "re-checked, not re-bought"
    );
    // a later supplier delivery (callback / poll) still completes it
    procurement
        .handle_event(
            p5,
            &zs_domain::integration::procurement::UpstreamEvent::Delivered,
            Some(&RemoteFulfillment {
                kind: "auto".into(),
                status: "delivered".into(),
                payload: "CODE-MANUAL".into(),
                ..RemoteFulfillment::default()
            }),
        )
        .await
        .unwrap();
    assert_eq!(row(&app, p5).await.status, "fulfilled");
}
