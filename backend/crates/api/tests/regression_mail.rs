//! Regression tests for checklist items of `bugfix-lessons.md` §21 on outgoing mail:
//! NTF-02 (white-label brand isolation).

mod reseller_common;

use reseller_common::{App, data};
use serde_json::json;
use zs_infra::testkit::{MockSmtp, SmtpMessage};

async fn mail_app(smtp: &MockSmtp) -> App {
    let app = App::new().await;
    app.set_setting(
        "smtp_config",
        json!({"enabled": true, "host": "127.0.0.1", "port": smtp.port,
            "from": "noreply@mail.example", "from_name": "Main Shop Mailer",
            "use_tls": false, "use_ssl": false}),
    )
    .await;
    app.set_setting(
        "site_config",
        json!({"brand": {"site_name": "Main Shop", "site_url": "https://main.example.com/"}}),
    )
    .await;
    app
}

/// Requests a registration code on `host`; the SMTP transaction completes before the
/// response, so the mock has recorded the mail afterwards.
async fn send_code(app: &App, host: &str, email: &str) {
    app.set_setting(
        "registration_config",
        json!({"registration_enabled": true, "email_verification_enabled": true}),
    )
    .await;
    let (status, res) = app
        .raw(
            "POST",
            "/api/v1/auth/send-verify-code",
            Some(json!({"email": email, "purpose": "register"})),
            None,
            &[("host", host), ("x-lang", "en-US")],
        )
        .await;
    assert_eq!(
        (status.as_u16(), &res["status_code"]),
        (200, &json!(0)),
        "{res}"
    );
}

fn last_to(smtp: &MockSmtp, email: &str) -> SmtpMessage {
    smtp.messages()
        .into_iter()
        .rev()
        .find(|m| m.rcpt_to.iter().any(|r| r.contains(email)))
        .unwrap_or_else(|| panic!("no mail to {email}: {:?}", smtp.messages()))
}

/// NTF-02: a verification mail requested on a reseller domain carries the reseller's
/// site name (subject, footer, From name), its URL and support Reply-To; a reseller
/// without a site name falls back to its domain; the main shop keeps its own brand.
#[tokio::test]
async fn ntf_02_verify_code_mail_brand_follows_the_storefront() {
    let smtp = MockSmtp::start("").await;
    let app = mail_app(&smtp).await;

    // reseller with a configured site
    let (_, token, pid) = app.reseller("white@example.test", "0").await;
    let saved = app
        .call(
            "PUT",
            "/api/v1/reseller/site-config",
            Some(json!({"site_name": "White Label", "support": {"email": "Help <help@white.example>"}})),
            Some(&token),
        )
        .await;
    data(&saved);
    let host = app.system_domain(pid, "white").await;
    send_code(&app, &host, "new1@example.test").await;
    let m = last_to(&smtp, "new1@example.test");
    assert_eq!(
        m.header("Subject").as_deref(),
        Some("White Label - Registration Code")
    );
    let body = m.body().replace("\r\n", "\n");
    assert!(body.contains("Site: White Label"), "{body}");
    assert!(body.contains(&format!("URL: https://{host}")), "{body}");
    assert!(!body.contains("Main Shop"), "{body}");
    let from = m.header("From").unwrap();
    assert!(
        from.contains("White Label") && !from.contains("Main Shop"),
        "{from}"
    );
    assert_eq!(m.header("Reply-To").as_deref(), Some("help@white.example"));

    // reseller without a site name: its domain is the brand
    let (_, _, pid2) = app.reseller("bare@example.test", "0").await;
    let host2 = app.system_domain(pid2, "bare").await;
    send_code(&app, &host2, "new2@example.test").await;
    let m = last_to(&smtp, "new2@example.test");
    assert_eq!(
        m.header("Subject"),
        Some(format!("{host2} - Registration Code"))
    );
    assert!(m.header("From").unwrap().contains(&host2));
    assert!(!m.body().contains("Main Shop"));

    // main shop: its own brand, the SMTP sender name, no Reply-To
    send_code(&app, "localhost", "new3@example.test").await;
    let m = last_to(&smtp, "new3@example.test");
    assert_eq!(
        m.header("Subject").as_deref(),
        Some("Main Shop - Registration Code")
    );
    assert!(
        m.body().contains("URL: https://main.example.com"),
        "{}",
        m.body()
    );
    assert!(m.header("From").unwrap().contains("Main Shop Mailer"));
    assert!(m.header("Reply-To").is_none());
}
