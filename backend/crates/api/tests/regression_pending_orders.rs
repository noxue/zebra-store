//! FE-17: status totals must describe all matching orders, not the current page.
mod order_common;
use order_common::{Auth, OrderApp};
use sea_orm::{ActiveModelTrait, Set};
use serde_json::json;
use zs_infra::db::entity::user_oauth_identities;

#[tokio::test]
async fn fe_17_status_counts_ignore_pagination_and_other_users() {
    let app = OrderApp::new().await;
    let (pid, sku) = app
        .product(
            "stats",
            json!({"fulfillment_type": "manual", "manual_stock_total": -1}),
        )
        .await;
    let (uid, token) = app.user("stats@example.com", 0).await;
    let auth = Auth::Bearer(token);
    for _ in 0..3 {
        let created = app
            .call(
                "POST",
                "/api/v1/orders",
                Some(json!({"items": [{"product_id": pid, "sku_id": sku, "quantity": 1}]})),
                &auth,
            )
            .await;
        assert_eq!(created["status_code"], 0, "{created}");
    }
    let now = chrono::Utc::now();
    user_oauth_identities::ActiveModel {
        user_id: Set(uid),
        provider: Set("telegram".into()),
        provider_user_id: Set("12345".into()),
        username: Set("TelegramStats".into()),
        avatar_url: Set(String::new()),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&app.db)
    .await
    .expect("insert OAuth test identity");
    // DB-10: case-insensitive third-party usernames work for both users and orders.
    let found = app
        .admin_call(
            "GET",
            "/api/v1/admin/orders?user_keyword=telegramstats",
            None,
        )
        .await;
    assert_eq!(found["pagination"]["total"], 3, "{found}");
    let users = app
        .admin_call("GET", "/api/v1/admin/users?keyword=telegramstats", None)
        .await;
    assert_eq!(users["pagination"]["total"], 1, "{users}");
    let list = app
        .call("GET", "/api/v1/orders?page=2&page_size=1", None, &auth)
        .await;
    assert_eq!(list["data"].as_array().map(Vec::len), Some(1));
    let stats = app
        .call(
            "GET",
            "/api/v1/orders/stats?page=2&page_size=1",
            None,
            &auth,
        )
        .await;
    assert_eq!(stats["data"]["total"], 3, "{stats}");
    assert_eq!(stats["data"]["by_status"]["pending_payment"], 3);
    let (_, other) = app.user("other-stats@example.com", 0).await;
    let empty = app
        .call("GET", "/api/v1/orders/stats", None, &Auth::Bearer(other))
        .await;
    assert_eq!(empty["data"]["total"], 0);
}
