//! Remaining catalog acceptance cases from bugfix-lessons §21.
mod catalog_common;
mod common;
use catalog_common::{category, product};
use common::{TestApp, data};
use serde_json::json;
use zs_domain::settings::schema::telegram_bot::TelegramBotSetting;

#[tokio::test]
async fn set_05_public_product_preserves_localized_seo() {
    let app = TestApp::new().await;
    let cat = category(&app, "seo", 0).await;
    let seo = json!({"title": {"zh-CN": "标题", "en-US": "Title"}, "description": {"en-US": "Description"}, "keywords": "one,two"});
    product(&app, cat, "seo", json!({"seo_meta": seo})).await;
    let res = app
        .call("GET", "/api/v1/public/products/seo", None, None)
        .await;
    assert_eq!(data(&res)["seo_meta"], seo);
}

#[tokio::test]
async fn db_10_case_insensitive_catalog_search_and_invalid_payment_filters() {
    let app = TestApp::new().await;
    let cat = category(&app, "search", 0).await;
    product(&app, cat, "abc", json!({"title": {"en-US": "UPPERCASE"}})).await;
    for search in ["ABC", "uppercase", "UpperCase"] {
        let res = app
            .call(
                "GET",
                &format!("/api/v1/public/products?search={search}"),
                None,
                None,
            )
            .await;
        assert_eq!(data(&res).as_array().unwrap().len(), 1, "{res}");
    }
    data(
        &app.post(
            "/api/v1/admin/compliance/acknowledge",
            json!({
                "segment1": "我已阅读并理解上述合规声明提醒",
                "segment2": "知悉相关法律风险",
                "segment3": "并确认自行承担部署运营和收费行为产生的法律责任"
            }),
        )
        .await,
    );
    for query in ["order_id=bad", "channel_id=abc"] {
        let res = app.get(&format!("/api/v1/admin/payments?{query}")).await;
        assert_eq!(res["status_code"], 400, "{res}");
    }
}

#[test]
fn ntf_09_menu_backfill_preserves_disabled_orders() {
    let setting: TelegramBotSetting = serde_json::from_value(
        json!({"menu": {"items": [{"key": "my_orders", "enabled": false}]}}),
    )
    .unwrap();
    let setting = setting.normalized();
    assert!(
        !setting
            .menu
            .items
            .iter()
            .find(|i| i.key == "my_orders")
            .unwrap()
            .enabled
    );
    assert!(
        setting
            .menu
            .items
            .iter()
            .find(|i| i.key == "affiliate")
            .unwrap()
            .enabled
    );
}

#[tokio::test]
async fn misc_09_catalog_sorting_matches_current_upstream() {
    let app = TestApp::new().await;
    let low = category(&app, "low", 0).await;
    let first = category(&app, "first", 0).await;
    let second = category(&app, "second", 0).await;
    for id in [first, second] {
        data(&app.put(&format!("/api/v1/admin/categories/{id}"), json!({"name": {"zh-CN": format!("category{id}")}, "slug": format!("category{id}"), "sort_order": 100})).await);
    }
    let categories = app
        .call("GET", "/api/v1/public/categories", None, None)
        .await;
    let ids: Vec<_> = data(&categories)
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_i64().unwrap())
        .collect();
    assert_eq!(ids, vec![first, second, low]);
    product(&app, low, "low", json!({"sort_order": 1})).await;
    product(
        &app,
        low,
        "high",
        json!({"sort_order": 100, "skus": [
            {"sku_code": "LOW", "price_amount": 10, "sort_order": 1},
            {"sku_code": "FIRST", "price_amount": 20, "sort_order": 100},
            {"sku_code": "SECOND", "price_amount": 30, "sort_order": 100}
        ]}),
    )
    .await;
    let products = app.call("GET", "/api/v1/public/products", None, None).await;
    assert_eq!(data(&products)[0]["slug"], "high");
    assert_eq!(data(&products)[1]["slug"], "low");
    let detail = app
        .call("GET", "/api/v1/public/products/high", None, None)
        .await;
    let codes: Vec<_> = data(&detail)["skus"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["sku_code"].as_str().unwrap())
        .collect();
    assert_eq!(codes, vec!["FIRST", "SECOND", "LOW"]);
    // Current Go product_store.go uses created_at DESC for equal product sort_order;
    // only categories/SKUs require id ASC. Do not change products to match a stale audit.
}

#[tokio::test]
async fn fe_10_cached_config_has_fresh_server_time() {
    let app = TestApp::new().await;
    let first = app.call("GET", "/api/v1/public/config", None, None).await;
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    let second = app.call("GET", "/api/v1/public/config", None, None).await;
    assert!(
        data(&second)["server_time"].as_i64().unwrap()
            > data(&first)["server_time"].as_i64().unwrap()
    );
}
