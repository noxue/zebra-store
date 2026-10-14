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

// RSL-07/RSL-09: admin base overrides are isolated by reseller, and console edits cannot erase them.
#[tokio::test]
async fn reseller_agent_base_isolated_and_used_for_checkout_profit() {
    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter, Set};
    use zs_infra::db::entity::{orders, product_skus, reseller_order_snapshots};

    let app = App::new().await;
    let (_, owner_token, site_a) = app.reseller("base-a@example.test", "50").await;
    let (_, _, site_b) = app.reseller("base-b@example.test", "50").await;
    let host_a = app.system_domain(site_a, "base-a").await;
    let host_b = app.system_domain(site_b, "base-b").await;
    let (product, skus) = app
        .product(
            "site-agent-base",
            "100.00",
            &[("100.00", "70.00"), ("100.00", "75.00")],
        )
        .await;
    for sku_id in &skus {
        let mut row = product_skus::Entity::find_by_id(*sku_id)
            .one(&app.db)
            .await
            .unwrap()
            .unwrap()
            .into_active_model();
        row.agent_price_amount = Set(Some("80.00".parse().unwrap()));
        row.update(&app.db).await.unwrap();
    }
    let admin_url = format!("/api/v1/admin/resellers/product-settings/{site_a}/{product}");
    let save = app.admin_call("PUT", &admin_url, Some(json!({"settings": [
        {"sku_id": 0, "is_listed": true, "pricing_mode": "fixed_price", "fixed_price_amount": "90.00", "agent_price_amount": "75.00"},
        {"sku_id": skus[1], "is_listed": true, "pricing_mode": "markup_percent", "markup_percent": "0", "agent_price_amount": "85.00"}
    ]}))).await;
    assert_eq!(save["status_code"], 0, "{save}");
    assert_eq!(data(&save)["skus"][0]["base_price_amount"], "75.00");
    assert_eq!(data(&save)["skus"][1]["base_price_amount"], "85.00");

    let (_, a) = app
        .raw(
            "GET",
            "/api/v1/public/products/site-agent-base",
            None,
            None,
            &[("host", host_a.as_str())],
        )
        .await;
    let (_, b) = app
        .raw(
            "GET",
            "/api/v1/public/products/site-agent-base",
            None,
            None,
            &[("host", host_b.as_str())],
        )
        .await;
    let (_, main) = app
        .raw(
            "GET",
            "/api/v1/public/products/site-agent-base",
            None,
            None,
            &[("host", MAIN)],
        )
        .await;
    assert_eq!(data(&a)["skus"][0]["price_amount"], "90.00");
    assert_eq!(data(&a)["skus"][1]["price_amount"], "85.00");
    assert_eq!(data(&b)["skus"][0]["price_amount"], "80.00");
    assert_eq!(data(&main)["skus"][0]["price_amount"], "100.00");

    let (_, buyer) = app.user("base-buyer@example.test").await;
    let (_, order) = app
        .raw(
            "POST",
            "/api/v1/orders",
            Some(json!({"items": [{"product_id": product, "sku_id": skus[0], "quantity": 1}]})),
            Some(&buyer),
            &[("host", host_a.as_str())],
        )
        .await;
    assert_eq!(data(&order)["total_amount"], "90.00", "{order}");
    let order_no = data(&order)["order_no"].as_str().unwrap();
    let order_id = orders::Entity::find()
        .filter(orders::Column::OrderNo.eq(order_no))
        .one(&app.db)
        .await
        .unwrap()
        .unwrap()
        .id;
    let snapshot = reseller_order_snapshots::Entity::find()
        .filter(reseller_order_snapshots::Column::OrderId.eq(order_id))
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        zs_shared::money::Amount::new(snapshot.base_amount).to_string(),
        "75.00"
    );
    assert_eq!(
        zs_shared::money::Amount::new(snapshot.profit_amount).to_string(),
        "15.00"
    );

    // A later cost increase invalidates only site A's cheaper base for this SKU.
    let mut raised = product_skus::Entity::find_by_id(skus[0])
        .one(&app.db)
        .await
        .unwrap()
        .unwrap()
        .into_active_model();
    raised.cost_price_amount = Set("76.00".parse().unwrap());
    raised.update(&app.db).await.unwrap();
    let (_, a_after_cost) = app
        .raw(
            "GET",
            "/api/v1/public/products/site-agent-base",
            None,
            None,
            &[("host", host_a.as_str())],
        )
        .await;
    let (_, b_after_cost) = app
        .raw(
            "GET",
            "/api/v1/public/products/site-agent-base",
            None,
            None,
            &[("host", host_b.as_str())],
        )
        .await;
    assert_eq!(data(&a_after_cost)["skus"].as_array().unwrap().len(), 1);
    assert_eq!(data(&b_after_cost)["skus"].as_array().unwrap().len(), 2);
    let (_, denied) = app
        .raw(
            "POST",
            "/api/v1/orders",
            Some(json!({"items": [{"product_id": product, "sku_id": skus[0], "quantity": 1}]})),
            Some(&buyer),
            &[("host", host_a.as_str())],
        )
        .await;
    assert_ne!(denied["status_code"], 0);
    let mut restored = product_skus::Entity::find_by_id(skus[0])
        .one(&app.db)
        .await
        .unwrap()
        .unwrap()
        .into_active_model();
    restored.cost_price_amount = Set("70.00".parse().unwrap());
    restored.update(&app.db).await.unwrap();

    let console_url = format!("/api/v1/reseller/product-settings/{product}");
    let attempted = app.call("PUT", &console_url, Some(json!({"settings": [{"sku_id": 0, "is_listed": true, "agent_price_amount": "60.00"}]})), Some(&owner_token)).await;
    assert_ne!(attempted["status_code"], 0);
    let edited = app
        .call(
            "PUT",
            &console_url,
            Some(
                json!({"settings": [{"sku_id": 0, "is_listed": true, "pricing_mode": "inherit"}]}),
            ),
            Some(&owner_token),
        )
        .await;
    assert_eq!(edited["status_code"], 0, "{edited}");
    assert_eq!(data(&edited)["skus"][0]["base_price_amount"], "75.00");
    let reset = app
        .call("DELETE", &console_url, None, Some(&owner_token))
        .await;
    assert_eq!(reset["status_code"], 0, "{reset}");
    let after_reset = app.admin_call("GET", &admin_url, None).await;
    assert_eq!(
        data(&after_reset)["product_setting"]["agent_price_amount"],
        "75.00"
    );

    let below_cost = app.admin_call("PUT", &admin_url, Some(json!({"settings": [{"sku_id": 0, "is_listed": true, "agent_price_amount": "69.99"}]}))).await;
    assert_ne!(below_cost["status_code"], 0);
    let preview = app.admin_call("POST", &format!("{admin_url}/preview"), Some(json!({"settings": [{"sku_id": 0, "is_listed": true, "agent_price_amount": "69.99"}]}))).await;
    assert_eq!(data(&preview)["items"][0]["valid"], false);
    let clear = app.admin_call("PUT", &admin_url, Some(json!({"settings": [{"sku_id": 0, "is_listed": true, "pricing_mode": "inherit", "agent_price_amount": null}]}))).await;
    assert_eq!(clear["status_code"], 0, "{clear}");
    assert_eq!(data(&clear)["skus"][0]["base_price_amount"], "80.00");
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
