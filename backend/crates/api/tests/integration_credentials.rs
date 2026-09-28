//! API credentials: user self-service and admin review (UPS-10, UPS-18).

mod integration_common;

use integration_common::{IntApp, data, err};
use serde_json::json;

#[tokio::test]
async fn user_routes_require_login() {
    let app = IntApp::new().await;
    let res = app.call("GET", "/api/v1/api-credential", None, None).await;
    assert_eq!(res["status_code"], 401, "{res}");
}

#[tokio::test]
async fn apply_review_and_self_service() {
    let app = IntApp::new().await;
    let (uid, token) = app.user("buyer@example.com").await;

    let none = app
        .call("GET", "/api/v1/api-credential", None, Some(&token))
        .await;
    // Original `{"status":"none"}` plus the zebra-store rotation / protocol fields.
    assert_eq!(
        data(&none),
        &json!({
            "status": "none",
            "rotation_pending": false,
            "rotation_expires_at": null,
            "protocols": ["dujiao-next", "zebra-store"],
        })
    );

    let applied = app
        .call("POST", "/api/v1/api-credential/apply", None, Some(&token))
        .await;
    let cid = data(&applied)["id"].as_i64().unwrap();
    assert_eq!(data(&applied)["status"], "pending_review");

    // UPS-18: a pending application cannot be repeated.
    let again = app
        .call("POST", "/api/v1/api-credential/apply", None, Some(&token))
        .await;
    err(&again, 400, "Application is pending review");

    // Pending: no key shown, secret cannot be regenerated.
    let mine = app
        .call("GET", "/api/v1/api-credential", None, Some(&token))
        .await;
    assert_eq!(data(&mine)["status"], "pending_review");
    assert!(data(&mine).get("api_key").is_none());
    let regen = app
        .call(
            "POST",
            "/api/v1/api-credential/regenerate",
            None,
            Some(&token),
        )
        .await;
    err(&regen, 400, "API credential is not approved");

    // Admin list shows the owner; never the secret.
    let list = app
        .admin(
            "GET",
            "/api/v1/admin/api-credentials?status=pending_review",
            None,
        )
        .await;
    let items = data(&list).as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["user"]["email"], "buyer@example.com");
    assert_eq!(items[0]["user_id"], uid);
    assert!(items[0].get("api_secret").is_none());
    assert_eq!(list["pagination"]["total"], 1);
    let search = app
        .admin("GET", "/api/v1/admin/api-credentials?search=BUYER@", None)
        .await;
    assert_eq!(data(&search).as_array().unwrap().len(), 1);
    let miss = app
        .admin("GET", "/api/v1/admin/api-credentials?search=nobody", None)
        .await;
    assert_eq!(data(&miss).as_array().unwrap().len(), 0);

    // UPS-10: approval returns neither key nor secret material beyond the credential.
    let approved = app
        .admin(
            "POST",
            &format!("/api/v1/admin/api-credentials/{cid}/approve"),
            None,
        )
        .await;
    assert_eq!(data(&approved)["approved"], true);
    assert!(data(&approved)["credential"].get("api_secret").is_none());
    assert_eq!(data(&approved)["credential"]["status"], "approved");

    let mine = app
        .call("GET", "/api/v1/api-credential", None, Some(&token))
        .await;
    let d = data(&mine);
    assert_eq!(d["status"], "approved");
    assert_eq!(d["api_key"].as_str().unwrap().len(), 64);
    let regen = app
        .call(
            "POST",
            "/api/v1/api-credential/regenerate",
            None,
            Some(&token),
        )
        .await;
    let secret = data(&regen)["api_secret"].as_str().unwrap().to_owned();
    let mine = app
        .call("GET", "/api/v1/api-credential", None, Some(&token))
        .await;
    assert_eq!(data(&mine)["api_secret_tail"], secret[secret.len() - 4..]);

    // Self disable / admin enable.
    let off = app
        .call(
            "PUT",
            "/api/v1/api-credential/status",
            Some(json!({"is_active": false})),
            Some(&token),
        )
        .await;
    assert_eq!(data(&off)["updated"], true);
    let on = app
        .admin(
            "PUT",
            &format!("/api/v1/admin/api-credentials/{cid}/status"),
            Some(json!({"is_active": true})),
        )
        .await;
    assert_eq!(data(&on)["updated"], true);

    // Approved → applying again is refused.
    let again = app
        .call("POST", "/api/v1/api-credential/apply", None, Some(&token))
        .await;
    err(&again, 400, "API credential already exists");
}

// UPS-18 (2): reject → reapply resets; delete → the same row is restored pending.
#[tokio::test]
async fn ups18_reject_delete_and_reapply() {
    let app = IntApp::new().await;
    let (_, token) = app.user("r@example.com").await;
    let (_, token2) = app.user("s@example.com").await;
    let a = app
        .call("POST", "/api/v1/api-credential/apply", None, Some(&token))
        .await;
    let b = app
        .call("POST", "/api/v1/api-credential/apply", None, Some(&token2))
        .await;
    // UPS-18 (1): two pending rows coexist (unique random keys).
    let cid = data(&a)["id"].as_i64().unwrap();
    assert_ne!(cid, data(&b)["id"].as_i64().unwrap());

    let missing = app
        .admin(
            "POST",
            &format!("/api/v1/admin/api-credentials/{cid}/reject"),
            Some(json!({})),
        )
        .await;
    err(&missing, 400, "Reason: 不能为空");
    let rejected = app
        .admin(
            "POST",
            &format!("/api/v1/admin/api-credentials/{cid}/reject"),
            Some(json!({"reason": "no"})),
        )
        .await;
    assert_eq!(data(&rejected)["rejected"], true);
    let mine = app
        .call("GET", "/api/v1/api-credential", None, Some(&token))
        .await;
    assert_eq!(data(&mine)["reject_reason"], "no");

    // Status toggle of a non-approved credential.
    let toggle = app
        .admin(
            "PUT",
            &format!("/api/v1/admin/api-credentials/{cid}/status"),
            Some(json!({"is_active": true})),
        )
        .await;
    err(&toggle, 400, "API 凭证尚未审核通过");

    let re = app
        .call("POST", "/api/v1/api-credential/apply", None, Some(&token))
        .await;
    assert_eq!(data(&re)["id"], cid);
    assert_eq!(data(&re)["status"], "pending_review");

    let deleted = app
        .admin(
            "DELETE",
            &format!("/api/v1/admin/api-credentials/{cid}"),
            None,
        )
        .await;
    assert_eq!(data(&deleted)["deleted"], true);
    let gone = app
        .admin("GET", &format!("/api/v1/admin/api-credentials/{cid}"), None)
        .await;
    err(&gone, 404, "API 凭证不存在");
    let mine = app
        .call("GET", "/api/v1/api-credential", None, Some(&token))
        .await;
    assert_eq!(data(&mine)["status"], "none");

    let restored = app
        .call("POST", "/api/v1/api-credential/apply", None, Some(&token))
        .await;
    assert_eq!(data(&restored)["id"], cid);
    assert_eq!(data(&restored)["status"], "pending_review");
}

#[tokio::test]
async fn admin_errors() {
    let app = IntApp::new().await;
    let res = app
        .admin("POST", "/api/v1/admin/api-credentials/999/approve", None)
        .await;
    err(&res, 404, "API 凭证不存在");
    let res = app
        .call("GET", "/api/v1/admin/api-credentials", None, None)
        .await;
    assert_eq!(res["status_code"], 401);
}
