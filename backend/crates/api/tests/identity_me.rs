//! `/me`: profile, password, email change, login history, 2FA management and
//! the admin reset of a user's 2FA.

mod identity_common;

use identity_common::{App, data, err, totp};
use serde_json::json;

const PW: &str = "User12345";

#[tokio::test]
async fn profile_contract_and_update() {
    let app = App::new().await;
    assert_eq!(
        app.call("GET", "/api/v1/me", None, None).await["status_code"],
        401
    );
    let token = app.register("p@b.io", PW).await;
    let me = app.call("GET", "/api/v1/me", None, Some(&token)).await;
    let d = data(&me);
    assert_eq!(d["email"], "p@b.io");
    assert_eq!(d["nickname"], "p");
    assert_eq!(d["locale"], "zh-CN");
    assert_eq!(d["member_level_id"], 0);
    assert_eq!(d["total_recharged"], "0.00");
    assert_eq!(d["total_spent"], "0.00");
    assert_eq!(d["email_change_mode"], "change_with_old_and_new");
    assert_eq!(d["password_change_mode"], "change_with_old");
    assert!(d["email_verified_at"].is_string());
    assert!(d.get("password_hash").is_none());

    err(
        &app.call(
            "PUT",
            "/api/v1/me/profile",
            Some(json!({"nickname": "  "})),
            Some(&token),
        )
        .await,
        400,
        "请至少填写一项资料",
    );
    let updated = app
        .call(
            "PUT",
            "/api/v1/me/profile",
            Some(json!({"nickname": " Neo ", "locale": "en-US"})),
            Some(&token),
        )
        .await;
    assert_eq!(data(&updated)["nickname"], "Neo");
    assert_eq!(data(&updated)["locale"], "en-US");
}

#[tokio::test]
async fn change_password_revokes_sessions() {
    let app = App::new().await;
    let token = app.register("pw@b.io", PW).await;
    err(
        &app.call(
            "PUT",
            "/api/v1/me/password",
            Some(json!({"old_password": "Wrong1234", "new_password": "Next12345"})),
            Some(&token),
        )
        .await,
        400,
        "旧密码错误",
    );
    err(
        &app.call(
            "PUT",
            "/api/v1/me/password",
            Some(json!({"old_password": PW, "new_password": "next12345"})),
            Some(&token),
        )
        .await,
        400,
        "需包含大写字母",
    );
    let ok = app
        .call(
            "PUT",
            "/api/v1/me/password",
            Some(json!({"old_password": PW, "new_password": "Next12345"})),
            Some(&token),
        )
        .await;
    assert_eq!(data(&ok), &json!({"updated": true}));
    err(
        &app.call("GET", "/api/v1/me", None, Some(&token)).await,
        401,
        "登录状态已失效，请重新登录",
    );
    app.user_login("pw@b.io", "Next12345").await;
}

#[tokio::test]
async fn change_email_with_both_codes() {
    let app = App::new().await;
    let token = app.register("old@b.io", PW).await;
    app.register("taken@b.io", PW).await;
    let send = |body: serde_json::Value| {
        let app = &app;
        let token = token.clone();
        async move {
            app.call(
                "POST",
                "/api/v1/me/email/send-verify-code",
                Some(body),
                Some(&token),
            )
            .await
        }
    };
    err(
        &send(json!({"kind": "weird"})).await,
        400,
        "更换邮箱参数不合法",
    );
    err(
        &send(json!({"kind": "new", "new_email": "old@b.io"})).await,
        400,
        "更换邮箱参数不合法",
    );
    err(
        &send(json!({"kind": "new", "new_email": "taken@b.io"})).await,
        400,
        "新邮箱已被注册",
    );
    // AUTH-06
    err(
        &send(json!({"kind": "new", "new_email": "telegram_abc@login.local"})).await,
        400,
        "邮箱格式不正确",
    );
    assert_eq!(
        data(&send(json!({"kind": "old"})).await),
        &json!({"sent": true})
    );
    data(&send(json!({"kind": "new", "new_email": "new@b.io"})).await);
    let old_code = app.last_code("old@b.io", "change_email_old").await;
    let new_code = app.last_code("new@b.io", "change_email_new").await;

    let change = |body: serde_json::Value| {
        let app = &app;
        let token = token.clone();
        async move {
            app.call("POST", "/api/v1/me/email/change", Some(body), Some(&token))
                .await
        }
    };
    err(
        &change(json!({"new_email": "new@b.io", "old_code": "x", "new_code": new_code})).await,
        400,
        "验证码错误",
    );
    let ok =
        change(json!({"new_email": "New@b.io", "old_code": old_code, "new_code": new_code})).await;
    let d = data(&ok);
    assert_eq!(d["email"], "new@b.io");
    assert_eq!(d["email_change_mode"], "change_with_old_and_new");
    app.user_login("new@b.io", PW).await;
}

#[tokio::test]
async fn own_login_history() {
    let app = App::new().await;
    app.register("h@b.io", PW).await;
    let token = app.user_login("h@b.io", PW).await;
    app.call(
        "POST",
        "/api/v1/auth/login",
        Some(json!({"email": "h@b.io", "password": "Wrong1234"})),
        None,
    )
    .await;
    let res = app
        .call(
            "GET",
            "/api/v1/me/login-logs?page=1&page_size=500",
            None,
            Some(&token),
        )
        .await;
    let items = data(&res).as_array().unwrap();
    assert_eq!(items.len(), 1, "failed attempts carry no user id");
    let item = &items[0];
    assert_eq!(item["status"], "success");
    assert_eq!(item["email"], "h@b.io");
    assert_eq!(item["login_source"], "web");
    // Internal audit fields are not exposed to users.
    assert!(item.get("fail_reason").is_none());
    assert!(item.get("request_id").is_none());
    assert!(item.get("user_id").is_none());
    assert_eq!(res["pagination"]["page_size"], 100);
    assert_eq!(res["pagination"]["total"], 1);
}

#[tokio::test]
async fn user_2fa_management_and_admin_reset() {
    let app = App::new().await;
    let token = app.register("z@b.io", PW).await;
    let st = app
        .call("GET", "/api/v1/me/2fa/status", None, Some(&token))
        .await;
    assert_eq!(data(&st)["enabled"], false);
    err(
        &app.call(
            "POST",
            "/api/v1/me/2fa/enable",
            Some(json!({"code": "123456"})),
            Some(&token),
        )
        .await,
        400,
        "绑定流程已超时，请重新发起",
    );
    let setup = app
        .call("POST", "/api/v1/me/2fa/setup", None, Some(&token))
        .await;
    let secret = data(&setup)["secret"].as_str().unwrap().to_owned();
    let enabled = app
        .call(
            "POST",
            "/api/v1/me/2fa/enable",
            Some(json!({"code": totp(&secret)})),
            Some(&token),
        )
        .await;
    let d = data(&enabled);
    assert_eq!(d["recovery_codes"].as_array().unwrap().len(), 10);
    assert!(d["expires_at"].is_string());
    let token = d["token"].as_str().unwrap().to_owned();
    let codes: Vec<String> = d["recovery_codes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap().to_owned())
        .collect();
    err(
        &app.call("POST", "/api/v1/me/2fa/setup", None, Some(&token))
            .await,
        400,
        "已启用两步验证，无需重复绑定",
    );
    let regen = app
        .call(
            "POST",
            "/api/v1/me/2fa/recovery-codes/regenerate",
            Some(json!({"code": totp(&secret)})),
            Some(&token),
        )
        .await;
    assert_eq!(data(&regen)["recovery_codes"].as_array().unwrap().len(), 10);
    err(
        &app.call(
            "POST",
            "/api/v1/me/2fa/disable",
            Some(json!({"recovery_code": codes[0]})),
            Some(&token),
        )
        .await,
        400,
        "恢复码错误或已使用",
    );
    err(
        &app.call(
            "POST",
            "/api/v1/me/2fa/disable",
            Some(json!({})),
            Some(&token),
        )
        .await,
        400,
        "请提供动态验证码或恢复码",
    );

    // Admin reset: validation, success and session revocation.
    let uid = data(&app.call("GET", "/api/v1/me", None, Some(&token)).await)["id"]
        .as_i64()
        .unwrap();
    err(
        &app.admin_call("DELETE", "/api/v1/admin/users/abc/2fa", None)
            .await,
        400,
        "无效的用户ID",
    );
    err(
        &app.admin_call("DELETE", "/api/v1/admin/users/9999/2fa", None)
            .await,
        404,
        "用户不存在",
    );
    data(
        &app.admin_call("DELETE", &format!("/api/v1/admin/users/{uid}/2fa"), None)
            .await,
    );
    err(
        &app.call("GET", "/api/v1/me", None, Some(&token)).await,
        401,
        "登录状态已失效，请重新登录",
    );
    err(
        &app.admin_call("DELETE", &format!("/api/v1/admin/users/{uid}/2fa"), None)
            .await,
        400,
        "尚未启用两步验证",
    );
    let relogin = app
        .call(
            "POST",
            "/api/v1/auth/login",
            Some(json!({"email": "z@b.io", "password": PW})),
            None,
        )
        .await;
    assert_eq!(data(&relogin)["requires_totp"], false);
}

// AUTH-03 (1): tokens whose `typ` is not `access` are rejected.
#[tokio::test]
async fn non_access_typ_is_rejected() {
    use zs_app::identity::jwt::{Registered, Signer};
    use zs_app::identity::user_auth::UserClaims;
    let app = App::new().await;
    let token = app.register("typ@b.io", PW).await;
    let id = data(&app.call("GET", "/api/v1/me", None, Some(&token)).await)["id"]
        .as_i64()
        .unwrap();
    let signer = Signer::new(&app.cfg.user_jwt.secret);
    let mint = |typ: &str| {
        signer
            .sign(&UserClaims {
                user_id: id,
                email: "typ@b.io".into(),
                token_version: 0,
                typ: typ.into(),
                reg: Registered::new(chrono::Utc::now(), chrono::Duration::hours(1)),
            })
            .unwrap()
    };
    for typ in ["refresh", "2fa_challenge"] {
        err(
            &app.call("GET", "/api/v1/me", None, Some(&mint(typ))).await,
            401,
            "无效的 token",
        );
    }
    // Legacy tokens without `typ` stay valid, like the original.
    data(&app.call("GET", "/api/v1/me", None, Some(&mint(""))).await);
    // User tokens are not admin tokens.
    assert_eq!(
        app.call("GET", "/api/v1/admin/authz/me", None, Some(&token))
            .await["status_code"],
        401
    );
}
