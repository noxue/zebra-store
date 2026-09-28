//! Live QA 2026-09-26 regressions of the reseller sites (对接/分站, LQA-R*): one pricing
//! source for catalogue and checkout, hidden products, refund claw-back.

#![expect(clippy::unwrap_used, reason = "tests")]

mod reseller_common;

use axum::http::StatusCode;
use reseller_common::{App, MAIN, data};
use serde_json::{Value, json};

/// Price of `slug` in a public product list.
fn listed_price(list: &Value, slug: &str) -> Option<String> {
    data(list)
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["slug"] == slug)
        .map(|p| p["price_amount"].as_str().unwrap().to_owned())
}

// LQA-R1: on a reseller host the list, search, detail and checkout all show / charge the
// reseller price; a product the reseller unlisted is absent from the list, search and
// sitemap, 404 on its page and refused at checkout. The main host keeps base prices.
#[tokio::test]
async fn lqa_r1_reseller_catalog_matches_checkout() {
    let app = App::new().await;
    let (_, rtoken, pid) = app.reseller("sakura@example.test", "50").await;
    let host = app.system_domain(pid, "sakura").await;
    let (shown, shown_skus) = app.product("qa-int-a", "5.20", &[("5.20", "1.00")]).await;
    let (hidden, hidden_skus) = app.product("qa-int-b", "3.00", &[("3.00", "1.00")]).await;
    // 10 % markup on qa-int-a, qa-int-b unlisted.
    data(
        &app.call(
            "PUT",
            &format!("/api/v1/reseller/product-settings/{shown}"),
            Some(json!({"settings": [{"sku_id": 0, "is_listed": true, "pricing_mode": "markup_percent", "markup_percent": "10"}]})),
            Some(&rtoken),
        )
        .await,
    );
    data(
        &app.call(
            "PUT",
            &format!("/api/v1/reseller/product-settings/{hidden}"),
            Some(json!({"settings": [{"sku_id": 0, "is_listed": false}]})),
            Some(&rtoken),
        )
        .await,
    );
    let on_site = [("host", host.as_str())];

    let (_, list) = app
        .raw("GET", "/api/v1/public/products", None, None, &on_site)
        .await;
    assert_eq!(listed_price(&list, "qa-int-a").as_deref(), Some("5.72"));
    assert_eq!(
        listed_price(&list, "qa-int-b"),
        None,
        "hidden product listed"
    );
    assert_eq!(list["pagination"]["total"], 1, "{list}");
    let (_, search) = app
        .raw(
            "GET",
            "/api/v1/public/products?search=qa-int",
            None,
            None,
            &on_site,
        )
        .await;
    assert_eq!(listed_price(&search, "qa-int-b"), None);
    assert_eq!(listed_price(&search, "qa-int-a").as_deref(), Some("5.72"));

    let (_, detail) = app
        .raw(
            "GET",
            "/api/v1/public/products/qa-int-a",
            None,
            None,
            &on_site,
        )
        .await;
    let detail = data(&detail);
    assert_eq!(detail["price_amount"], "5.72");
    assert_eq!(detail["skus"][0]["price_amount"], "5.72");
    let (_, gone) = app
        .raw(
            "GET",
            "/api/v1/public/products/qa-int-b",
            None,
            None,
            &on_site,
        )
        .await;
    assert_eq!(gone["status_code"], 404, "{gone}");

    // Checkout charges exactly the displayed price.
    let (_, buyer) = app.user("buyer@example.test").await;
    let (_, created) = app
        .raw(
            "POST",
            "/api/v1/orders",
            Some(json!({"items": [{"product_id": shown, "sku_id": shown_skus[0], "quantity": 1}]})),
            Some(&buyer),
            &on_site,
        )
        .await;
    assert_eq!(data(&created)["total_amount"], "5.72");
    let (_, refused) = app
        .raw(
            "POST",
            "/api/v1/orders",
            Some(
                json!({"items": [{"product_id": hidden, "sku_id": hidden_skus[0], "quantity": 1}]}),
            ),
            Some(&buyer),
            &on_site,
        )
        .await;
    assert_ne!(refused["status_code"], 0, "hidden product sold: {refused}");

    let (status, xml) = app.text("/sitemap.xml", &on_site).await;
    assert_eq!(status, StatusCode::OK);
    assert!(xml.contains(&format!("{host}/products/qa-int-a")), "{xml}");
    assert!(!xml.contains("/products/qa-int-b"), "{xml}");

    // The main shop is unaffected.
    let main = [("host", MAIN)];
    let (_, list) = app
        .raw("GET", "/api/v1/public/products", None, None, &main)
        .await;
    assert_eq!(listed_price(&list, "qa-int-a").as_deref(), Some("5.20"));
    assert_eq!(listed_price(&list, "qa-int-b").as_deref(), Some("3.00"));
    let (_, xml) = app.text("/sitemap.xml", &main).await;
    assert!(xml.contains("/products/qa-int-b"));
}

/// Paid reseller-site order of `qty` × the single SKU of `slug`: `(parent_id, child_id)`.
async fn site_sale(app: &App, host: &str, buyer: &str, product: i64, sku: i64) -> (i64, i64) {
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
    use zs_infra::db::entity::orders;
    let on_site = [("host", host)];
    let (_, created) = app
        .raw(
            "POST",
            "/api/v1/orders",
            Some(json!({"items": [{"product_id": product, "sku_id": sku, "quantity": 1}]})),
            Some(buyer),
            &on_site,
        )
        .await;
    let no = data(&created)["order_no"].as_str().unwrap().to_owned();
    let (_, paid) = app
        .raw(
            "POST",
            "/api/v1/payments",
            Some(json!({"order_no": no, "channel_id": 0, "use_balance": true})),
            Some(buyer),
            &on_site,
        )
        .await;
    assert_eq!(data(&paid)["order_paid"], true, "{paid}");
    let parent = orders::Entity::find()
        .filter(orders::Column::OrderNo.eq(no))
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    let child = orders::Entity::find()
        .filter(orders::Column::ParentId.eq(parent.id))
        .one(&app.db)
        .await
        .unwrap()
        .map_or(parent.id, |c| c.id);
    (parent.id, child)
}

async fn deductions(app: &App, order_id: i64) -> Vec<String> {
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
    use zs_infra::db::entity::reseller_ledger_entries as ledger;
    ledger::Entity::find()
        .filter(ledger::Column::OrderId.eq(order_id))
        .filter(ledger::Column::Type.eq("refund_deduct"))
        .order_by_asc(ledger::Column::Id)
        .all(&app.db)
        .await
        .unwrap()
        .into_iter()
        .map(|e| zs_shared::money::Amount::new(e.amount).to_string())
        .collect()
}

async fn balance(app: &App, pid: i64) -> (String, String, String) {
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
    use zs_infra::db::entity::reseller_balance_accounts as balances;
    let b = balances::Entity::find()
        .filter(balances::Column::ResellerId.eq(pid))
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    let a = zs_shared::money::Amount::new;
    (
        a(b.available_amount_cache).to_string(),
        a(b.negative_amount_cache).to_string(),
        b.status,
    )
}

// LQA-R4: refunding the child order of a reseller sale claws back the profit posted on
// the parent: 5.72 sale (profit 0.52), refund 2.86 → refund_deduct -0.26 and the parent
// shows 2.86 refunded; 0.30 was already withdrawn and paid, so refunding the rest
// (-0.26) leaves the reseller 0.30 negative. Parent + child refunds never exceed 5.72.
#[tokio::test]
async fn lqa_r4_child_refund_claws_back_profit() {
    let mut cfg = reseller_common::config();
    cfg.reseller.settlement_confirm_days = 0;
    let app = App::with(cfg).await;
    let (_, rtoken, pid) = app.reseller("sakura@example.test", "50").await;
    let host = app.system_domain(pid, "sakura").await;
    let (product, skus) = app.product("qa-int-a", "5.20", &[("5.20", "1.00")]).await;
    data(
        &app.call(
            "PUT",
            &format!("/api/v1/reseller/product-settings/{product}"),
            Some(json!({"settings": [{"sku_id": 0, "is_listed": true, "pricing_mode": "markup_percent", "markup_percent": "10"}]})),
            Some(&rtoken),
        )
        .await,
    );
    let (buyer_id, buyer) = app.user("buyer@example.test").await;
    data(
        &app.admin_call(
            "POST",
            &format!("/api/v1/admin/users/{buyer_id}/wallet/adjust"),
            Some(json!({"amount": "100.00", "operation": "add", "remark": "qa"})),
        )
        .await,
    );
    let (parent, child) = site_sale(&app, &host, &buyer, product, skus[0]).await;
    app.services.reseller.ledger.confirm_due().await.unwrap();
    assert_eq!(balance(&app, pid).await.0, "0.52");

    // Withdraw 0.30 and have it paid out.
    let w = app
        .call(
            "POST",
            "/api/v1/reseller/withdraws",
            Some(json!({"amount": "0.30", "currency": "CNY", "channel": "alipay", "account": "a@b.test"})),
            Some(&rtoken),
        )
        .await;
    let wid = data(&w)["id"].as_i64().unwrap();
    data(
        &app.admin_call(
            "POST",
            &format!("/api/v1/admin/resellers/withdraws/{wid}/pay"),
            None,
        )
        .await,
    );
    assert_eq!(balance(&app, pid).await.0, "0.22");

    let refund = |id: i64, amount: &str| {
        let body = json!({"amount": amount, "remark": "qa"});
        let uri = format!("/api/v1/admin/orders/{id}/refund-to-wallet");
        let app = &app;
        async move { app.admin_call("POST", &uri, Some(body)).await }
    };
    data(&refund(child, "2.86").await);
    assert_eq!(deductions(&app, parent).await, ["-0.26"]);
    let parent_row = app
        .admin_call("GET", &format!("/api/v1/admin/orders/{parent}"), None)
        .await;
    assert_eq!(data(&parent_row)["refunded_amount"], "2.86");
    assert_eq!(balance(&app, pid).await.0, "-0.04");

    // The parent cannot refund more than what is left (2.86).
    let over = refund(parent, "3.00").await;
    assert_ne!(over["status_code"], 0, "{over}");
    data(&refund(parent, "2.86").await);
    assert_eq!(deductions(&app, parent).await, ["-0.26", "-0.26"]);
    let (available, negative, status) = balance(&app, pid).await;
    assert_eq!(available, "-0.30");
    assert_eq!(
        (negative.as_str(), status.as_str()),
        ("0.30", "negative_balance")
    );
    // Nothing left to refund on the child either.
    let over = refund(child, "0.01").await;
    assert_ne!(over["status_code"], 0, "{over}");

    // Finance overview shows the deductions.
    let fin = app
        .admin_call(
            "GET",
            "/api/v1/admin/resellers/operations/finance?range=today",
            None,
        )
        .await;
    let text = data(&fin).to_string();
    assert!(text.contains("\"refund_deducted\":\"0.52\""), "{text}");
}
