//! Site connections: CRUD with encrypted secret, ping against a mock supplier,
//! status, pricing validation and re-pricing (UPS-01, UPS-05).

mod integration_common;

use integration_common::{IntApp, SUP_SECRET, data, err, start_supplier};
use serde_json::json;
use zs_infra::integration::http::AddressPolicy;

#[tokio::test]
async fn crud_keeps_secret_encrypted() {
    let app = IntApp::new().await;
    let id = app
        .connection(
            "https://supplier.example.com/",
            json!({"exchange_rate": "7.2", "price_markup_percent": 20, "price_rounding_mode": "ceil_int", "retry_intervals": "[10, 20]"}),
        )
        .await;
    let got = app
        .admin("GET", &format!("/api/v1/admin/site-connections/{id}"), None)
        .await;
    let d = data(&got);
    assert_eq!(d["base_url"], "https://supplier.example.com");
    assert_eq!(d["status"], "pending");
    assert_eq!(d["protocol"], "dujiao-next");
    assert_eq!(d["exchange_rate"], "7.2");
    assert_eq!(d["price_markup_percent"], "20");
    assert_eq!(d["price_rounding_mode"], "ceil_int");
    assert_eq!(d["retry_max"], 5);
    assert_eq!(d["retry_intervals"], "[10,20]");
    assert!(d.get("api_secret").is_none());
    assert!(!got.to_string().contains(SUP_SECRET));

    use sea_orm::EntityTrait;
    let row = zs_infra::db::entity::site_connections::Entity::find_by_id(id)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    assert_ne!(row.api_secret, SUP_SECRET);
    assert!(row.api_secret.len() <= 512);

    let list = app
        .admin("GET", "/api/v1/admin/site-connections", None)
        .await;
    assert_eq!(data(&list).as_array().unwrap().len(), 1);
    assert_eq!(list["pagination"]["total"], 1);

    // Blank fields keep their value; an empty secret keeps the old one.
    let upd = app
        .admin(
            "PUT",
            &format!("/api/v1/admin/site-connections/{id}"),
            Some(json!({"name": "Renamed", "api_secret": "", "auto_sync_price": true})),
        )
        .await;
    assert_eq!(data(&upd)["name"], "Renamed");
    assert_eq!(data(&upd)["auto_sync_price"], true);
    assert_eq!(data(&upd)["exchange_rate"], "7.2");
    let row2 = zs_infra::db::entity::site_connections::Entity::find_by_id(id)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row2.api_secret, row.api_secret);

    let st = app
        .admin(
            "PUT",
            &format!("/api/v1/admin/site-connections/{id}/status"),
            Some(json!({"status": "disabled"})),
        )
        .await;
    assert_eq!(data(&st)["updated"], true);
    let st = app
        .admin(
            "PUT",
            &format!("/api/v1/admin/site-connections/{id}/status"),
            Some(json!({"status": "bogus"})),
        )
        .await;
    err(&st, 400, "请求参数错误");

    let del = app
        .admin(
            "DELETE",
            &format!("/api/v1/admin/site-connections/{id}"),
            None,
        )
        .await;
    assert_eq!(data(&del)["deleted"], true);
    let gone = app
        .admin("GET", &format!("/api/v1/admin/site-connections/{id}"), None)
        .await;
    err(&gone, 404, "站点连接不存在");
}

// UPS-05 (1): markup ≤ -100 %, non-positive rate, unknown rounding and bad URLs are refused.
#[tokio::test]
async fn ups05_invalid_pricing_is_refused() {
    let app = IntApp::new().await;
    for extra in [
        json!({"price_markup_percent": -150}),
        json!({"exchange_rate": 0}),
        json!({"exchange_rate": "-1"}),
        json!({"price_rounding_mode": "floor"}),
        json!({"base_url": "ftp://x"}),
        json!({"api_secret": ""}),
    ] {
        let mut body = json!({"name": "S", "base_url": "https://s.example.com", "api_key": "k", "api_secret": "s"});
        for (k, v) in extra.as_object().unwrap() {
            body[k] = v.clone();
        }
        let res = app
            .admin("POST", "/api/v1/admin/site-connections", Some(body))
            .await;
        err(&res, 400, "站点连接参数无效");
    }
    let res = app
        .admin("POST", "/api/v1/admin/site-connections", Some(json!({"name": "S", "base_url": "https://s.example.com", "api_key": "k", "api_secret": "s", "exchange_rate": "abc"})))
        .await;
    err(&res, 400, "请求参数错误");
}

#[tokio::test]
async fn ping_activates_pending_connection() {
    let (base, supplier) = start_supplier().await;
    let app = IntApp::new().await;
    let id = app.connection(&base, json!({})).await;
    let res = app
        .admin(
            "POST",
            &format!("/api/v1/admin/site-connections/{id}/ping"),
            None,
        )
        .await;
    let d = data(&res);
    assert_eq!(d["site_name"], "Supplier");
    assert_eq!(d["balance"], "99.00");
    assert_eq!(d["currency"], "USD");
    let got = app
        .admin("GET", &format!("/api/v1/admin/site-connections/{id}"), None)
        .await;
    assert_eq!(data(&got)["status"], "active");
    assert_eq!(data(&got)["last_ping_ok"], true);
    assert!(data(&got)["last_ping_at"].is_string());
    // The supplier verified our signature with the decrypted secret: one handshake
    // (ping) when the connection was created, one for the explicit ping.
    assert_eq!(
        supplier.paths(),
        vec!["/api/v1/upstream/ping", "/api/v1/upstream/ping"]
    );
    // Negotiated adapter state for the legacy protocol: categories only, full sync, no push.
    let d = data(&got);
    assert_eq!(d["features"], json!(["categories"]));
    assert_eq!(d["capabilities"], json!(["categories"]));
    assert_eq!(d["supplier_currency"], "USD");
    assert_eq!(d["sync_mode"], "full");
    assert_eq!(d["webhook_status"], "unsupported");
    assert_eq!(d["handshake_error"], "");

    // A wrong secret: ping fails with the supplier's error, stored as not ok.
    let upd = app
        .admin(
            "PUT",
            &format!("/api/v1/admin/site-connections/{id}"),
            Some(json!({"api_secret": "wrong"})),
        )
        .await;
    data(&upd);
    let res = app
        .admin(
            "POST",
            &format!("/api/v1/admin/site-connections/{id}/ping"),
            None,
        )
        .await;
    assert_eq!(res["status_code"], 500, "{res}");
    assert!(res["msg"].as_str().unwrap().contains("401"), "{res}");
    let got = app
        .admin("GET", &format!("/api/v1/admin/site-connections/{id}"), None)
        .await;
    assert_eq!(data(&got)["last_ping_ok"], false);
}

// UPS-01: production policy refuses loopback suppliers before any request.
#[tokio::test]
async fn ups01_loopback_supplier_is_refused_in_production() {
    let (base, supplier) = start_supplier().await;
    let app = IntApp::with_policy(AddressPolicy::PublicOnly).await;
    let id = app.connection(&base, json!({})).await;
    let res = app
        .admin(
            "POST",
            &format!("/api/v1/admin/site-connections/{id}/ping"),
            None,
        )
        .await;
    assert_eq!(res["status_code"], 500);
    assert!(
        res["msg"].as_str().unwrap().contains("forbidden address"),
        "{res}"
    );
    assert!(supplier.hits().is_empty());
}

#[tokio::test]
async fn reapply_markup_of_unknown_connection() {
    let app = IntApp::new().await;
    let res = app
        .admin(
            "POST",
            "/api/v1/admin/site-connections/77/reapply-markup",
            None,
        )
        .await;
    err(&res, 404, "站点连接不存在");
    let id = app.connection("https://s.example.com", json!({})).await;
    let res = app
        .admin(
            "POST",
            &format!("/api/v1/admin/site-connections/{id}/reapply-markup"),
            None,
        )
        .await;
    assert_eq!(data(&res)["updated_products"], 0);
}
