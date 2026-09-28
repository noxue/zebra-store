//! Admin 2FA: setup/enable/disable/recovery codes, the login challenge,
//! resets and the admin login log.

#![expect(
    clippy::unwrap_used,
    reason = "integration test helpers abort on failure"
)]

mod identity_common;

use identity_common::{App, data, err, totp, wrong_totp};
use sea_orm::{EntityTrait, QueryOrder};
use serde_json::{Value, json};

const PW: &str = "Admin12345";

/// Enables 2FA for the logged-in admin; returns `(secret, recovery_codes)`.
async fn enable(app: &App, token: &str) -> (String, Vec<String>) {
    let setup = app
        .call("POST", "/api/v1/admin/2fa/setup", None, Some(token))
        .await;
    let d = data(&setup);
    let secret = d["secret"].as_str().unwrap().to_owned();
    assert!(
        d["otpauth_url"]
            .as_str()
            .unwrap()
            .starts_with("otpauth://totp/Zebra-Store:")
    );
    assert!(d["expires_at"].is_string());
    let res = app
        .call(
            "POST",
            "/api/v1/admin/2fa/enable",
            Some(json!({"code": totp(&secret)})),
            Some(token),
        )
        .await;
    let d = data(&res);
    assert!(d["enabled_at"].is_string());
    let codes: Vec<String> = d["recovery_codes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(codes.len(), 10);
    assert!(codes.iter().all(|c| c.len() == 11 && &c[4..5] == "-"));
    (secret, codes)
}

async fn login_challenge(app: &App) -> String {
    let res = app
        .call(
            "POST",
            "/api/v1/admin/login",
            Some(json!({"username": "admin", "password": PW})),
            None,
        )
        .await;
    let d = data(&res);
    assert_eq!(d["requires_totp"], true);
    assert!(d.get("token").is_none());
    assert!(d["challenge_expires_at"].as_str().unwrap().ends_with('Z'));
    d["challenge_token"].as_str().unwrap().to_owned()
}

async fn verify(app: &App, challenge: &str, body: Value) -> Value {
    let mut b = body;
    b["challenge_token"] = json!(challenge);
    app.call("POST", "/api/v1/admin/login/verify-2fa", Some(b), None)
        .await
}

async fn events(app: &App) -> Vec<(String, String, String)> {
    use zs_infra::db::entity::admin_login_logs as l;
    l::Entity::find()
        .order_by_asc(l::Column::Id)
        .all(&app.db)
        .await
        .unwrap()
        .into_iter()
        .map(|r| (r.event_type, r.status, r.fail_reason))
        .collect()
}

#[tokio::test]
async fn status_setup_enable_and_login_with_challenge() {
    let app = App::new().await;
    let st = app
        .admin_call("GET", "/api/v1/admin/2fa/status", None)
        .await;
    assert_eq!(
        data(&st),
        &json!({"enabled": false, "recovery_codes_remaining": 0, "recovery_codes_total": 0})
    );
    let enable_without_setup = app
        .admin_call(
            "POST",
            "/api/v1/admin/2fa/enable",
            Some(json!({"code": "123456"})),
        )
        .await;
    err(&enable_without_setup, 400, "绑定流程已超时，请重新发起");

    let token = app.admin.clone();
    let (secret, codes) = enable(&app, &token).await;
    let st = app
        .admin_call("GET", "/api/v1/admin/2fa/status", None)
        .await;
    let d = data(&st);
    assert_eq!(d["enabled"], true);
    assert_eq!(d["recovery_codes_remaining"], 10);
    // Admin sessions survive enabling 2FA.
    data(&app.admin_call("GET", "/api/v1/admin/authz/me", None).await);
    err(
        &app.admin_call("POST", "/api/v1/admin/2fa/setup", None)
            .await,
        400,
        "已启用两步验证，无需重复绑定",
    );

    // AUTH-03: password alone only yields a challenge.
    let challenge = login_challenge(&app).await;
    // The challenge token is not an access token.
    let as_access = app
        .call("GET", "/api/v1/admin/authz/me", None, Some(&challenge))
        .await;
    assert_eq!(as_access["status_code"], 401);
    // An access token is not a challenge token.
    let wrong_kind = verify(&app, &token, json!({"code": totp(&secret)})).await;
    err(&wrong_kind, 401, "登录会话已失效，请重新输入密码");
    let missing = verify(&app, &challenge, json!({})).await;
    err(&missing, 400, "请提供动态验证码或恢复码");

    let bad = verify(&app, &challenge, json!({"code": wrong_totp(&secret)})).await;
    err(&bad, 401, "动态验证码错误");
    let ok = verify(&app, &challenge, json!({"code": totp(&secret)})).await;
    let d = data(&ok);
    assert_eq!(d["requires_totp"], false);
    assert_eq!(d["user"]["username"], "admin");
    let new_token = d["token"].as_str().unwrap();
    data(
        &app.call("GET", "/api/v1/admin/authz/me", None, Some(new_token))
            .await,
    );
    // AUTH-03: the challenge is one-shot.
    let replay = verify(&app, &challenge, json!({"code": totp(&secret)})).await;
    err(&replay, 401, "登录会话已失效，请重新输入密码");

    // Recovery codes work once.
    let c2 = login_challenge(&app).await;
    data(&verify(&app, &c2, json!({"recovery_code": codes[0]})).await);
    let c3 = login_challenge(&app).await;
    err(
        &verify(&app, &c3, json!({"recovery_code": codes[0]})).await,
        401,
        "恢复码错误或已使用",
    );
    let st = app
        .call("GET", "/api/v1/admin/2fa/status", None, Some(new_token))
        .await;
    assert_eq!(data(&st)["recovery_codes_remaining"], 9);

    let ev = events(&app).await;
    let has = |e: &str, s: &str, r: &str| ev.iter().any(|(a, b, c)| a == e && b == s && c == r);
    assert!(has("2fa_setup", "success", ""));
    assert!(has("2fa_enabled", "success", ""));
    assert!(has("login_password", "success", ""));
    assert!(has("login_2fa_verify", "failed", "invalid_totp_code"));
    assert!(has("login_2fa_verify", "success", ""));
    assert!(has("login_recovery_code", "success", ""));
    assert!(has(
        "login_recovery_code",
        "failed",
        "invalid_recovery_code"
    ));
}

// AUTH-03 (2)-1: five failures revoke the challenge even for a correct code.
#[tokio::test]
async fn challenge_revoked_after_five_failures() {
    let app = App::new().await;
    let token = app.admin.clone();
    let (secret, _) = enable(&app, &token).await;
    let challenge = login_challenge(&app).await;
    for _ in 0..4 {
        err(
            &verify(&app, &challenge, json!({"code": wrong_totp(&secret)})).await,
            401,
            "动态验证码错误",
        );
    }
    err(
        &verify(&app, &challenge, json!({"code": wrong_totp(&secret)})).await,
        401,
        "失败次数过多，请稍后重试",
    );
    err(
        &verify(&app, &challenge, json!({"code": totp(&secret)})).await,
        401,
        "登录会话已失效，请重新输入密码",
    );
    let garbage = verify(&app, "garbage", json!({"code": "123456"})).await;
    err(&garbage, 401, "登录会话已失效，请重新输入密码");
}

#[tokio::test]
async fn disable_and_regenerate() {
    let app = App::new().await;
    let token = app.admin.clone();
    let (secret, codes) = enable(&app, &token).await;

    // Regenerate requires a TOTP code; recovery codes are refused.
    err(
        &app.admin_call(
            "POST",
            "/api/v1/admin/2fa/recovery-codes/regenerate",
            Some(json!({"code": codes[1]})),
        )
        .await,
        400,
        "动态验证码错误",
    );
    let regen = app
        .admin_call(
            "POST",
            "/api/v1/admin/2fa/recovery-codes/regenerate",
            Some(json!({"code": totp(&secret)})),
        )
        .await;
    let fresh: Vec<String> = data(&regen)["recovery_codes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(fresh.len(), 10);

    err(
        &app.admin_call("POST", "/api/v1/admin/2fa/disable", Some(json!({})))
            .await,
        400,
        "请提供动态验证码或恢复码",
    );
    // Old recovery codes were replaced.
    err(
        &app.admin_call(
            "POST",
            "/api/v1/admin/2fa/disable",
            Some(json!({"recovery_code": codes[2]})),
        )
        .await,
        400,
        "恢复码错误或已使用",
    );
    data(
        &app.admin_call(
            "POST",
            "/api/v1/admin/2fa/disable",
            Some(json!({"recovery_code": fresh[0]})),
        )
        .await,
    );
    // AUTH-03 (2)-5: disabling revokes the old access token.
    let old = app
        .call("GET", "/api/v1/admin/2fa/status", None, Some(&token))
        .await;
    err(&old, 401, "登录状态已失效，请重新登录");
    let relogin = app
        .call(
            "POST",
            "/api/v1/admin/login",
            Some(json!({"username": "admin", "password": PW})),
            None,
        )
        .await;
    let t2 = data(&relogin)["token"].as_str().unwrap().to_owned();
    err(
        &app.call(
            "POST",
            "/api/v1/admin/2fa/disable",
            Some(json!({"code": "123456"})),
            Some(&t2),
        )
        .await,
        400,
        "尚未启用两步验证",
    );
    let ev = events(&app).await;
    assert!(
        ev.iter()
            .any(|e| e.0 == "recovery_regenerated" && e.1 == "success")
    );
    assert!(
        ev.iter()
            .any(|e| e.0 == "2fa_disabled" && e.2 == "invalid_recovery_code")
    );
    assert!(ev.iter().any(|e| e.0 == "2fa_disabled" && e.1 == "success"));
}

#[tokio::test]
async fn super_admin_resets_other_admins_2fa() {
    let app = App::new().await;
    let (ops_id, ops_token) = app
        .create_admin("ops", "Ops12345", &["readonly_auditor"])
        .await;
    let (secret, _) = enable(&app, &ops_token).await;
    let _ = secret;

    // Non-super admins are refused (readonly_auditor has no RBAC right either).
    let denied = app
        .call(
            "POST",
            &format!("/api/v1/admin/authz/admins/{ops_id}/2fa/reset"),
            None,
            Some(&ops_token),
        )
        .await;
    assert_eq!(denied["status_code"], 403);

    let me = data(&app.admin_call("GET", "/api/v1/admin/authz/me", None).await)["admin_id"]
        .as_i64()
        .unwrap();
    err(
        &app.admin_call(
            "POST",
            &format!("/api/v1/admin/authz/admins/{me}/2fa/reset"),
            None,
        )
        .await,
        400,
        "无法通过此入口重置自己的 2FA，请使用 admin-tool CLI",
    );
    err(
        &app.admin_call("POST", "/api/v1/admin/authz/admins/abc/2fa/reset", None)
            .await,
        400,
        "请求参数错误",
    );
    err(
        &app.admin_call("POST", "/api/v1/admin/authz/admins/999/2fa/reset", None)
            .await,
        404,
        "用户不存在",
    );
    data(
        &app.admin_call(
            "POST",
            &format!("/api/v1/admin/authz/admins/{ops_id}/2fa/reset"),
            None,
        )
        .await,
    );
    // The target's sessions are revoked and 2FA is gone.
    assert_eq!(
        app.call("GET", "/api/v1/admin/authz/me", None, Some(&ops_token))
            .await["status_code"],
        401
    );
    let relogin = app
        .call(
            "POST",
            "/api/v1/admin/login",
            Some(json!({"username": "ops", "password": "Ops12345"})),
            None,
        )
        .await;
    assert_eq!(data(&relogin)["requires_totp"], false);

    use zs_infra::db::entity::admin_login_logs as l;
    let reset = l::Entity::find()
        .all(&app.db)
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.event_type == "2fa_reset_by_admin")
        .unwrap();
    assert_eq!(reset.admin_id, ops_id);
    assert_eq!(reset.operator_id, Some(me));
}

// ADM-05: the CLI password reset revokes old sessions; CLI 2FA reset clears 2FA.
#[tokio::test]
async fn cli_resets_revoke_sessions() {
    let app = App::new().await;
    let old = app.admin.clone();
    enable(&app, &old).await;
    app.services
        .identity
        .admin_auth
        .cli_reset_password("admin", "Changed123")
        .await
        .unwrap();
    err(
        &app.call("GET", "/api/v1/admin/authz/me", None, Some(&old))
            .await,
        401,
        "登录状态已失效，请重新登录",
    );
    assert!(
        app.services
            .identity
            .admin_auth
            .cli_reset_password("admin", "short")
            .await
            .is_err()
    );
    app.services
        .identity
        .admin_2fa
        .cli_reset("admin")
        .await
        .unwrap();
    let relogin = app
        .call(
            "POST",
            "/api/v1/admin/login",
            Some(json!({"username": "admin", "password": "Changed123"})),
            None,
        )
        .await;
    assert_eq!(data(&relogin)["requires_totp"], false);
    let ev = events(&app).await;
    assert!(ev.iter().any(|e| e.0 == "password_reset_by_cli"));
    assert!(ev.iter().any(|e| e.0 == "2fa_reset_by_admin"));
}
