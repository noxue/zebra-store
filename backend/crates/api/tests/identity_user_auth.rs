//! Storefront registration, login (with 2FA), verification codes, password
//! reset, captcha and login rate limiting.

#![expect(
    clippy::unwrap_used,
    reason = "integration test helpers abort on failure"
)]

mod identity_common;

use axum::http::StatusCode;
use identity_common::{App, config, data, err, totp, wrong_totp};
use sea_orm::{ActiveModelTrait, EntityTrait, PaginatorTrait, Set};
use serde_json::{Value, json};

const PW: &str = "User12345";

async fn post(app: &App, uri: &str, body: Value) -> Value {
    app.call("POST", uri, Some(body), None).await
}

async fn user_count(app: &App) -> u64 {
    zs_infra::db::entity::users::Entity::find()
        .count(&app.db)
        .await
        .unwrap()
}

#[tokio::test]
async fn register_with_email_code() {
    let app = App::new().await;
    // Defaults: registration and email verification enabled.
    let no_code = post(
        &app,
        "/api/v1/auth/register",
        json!({"email": "a@b.io", "password": PW, "agreement_accepted": true}),
    )
    .await;
    err(&no_code, 400, "验证码错误");
    err(
        &post(
            &app,
            "/api/v1/auth/register",
            json!({"email": "a@b.io", "password": PW, "code": "1"}),
        )
        .await,
        400,
        "请先同意隐私政策和服务条款",
    );
    err(
        &post(&app, "/api/v1/auth/register", json!({"email": "a@b.io"})).await,
        400,
        "Password: 不能为空",
    );
    let sent = post(
        &app,
        "/api/v1/auth/send-verify-code",
        json!({"email": " A@B.io ", "purpose": "register"}),
    )
    .await;
    assert_eq!(data(&sent), &json!({"sent": true}));
    err(
        &post(
            &app,
            "/api/v1/auth/send-verify-code",
            json!({"email": "a@b.io", "purpose": "register"}),
        )
        .await,
        429,
        "发送过于频繁，请稍后重试",
    );
    let code = app.last_code("a@b.io", "register").await;
    assert_eq!(code.len(), 6);
    err(
        &post(
            &app,
            "/api/v1/auth/register",
            json!({"email": "a@b.io", "password": "weak", "code": code, "agreement_accepted": true}),
        )
        .await,
        400,
        "密码长度至少 8 位",
    );
    let ok = post(
        &app,
        "/api/v1/auth/register",
        json!({"email": "A@b.io", "password": PW, "code": code, "agreement_accepted": true}),
    )
    .await;
    let d = data(&ok);
    assert_eq!(d["user"]["email"], "a@b.io");
    assert_eq!(d["user"]["nickname"], "a");
    assert!(d["user"]["email_verified_at"].is_string());
    assert!(d["expires_at"].as_str().unwrap().ends_with('Z'));
    let token = d["token"].as_str().unwrap();
    data(&app.call("GET", "/api/v1/me", None, Some(token)).await);
    err(
        &post(
            &app,
            "/api/v1/auth/register",
            json!({"email": "a@b.io", "password": PW, "code": code, "agreement_accepted": true}),
        )
        .await,
        400,
        "邮箱已注册",
    );
    err(
        &post(
            &app,
            "/api/v1/auth/send-verify-code",
            json!({"email": "a@b.io", "purpose": "register"}),
        )
        .await,
        400,
        "邮箱已注册",
    );
    err(
        &post(
            &app,
            "/api/v1/auth/send-verify-code",
            json!({"email": "x@b.io", "purpose": "nonsense"}),
        )
        .await,
        400,
        "验证码用途无效",
    );
    err(
        &post(
            &app,
            "/api/v1/auth/send-verify-code",
            json!({"email": "bad", "purpose": "register"}),
        )
        .await,
        400,
        "邮箱格式不正确",
    );
}

// Lesson 2b47bf5d: new users get the default member level.
#[tokio::test]
async fn registration_assigns_default_member_level() {
    let app = App::new().await;
    let now = chrono::Utc::now();
    let level = zs_infra::db::entity::member_levels::ActiveModel {
        name_json: Set(Some(json!({"zh-CN": "普通"}))),
        slug: Set("normal".into()),
        icon: Set(String::new()),
        discount_rate: Set(sea_orm::prelude::Decimal::from(100)),
        recharge_threshold: Set(sea_orm::prelude::Decimal::ZERO),
        spend_threshold: Set(sea_orm::prelude::Decimal::ZERO),
        is_default: Set(true),
        sort_order: Set(0),
        is_active: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&app.db)
    .await
    .unwrap();
    let token = app.register("lv@b.io", PW).await;
    let me = app.call("GET", "/api/v1/me", None, Some(&token)).await;
    assert_eq!(data(&me)["member_level_id"], level.id);
}

// AUTH-05: switches are enforced by the backend on every entry point.
#[tokio::test]
async fn registration_and_verification_switches() {
    let app = App::new().await;
    app.set_setting(
        "registration_config",
        json!({"registration_enabled": false}),
    )
    .await;
    err(
        &post(
            &app,
            "/api/v1/auth/register",
            json!({"email": "c@b.io", "password": PW, "agreement_accepted": true}),
        )
        .await,
        403,
        "注册功能已关闭",
    );
    err(
        &post(
            &app,
            "/api/v1/auth/send-verify-code",
            json!({"email": "c@b.io", "purpose": "register"}),
        )
        .await,
        403,
        "注册功能已关闭",
    );
    assert_eq!(user_count(&app).await, 0);

    app.set_setting(
        "registration_config",
        json!({"registration_enabled": true, "email_verification_enabled": false}),
    )
    .await;
    for purpose in ["register", "reset"] {
        err(
            &post(
                &app,
                "/api/v1/auth/send-verify-code",
                json!({"email": "c@b.io", "purpose": purpose}),
            )
            .await,
            403,
            "邮箱验证功能已关闭",
        );
    }
    err(
        &post(
            &app,
            "/api/v1/auth/forgot-password",
            json!({"email": "c@b.io", "code": "1", "new_password": PW}),
        )
        .await,
        403,
        "密码重置功能已关闭，请联系管理员修改密码",
    );
    // Without verification a code is not needed.
    let ok = post(
        &app,
        "/api/v1/auth/register",
        json!({"email": "c@b.io", "password": PW, "agreement_accepted": true}),
    )
    .await;
    data(&ok);
    assert_eq!(app.code_count("c@b.io").await, 0);
}

// AUTH-06: Telegram placeholder addresses cannot be registered.
#[tokio::test]
async fn placeholder_emails_rejected() {
    let app = App::new().await;
    app.set_setting(
        "registration_config",
        json!({"email_verification_enabled": false}),
    )
    .await;
    err(
        &post(
            &app,
            "/api/v1/auth/register",
            json!({"email": "Telegram_1@LOGIN.local", "password": PW, "agreement_accepted": true}),
        )
        .await,
        400,
        "邮箱格式不正确",
    );
    app.set_setting("registration_config", json!({})).await;
    err(
        &post(
            &app,
            "/api/v1/auth/send-verify-code",
            json!({"email": "telegram_9@login.local", "purpose": "register"}),
        )
        .await,
        400,
        "邮箱格式不正确",
    );
}

// AUTH-07: exact-domain allowlist, checked before any email is sent.
#[tokio::test]
async fn email_domain_allowlist() {
    let app = App::new().await;
    app.set_setting(
        "registration_config",
        json!({"email_domain_allowlist_enabled": true, "allowed_email_domains": ["gmail.com"]}),
    )
    .await;
    err(
        &post(
            &app,
            "/api/v1/auth/send-verify-code",
            json!({"email": "a@qq.com", "purpose": "register"}),
        )
        .await,
        400,
        "当前邮箱后缀不允许注册",
    );
    assert_eq!(app.code_count("a@qq.com").await, 0);
    err(
        &post(
            &app,
            "/api/v1/auth/send-verify-code",
            json!({"email": "a@mail.gmail.com", "purpose": "register"}),
        )
        .await,
        400,
        "当前邮箱后缀不允许注册",
    );
    data(
        &post(
            &app,
            "/api/v1/auth/send-verify-code",
            json!({"email": "a@GMAIL.COM", "purpose": "register"}),
        )
        .await,
    );
    assert_eq!(app.code_count("a@gmail.com").await, 1);
    app.set_setting(
        "registration_config",
        json!({"email_verification_enabled": false, "email_domain_allowlist_enabled": true, "allowed_email_domains": "gmail.com"}),
    )
    .await;
    err(
        &post(
            &app,
            "/api/v1/auth/register",
            json!({"email": "z@qq.com", "password": PW, "agreement_accepted": true}),
        )
        .await,
        400,
        "当前邮箱后缀不允许注册",
    );
}

#[tokio::test]
async fn login_errors_and_logs() {
    let app = App::new().await;
    app.register("u@b.io", PW).await;
    let ok = post(
        &app,
        "/api/v1/auth/login",
        json!({"email": " U@B.io ", "password": PW, "remember_me": true}),
    )
    .await;
    let d = data(&ok);
    assert_eq!(d["requires_totp"], false);
    assert_eq!(d["user"]["email"], "u@b.io");
    // remember_me → 168 h lifetime.
    let exp = chrono::DateTime::parse_from_rfc3339(d["expires_at"].as_str().unwrap()).unwrap();
    assert!(exp.timestamp() - chrono::Utc::now().timestamp() > 160 * 3600);

    err(
        &post(
            &app,
            "/api/v1/auth/login",
            json!({"email": "u@b.io", "password": "Wrong1234"}),
        )
        .await,
        401,
        "邮箱或密码错误",
    );
    err(
        &post(
            &app,
            "/api/v1/auth/login",
            json!({"email": "nobody@b.io", "password": PW}),
        )
        .await,
        401,
        "邮箱或密码错误",
    );
    err(
        &post(
            &app,
            "/api/v1/auth/login",
            json!({"email": "bad", "password": PW}),
        )
        .await,
        400,
        "邮箱格式不正确",
    );
    err(
        &post(&app, "/api/v1/auth/login", json!({"email": "u@b.io"})).await,
        400,
        "请求参数错误",
    );
    // Disabled accounts (with the right password) are told so.
    let users = &app.services.identity.user_auth;
    let mut u = users.repo().get_by_email("u@b.io").await.unwrap().unwrap();
    u.status = "disabled".into();
    users.repo().save(&u).await.unwrap();
    err(
        &post(
            &app,
            "/api/v1/auth/login",
            json!({"email": "u@b.io", "password": PW}),
        )
        .await,
        401,
        "账号已禁用",
    );
    // AUTH-09: without the password the state is not revealed.
    err(
        &post(
            &app,
            "/api/v1/auth/login",
            json!({"email": "u@b.io", "password": "Nope12345"}),
        )
        .await,
        401,
        "邮箱或密码错误",
    );
    u.status = "active".into();
    u.email_verified_at = None;
    users.repo().save(&u).await.unwrap();
    err(
        &post(
            &app,
            "/api/v1/auth/login",
            json!({"email": "u@b.io", "password": PW}),
        )
        .await,
        401,
        "邮箱未验证",
    );

    let logs = app
        .admin_call("GET", "/api/v1/admin/user-login-logs?page_size=50", None)
        .await;
    let items = data(&logs).as_array().unwrap();
    let reasons: Vec<&str> = items
        .iter()
        .map(|i| i["fail_reason"].as_str().unwrap())
        .collect();
    for r in [
        "",
        "invalid_credentials",
        "invalid_email",
        "bad_request",
        "user_disabled",
        "email_not_verified",
    ] {
        assert!(reasons.contains(&r), "missing {r} in {reasons:?}");
    }
    let first = &items[0];
    for key in [
        "id",
        "user_id",
        "email",
        "status",
        "fail_reason",
        "client_ip",
        "user_agent",
        "login_source",
        "request_id",
        "created_at",
    ] {
        assert!(first.get(key).is_some(), "missing {key}");
    }
    assert_eq!(first["login_source"], "web");
    let filtered = app
        .admin_call("GET", "/api/v1/admin/user-login-logs?status=success", None)
        .await;
    assert!(
        data(&filtered)
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["status"] == "success")
    );
    err(
        &app.admin_call("GET", "/api/v1/admin/user-login-logs?user_id=-3", None)
            .await,
        400,
        "请求参数错误",
    );
}

#[tokio::test]
async fn forgot_password_flow() {
    let app = App::new().await;
    let old_token = app.register("f@b.io", PW).await;
    app.set_setting("registration_config", json!({})).await;
    err(
        &post(
            &app,
            "/api/v1/auth/send-verify-code",
            json!({"email": "missing@b.io", "purpose": "reset"}),
        )
        .await,
        404,
        "用户不存在",
    );
    data(
        &post(
            &app,
            "/api/v1/auth/send-verify-code",
            json!({"email": "f@b.io", "purpose": "reset"}),
        )
        .await,
    );
    let code = app.last_code("f@b.io", "reset").await;
    err(
        &post(
            &app,
            "/api/v1/auth/forgot-password",
            json!({"email": "f@b.io", "code": "000000x", "new_password": "NewPass123"}),
        )
        .await,
        400,
        "验证码错误",
    );
    let ok = post(
        &app,
        "/api/v1/auth/forgot-password",
        json!({"email": "f@b.io", "code": code, "new_password": "NewPass123"}),
    )
    .await;
    assert_eq!(data(&ok), &json!({"reset": true}));
    // Old sessions are revoked, the code is consumed.
    err(
        &app.call("GET", "/api/v1/me", None, Some(&old_token)).await,
        401,
        "登录状态已失效，请重新登录",
    );
    err(
        &post(
            &app,
            "/api/v1/auth/forgot-password",
            json!({"email": "f@b.io", "code": code, "new_password": "Again1234"}),
        )
        .await,
        400,
        "验证码错误",
    );
    app.user_login("f@b.io", "NewPass123").await;
}

// Verification codes lock after too many wrong attempts.
#[tokio::test]
async fn verify_code_attempt_limit() {
    let app = App::new().await;
    app.register("g@b.io", PW).await;
    app.set_setting("registration_config", json!({})).await;
    data(
        &post(
            &app,
            "/api/v1/auth/send-verify-code",
            json!({"email": "g@b.io", "purpose": "reset"}),
        )
        .await,
    );
    let code = app.last_code("g@b.io", "reset").await;
    for _ in 0..5 {
        err(
            &post(
                &app,
                "/api/v1/auth/forgot-password",
                json!({"email": "g@b.io", "code": "x", "new_password": "NewPass123"}),
            )
            .await,
            400,
            "验证码错误",
        );
    }
    err(
        &post(
            &app,
            "/api/v1/auth/forgot-password",
            json!({"email": "g@b.io", "code": code, "new_password": "NewPass123"}),
        )
        .await,
        400,
        "验证码尝试次数过多",
    );
}

async fn enable_user_2fa(app: &App, token: &str) -> (String, Vec<String>, String) {
    let setup = app
        .call("POST", "/api/v1/me/2fa/setup", None, Some(token))
        .await;
    let secret = data(&setup)["secret"].as_str().unwrap().to_owned();
    assert!(
        data(&setup)["otpauth_url"]
            .as_str()
            .unwrap()
            .contains("Zebra-Store")
    );
    let res = app
        .call(
            "POST",
            "/api/v1/me/2fa/enable",
            Some(json!({"code": totp(&secret)})),
            Some(token),
        )
        .await;
    let d = data(&res);
    let codes = d["recovery_codes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap().to_owned())
        .collect();
    (secret, codes, d["token"].as_str().unwrap().to_owned())
}

// AUTH-03: user 2FA login challenge.
#[tokio::test]
async fn user_login_with_2fa() {
    let app = App::new().await;
    let first = app.register("t@b.io", PW).await;
    let (secret, codes, fresh) = enable_user_2fa(&app, &first).await;
    // Enabling revokes the old session but hands out a fresh token.
    err(
        &app.call("GET", "/api/v1/me", None, Some(&first)).await,
        401,
        "登录状态已失效，请重新登录",
    );
    data(&app.call("GET", "/api/v1/me", None, Some(&fresh)).await);

    let step1 = post(
        &app,
        "/api/v1/auth/login",
        json!({"email": "t@b.io", "password": PW, "remember_me": true}),
    )
    .await;
    let d = data(&step1);
    assert_eq!(d["requires_totp"], true);
    assert!(d.get("token").is_none());
    let challenge = d["challenge_token"].as_str().unwrap().to_owned();
    // A challenge token is not an access token (AUTH-03 (1)).
    assert_eq!(
        app.call("GET", "/api/v1/me", None, Some(&challenge)).await["status_code"],
        401
    );
    let verify = |body: Value| {
        let app = &app;
        async move { post(app, "/api/v1/auth/login/verify-2fa", body).await }
    };
    err(
        &verify(json!({"challenge_token": challenge})).await,
        400,
        "请提供动态验证码或恢复码",
    );
    err(
        &verify(json!({"challenge_token": fresh, "code": totp(&secret)})).await,
        401,
        "登录会话已失效，请重新输入密码",
    );
    err(
        &verify(json!({"challenge_token": challenge, "code": wrong_totp(&secret)})).await,
        401,
        "动态验证码错误",
    );
    let ok = verify(json!({"challenge_token": challenge, "code": totp(&secret)})).await;
    let d = data(&ok);
    assert_eq!(d["user"]["email"], "t@b.io");
    let exp = chrono::DateTime::parse_from_rfc3339(d["expires_at"].as_str().unwrap()).unwrap();
    assert!(
        exp.timestamp() - chrono::Utc::now().timestamp() > 160 * 3600,
        "remember_me kept"
    );
    err(
        &verify(json!({"challenge_token": challenge, "code": totp(&secret)})).await,
        401,
        "登录会话已失效，请重新输入密码",
    );

    // Recovery code once; five failures revoke the challenge.
    let c2 = data(
        &post(
            &app,
            "/api/v1/auth/login",
            json!({"email": "t@b.io", "password": PW}),
        )
        .await,
    )["challenge_token"]
        .as_str()
        .unwrap()
        .to_owned();
    data(&verify(json!({"challenge_token": c2, "recovery_code": codes[0]})).await);
    let c3 = data(
        &post(
            &app,
            "/api/v1/auth/login",
            json!({"email": "t@b.io", "password": PW}),
        )
        .await,
    )["challenge_token"]
        .as_str()
        .unwrap()
        .to_owned();
    err(
        &verify(json!({"challenge_token": c3, "recovery_code": codes[0]})).await,
        401,
        "恢复码错误或已使用",
    );
    for _ in 0..3 {
        verify(json!({"challenge_token": c3, "code": wrong_totp(&secret)})).await;
    }
    err(
        &verify(json!({"challenge_token": c3, "code": wrong_totp(&secret)})).await,
        401,
        "失败次数过多，请稍后重试",
    );
    err(
        &verify(json!({"challenge_token": c3, "code": totp(&secret)})).await,
        401,
        "登录会话已失效，请重新输入密码",
    );
}

async fn set_captcha(app: &App, scenes: Value) {
    app.set_setting(
        "captcha_config",
        json!({"provider": "image", "scenes": scenes}),
    )
    .await;
}

#[tokio::test]
async fn image_captcha_on_login_and_send_code() {
    let app = App::new().await;
    err(
        &app.call("GET", "/api/v1/public/captcha/image", None, None)
            .await,
        400,
        "验证码服务不可用",
    );
    app.register("cap@b.io", PW).await;
    app.set_setting("registration_config", json!({})).await;
    set_captcha(&app, json!({"login": true, "reset_send_code": true})).await;
    let img = app
        .call("GET", "/api/v1/public/captcha/image", None, None)
        .await;
    let d = data(&img);
    assert!(d["captcha_id"].as_str().unwrap().len() >= 16);
    assert!(
        d["image_base64"]
            .as_str()
            .unwrap()
            .starts_with("data:image/png;base64,")
    );
    err(
        &post(
            &app,
            "/api/v1/auth/login",
            json!({"email": "cap@b.io", "password": PW}),
        )
        .await,
        400,
        "请先完成验证码",
    );
    err(
        &post(
            &app,
            "/api/v1/auth/login",
            json!({"email": "cap@b.io", "password": PW,
                   "captcha_payload": {"captcha_id": d["captcha_id"], "captcha_code": "nope"}}),
        )
        .await,
        400,
        "验证码错误或已失效",
    );
    err(
        &post(
            &app,
            "/api/v1/auth/send-verify-code",
            json!({"email": "cap@b.io", "purpose": "reset"}),
        )
        .await,
        400,
        "请先完成验证码",
    );
    // Admin login uses the same scene.
    err(
        &post(
            &app,
            "/api/v1/admin/login",
            json!({"username": "admin", "password": "Admin12345"}),
        )
        .await,
        400,
        "请先完成验证码",
    );
    // provider none with a scene enabled is a configuration error.
    app.set_setting(
        "captcha_config",
        json!({"provider": "none", "scenes": {"login": true}}),
    )
    .await;
    err(
        &post(
            &app,
            "/api/v1/auth/login",
            json!({"email": "cap@b.io", "password": PW}),
        )
        .await,
        500,
        "验证码配置不合法",
    );
    // Disabled scene passes without a payload.
    set_captcha(&app, json!({"login": false})).await;
    data(
        &post(
            &app,
            "/api/v1/auth/login",
            json!({"email": "cap@b.io", "password": PW}),
        )
        .await,
    );

    use sea_orm::QueryOrder;
    let reasons: Vec<String> = zs_infra::db::entity::user_login_logs::Entity::find()
        .order_by_asc(zs_infra::db::entity::user_login_logs::Column::Id)
        .all(&app.db)
        .await
        .unwrap()
        .into_iter()
        .map(|l| l.fail_reason)
        .collect();
    for r in [
        "captcha_required",
        "captcha_invalid",
        "captcha_config_invalid",
    ] {
        assert!(reasons.iter().any(|x| x == r), "missing {r}");
    }
}

// RISK-04 / RISK-06 / RISK-07: in-process login limits return HTTP 429, keys
// normalise the email and the handler still reads the whole body.
#[tokio::test]
async fn login_rate_limits() {
    let mut cfg = config();
    cfg.security.login_rate_limit.max_attempts = 3;
    cfg.security.login_rate_limit.window_seconds = 60;
    cfg.security.login_rate_limit.block_seconds = 600;
    let app = App::with(cfg).await;
    app.register("r@b.io", PW).await;
    // Successful logins count too; " R@B.io " and "r@b.io" share the counter.
    data(
        &post(
            &app,
            "/api/v1/auth/login",
            json!({"email": " R@B.io ", "password": PW}),
        )
        .await,
    );
    data(
        &post(
            &app,
            "/api/v1/auth/login",
            json!({"email": "r@b.io", "password": PW}),
        )
        .await,
    );
    err(
        &post(
            &app,
            "/api/v1/auth/login",
            json!({"email": "r@b.io", "password": "Wrong1234"}),
        )
        .await,
        401,
        "邮箱或密码错误",
    );
    let (status, body) = app
        .raw(
            "POST",
            "/api/v1/auth/login",
            Some(json!({"email": "r@b.io", "password": PW})),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    err(&body, 429, "登录尝试过多，请在 600 秒后重试");
    // Still blocked on the next attempt; other emails are independent.
    assert_eq!(
        app.raw(
            "POST",
            "/api/v1/auth/login",
            Some(json!({"email": "r@b.io", "password": PW})),
            None
        )
        .await
        .0,
        StatusCode::TOO_MANY_REQUESTS
    );
    err(
        &post(
            &app,
            "/api/v1/auth/login",
            json!({"email": "other@b.io", "password": PW}),
        )
        .await,
        401,
        "邮箱或密码错误",
    );

    // QA-A03: admin login counts failures only, per (username, IP) — the
    // bootstrap login does not count, the 3rd failure blocks.
    for _ in 0..3 {
        err(
            &post(
                &app,
                "/api/v1/admin/login",
                json!({"username": "admin", "password": "x"}),
            )
            .await,
            401,
            "用户名或密码错误",
        );
    }
    let (status, body) = app
        .raw(
            "POST",
            "/api/v1/admin/login",
            Some(json!({"username": "admin", "password": "Admin12345"})),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(body["status_code"], 429);
}
