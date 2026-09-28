//! RBAC management: roles, policies, administrators, permission catalog,
//! audit trail and bootstrap rules.

mod identity_common;

use identity_common::{App, config, data, err};
use sea_orm::{ActiveModelTrait, Set};
use serde_json::json;
use zs_domain::authz::{builtin_roles, key_match2, normalize_action, normalize_object};

#[tokio::test]
async fn roles_and_policies() {
    let app = App::new().await;
    let legacy = app
        .admin_call("GET", "/api/v1/admin/authz/roles", None)
        .await;
    let roles = data(&legacy).as_array().unwrap().clone();
    assert!(roles.contains(&json!("role:operations")));
    assert!(roles.iter().all(|r| r.is_string()));
    let meta = app
        .admin_call(
            "GET",
            "/api/v1/admin/authz/roles?include_metadata=true",
            None,
        )
        .await;
    assert!(
        data(&meta)
            .as_array()
            .unwrap()
            .contains(&json!({"role": "role:operations", "immutable": true}))
    );
    err(
        &app.admin_call(
            "GET",
            "/api/v1/admin/authz/roles?include_metadata=maybe",
            None,
        )
        .await,
        400,
        "请求参数错误",
    );

    let created = app
        .admin_call(
            "POST",
            "/api/v1/admin/authz/roles",
            Some(json!({"role": "my role"})),
        )
        .await;
    assert_eq!(data(&created), &json!({"role": "role:my_role"}));
    err(
        &app.admin_call(
            "POST",
            "/api/v1/admin/authz/roles",
            Some(json!({"role": ""})),
        )
        .await,
        400,
        "Role: 不能为空",
    );
    let meta = app
        .admin_call("GET", "/api/v1/admin/authz/roles?include_metadata=1", None)
        .await;
    assert!(
        data(&meta)
            .as_array()
            .unwrap()
            .contains(&json!({"role": "role:my_role", "immutable": false}))
    );

    let grant =
        json!({"role": "role:my_role", "object": "/api/v1/admin/categories/:id", "action": "get"});
    data(
        &app.admin_call("POST", "/api/v1/admin/authz/policies", Some(grant.clone()))
            .await,
    );
    let policies = app
        .admin_call(
            "GET",
            "/api/v1/admin/authz/roles/role:my_role/policies",
            None,
        )
        .await;
    assert_eq!(
        data(&policies),
        &json!([{"subject": "role:my_role", "object": "/admin/categories/:id", "action": "GET"}])
    );
    data(
        &app.admin_call("DELETE", "/api/v1/admin/authz/policies", Some(grant))
            .await,
    );
    let policies = app
        .admin_call("GET", "/api/v1/admin/authz/roles/my_role/policies", None)
        .await;
    assert_eq!(data(&policies), &json!([]));

    // ADM-02: built-in roles are immutable through the API.
    let builtin = json!({"role": "readonly_auditor", "object": "/admin/*", "action": "GET"});
    err(
        &app.admin_call(
            "POST",
            "/api/v1/admin/authz/policies",
            Some(builtin.clone()),
        )
        .await,
        400,
        "系统内置角色由程序托管，不允许修改权限或删除",
    );
    err(
        &app.admin_call("DELETE", "/api/v1/admin/authz/policies", Some(builtin))
            .await,
        400,
        "系统内置角色由程序托管，不允许修改权限或删除",
    );
    err(
        &app.admin_call("DELETE", "/api/v1/admin/authz/roles/operations", None)
            .await,
        400,
        "系统内置角色由程序托管，不允许修改权限或删除",
    );
    data(
        &app.admin_call("DELETE", "/api/v1/admin/authz/roles/role:my_role", None)
            .await,
    );
    let legacy = app
        .admin_call("GET", "/api/v1/admin/authz/roles", None)
        .await;
    assert!(
        !data(&legacy)
            .as_array()
            .unwrap()
            .contains(&json!("role:my_role"))
    );

    // Every change is audited.
    let logs = app
        .admin_call(
            "GET",
            "/api/v1/admin/authz/audit-logs?page=1&page_size=50",
            None,
        )
        .await;
    let items = data(&logs).as_array().unwrap();
    let actions: Vec<&str> = items
        .iter()
        .map(|i| i["action"].as_str().unwrap())
        .collect();
    assert_eq!(
        actions,
        vec![
            "role_delete",
            "policy_revoke",
            "policy_grant",
            "role_create"
        ]
    );
    let grant_log = &items[2];
    assert_eq!(grant_log["operator_username"], "admin");
    assert_eq!(grant_log["method"], "GET");
    assert_eq!(grant_log["detail"]["method"], "GET");
    assert!(grant_log.get("target_admin_id").is_none());
    assert_eq!(logs["pagination"]["total"], 4);
    let filtered = app
        .admin_call(
            "GET",
            "/api/v1/admin/authz/audit-logs?action=role_create",
            None,
        )
        .await;
    assert_eq!(data(&filtered).as_array().unwrap().len(), 1);
    err(
        &app.admin_call(
            "GET",
            "/api/v1/admin/authz/audit-logs?operator_admin_id=x",
            None,
        )
        .await,
        400,
        "请求参数错误",
    );
    err(
        &app.admin_call(
            "GET",
            "/api/v1/admin/authz/audit-logs?created_from=yesterday",
            None,
        )
        .await,
        400,
        "请求参数错误",
    );
}

// ADM-02: stale policies on built-in roles are removed at start-up; auditors cannot write.
#[tokio::test]
async fn bootstrap_resets_builtin_roles() {
    let app = App::new().await;
    zs_infra::db::entity::casbin_rule::ActiveModel {
        ptype: Set("p".into()),
        v0: Set("role:readonly_auditor".into()),
        v1: Set("/admin/*".into()),
        v2: Set("GET".into()),
        v3: Set(String::new()),
        v4: Set(String::new()),
        v5: Set(String::new()),
        ..Default::default()
    }
    .insert(&app.db)
    .await
    .unwrap();
    app.services
        .identity
        .authz
        .bootstrap_builtin_roles()
        .await
        .unwrap();
    let policies = app
        .admin_call(
            "GET",
            "/api/v1/admin/authz/roles/readonly_auditor/policies",
            None,
        )
        .await;
    assert!(
        !data(&policies)
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["object"] == "/admin/*")
    );
    let (_, auditor) = app
        .create_admin("auditor", "Audit1234", &["readonly_auditor"])
        .await;
    let denied = app
        .call("GET", "/api/v1/admin/authz/admins", None, Some(&auditor))
        .await;
    err(&denied, 403, "无权限访问");
    data(
        &app.call("GET", "/api/v1/admin/2fa/status", None, Some(&auditor))
            .await,
    );
}

#[tokio::test]
async fn admin_accounts_crud() {
    let app = App::new().await;
    let weak = app
        .admin_call(
            "POST",
            "/api/v1/admin/authz/admins",
            Some(json!({"username": "bob", "password": "short"})),
        )
        .await;
    err(&weak, 400, "密码长度至少 8 位");
    err(
        &app.admin_call(
            "POST",
            "/api/v1/admin/authz/admins",
            Some(json!({"username": "b b", "password": "Bob12345"})),
        )
        .await,
        400,
        "管理员账号格式不合法",
    );
    err(
        &app.admin_call(
            "POST",
            "/api/v1/admin/authz/admins",
            Some(json!({"username": "bob", "password": "   "})),
        )
        .await,
        400,
        "密码强度不足",
    );
    err(
        &app.admin_call(
            "POST",
            "/api/v1/admin/authz/admins",
            Some(json!({"username": "admin", "password": "Bob12345"})),
        )
        .await,
        400,
        "管理员账号已存在",
    );
    let created = app
        .admin_call(
            "POST",
            "/api/v1/admin/authz/admins",
            Some(json!({"username": "bob", "password": "Bob12345"})),
        )
        .await;
    let bob = data(&created);
    let bob_id = bob["id"].as_i64().unwrap();
    assert_eq!(bob["username"], "bob");
    assert_eq!(bob["is_super"], false);
    assert!(bob.get("password_hash").is_none());
    assert!(bob.get("totp_enabled_at").is_none());

    data(
        &app.admin_call(
            "PUT",
            &format!("/api/v1/admin/authz/admins/{bob_id}/roles"),
            Some(json!({"roles": ["operations", "support"]})),
        )
        .await,
    );
    let roles = app
        .admin_call(
            "GET",
            &format!("/api/v1/admin/authz/admins/{bob_id}/roles"),
            None,
        )
        .await;
    assert_eq!(data(&roles), &json!(["role:operations", "role:support"]));
    err(
        &app.admin_call(
            "PUT",
            "/api/v1/admin/authz/admins/999/roles",
            Some(json!({"roles": []})),
        )
        .await,
        400,
        "无效的管理员ID",
    );
    err(
        &app.admin_call("GET", "/api/v1/admin/authz/admins/zero/roles", None)
            .await,
        400,
        "无效的管理员ID",
    );

    let list = app
        .admin_call("GET", "/api/v1/admin/authz/admins", None)
        .await;
    let rows = data(&list).as_array().unwrap();
    let row = rows.iter().find(|r| r["username"] == "bob").unwrap();
    assert_eq!(row["roles"], json!(["role:operations", "role:support"]));
    assert_eq!(row["totp_enabled"], false);
    assert!(row["totp_enabled_at"].is_null());
    assert!(row["last_login_at"].is_null());
    assert!(row["created_at"].is_string());

    let login = app
        .call(
            "POST",
            "/api/v1/admin/login",
            Some(json!({"username": "bob", "password": "Bob12345"})),
            None,
        )
        .await;
    let bob_token = data(&login)["token"].as_str().unwrap().to_owned();

    err(
        &app.admin_call(
            "PUT",
            &format!("/api/v1/admin/authz/admins/{bob_id}"),
            Some(json!({})),
        )
        .await,
        400,
        "请求参数错误",
    );
    err(
        &app.admin_call(
            "PUT",
            &format!("/api/v1/admin/authz/admins/{bob_id}"),
            Some(json!({"username": "admin"})),
        )
        .await,
        400,
        "管理员账号已存在",
    );
    let updated = app
        .admin_call(
            "PUT",
            &format!("/api/v1/admin/authz/admins/{bob_id}"),
            Some(json!({"username": "bobby", "password": "Bobby1234", "is_super": true})),
        )
        .await;
    assert_eq!(data(&updated)["username"], "bobby");
    assert_eq!(data(&updated)["is_super"], true);
    // A password change revokes the account's sessions.
    assert_eq!(
        app.call("GET", "/api/v1/admin/authz/me", None, Some(&bob_token))
            .await["status_code"],
        401
    );
    let audit = app
        .admin_call(
            "GET",
            "/api/v1/admin/authz/audit-logs?action=admin_update",
            None,
        )
        .await;
    let entry = &data(&audit)[0];
    assert_eq!(entry["target_admin_id"], bob_id);
    assert_eq!(
        entry["detail"]["updated_fields"],
        json!(["is_super", "password", "username"])
    );

    // The protected default admin stays super.
    let me = data(&app.admin_call("GET", "/api/v1/admin/authz/me", None).await)["admin_id"]
        .as_i64()
        .unwrap();
    let still = app
        .admin_call(
            "PUT",
            &format!("/api/v1/admin/authz/admins/{me}"),
            Some(json!({"is_super": false, "password": "Admin12345"})),
        )
        .await;
    assert_eq!(data(&still)["is_super"], true);
    let admin_token = data(
        &app.call(
            "POST",
            "/api/v1/admin/login",
            Some(json!({"username": "admin", "password": "Admin12345"})),
            None,
        )
        .await,
    )["token"]
        .as_str()
        .unwrap()
        .to_owned();

    let del = |uri: String| {
        let app = &app;
        let token = admin_token.clone();
        async move { app.call("DELETE", &uri, None, Some(&token)).await }
    };
    err(
        &del(format!("/api/v1/admin/authz/admins/{me}")).await,
        400,
        "不允许删除当前登录管理员",
    );
    err(
        &del("/api/v1/admin/authz/admins/999".into()).await,
        400,
        "无效的管理员ID",
    );
    data(&del(format!("/api/v1/admin/authz/admins/{bob_id}")).await);
    let list = app
        .call(
            "GET",
            "/api/v1/admin/authz/admins",
            None,
            Some(&admin_token),
        )
        .await;
    assert_eq!(data(&list).as_array().unwrap().len(), 1);
    let roles = app
        .call(
            "GET",
            &format!("/api/v1/admin/authz/admins/{bob_id}/roles"),
            None,
            Some(&admin_token),
        )
        .await;
    assert_eq!(data(&roles), &json!([]));
}

#[tokio::test]
async fn protected_admin_cannot_be_deleted() {
    let mut cfg = config();
    cfg.bootstrap.default_admin_username = "root".into();
    let app = App::with(cfg).await;
    let created = app
        .admin_call(
            "POST",
            "/api/v1/admin/authz/admins",
            Some(json!({"username": "Admin", "password": "Admin12345"})),
        )
        .await;
    // The protected username is always super.
    assert_eq!(data(&created)["is_super"], true);
    let admin_id = data(&created)["id"].as_i64().unwrap();
    err(
        &app.admin_call(
            "DELETE",
            &format!("/api/v1/admin/authz/admins/{admin_id}"),
            None,
        )
        .await,
        400,
        "默认超级管理员不允许删除",
    );
    // Log in as "Admin" and try to delete root while root is the only other admin.
    let t = data(
        &app.call(
            "POST",
            "/api/v1/admin/login",
            Some(json!({"username": "Admin", "password": "Admin12345"})),
            None,
        )
        .await,
    )["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let root_id = data(&app.admin_call("GET", "/api/v1/admin/authz/me", None).await)["admin_id"]
        .as_i64()
        .unwrap();
    data(
        &app.call(
            "DELETE",
            &format!("/api/v1/admin/authz/admins/{root_id}"),
            None,
            Some(&t),
        )
        .await,
    );
}

// ADM-03: the configured bootstrap administrator is always super.
#[tokio::test]
async fn bootstrap_admin_is_super() {
    let mut cfg = config();
    cfg.bootstrap.default_admin_username = "root".into();
    let app = App::with(cfg).await;
    assert!(!app.admin.is_empty());
    let me = app.admin_call("GET", "/api/v1/admin/authz/me", None).await;
    assert_eq!(data(&me)["is_super"], true);
    data(
        &app.admin_call("GET", "/api/v1/admin/authz/admins", None)
            .await,
    );

    // Existing root without the flag is repaired on the next start.
    let auth = &app.services.identity.admin_auth;
    let mut root = auth.repo().get_by_username("root").await.unwrap().unwrap();
    root.is_super = false;
    auth.repo().save(&root).await.unwrap();
    auth.bootstrap("root", "whatever", false).await.unwrap();
    assert!(
        auth.repo()
            .get_by_username("root")
            .await
            .unwrap()
            .unwrap()
            .is_super
    );

    // Empty username → "admin"; release mode without password → nothing created.
    let mut cfg = config();
    cfg.bootstrap.default_admin_username = String::new();
    let app = App::with(cfg).await;
    assert!(
        app.services
            .identity
            .admin_auth
            .repo()
            .get_by_username("admin")
            .await
            .unwrap()
            .unwrap()
            .is_super
    );
    let mut cfg = config();
    cfg.server.mode = "release".into();
    cfg.bootstrap.default_admin_password = String::new();
    let app = App::with(cfg).await;
    assert_eq!(
        app.services
            .identity
            .admin_auth
            .repo()
            .count()
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn permission_catalog_lists_admin_routes() {
    let app = App::new().await;
    let res = app
        .admin_call("GET", "/api/v1/admin/authz/permissions/catalog", None)
        .await;
    let items = data(&res).as_array().unwrap();
    assert!(items.contains(&json!({
        "module": "authz",
        "method": "GET",
        "object": "/admin/authz/roles",
        "permission": "GET:/admin/authz/roles",
    })));
    assert!(items.contains(&json!({
        "module": "users",
        "method": "DELETE",
        "object": "/admin/users/:id/2fa",
        "permission": "DELETE:/admin/users/:id/2fa",
    })));
    assert!(!items.iter().any(|i| i["object"] == "/admin/login"));
    let modules: Vec<&str> = items
        .iter()
        .map(|i| i["module"].as_str().unwrap())
        .collect();
    let mut sorted = modules.clone();
    sorted.sort_unstable();
    assert_eq!(modules, sorted);
}

// ADM-06: every registered admin route is granted by at least one built-in role.
#[tokio::test]
async fn builtin_roles_cover_every_admin_route() {
    let app = App::new().await;
    let res = app
        .admin_call("GET", "/api/v1/admin/authz/permissions/catalog", None)
        .await;
    let seeds = builtin_roles();
    let uncovered: Vec<String> = data(&res)
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| {
            let object = i["object"].as_str().unwrap();
            let method = i["method"].as_str().unwrap();
            !seeds.iter().any(|seed| {
                seed.policies.iter().any(|p| {
                    key_match2(object, &normalize_object(&p.object))
                        && (normalize_action(&p.action) == method || p.action == "*")
                })
            })
        })
        .map(|i| i["permission"].as_str().unwrap().to_owned())
        .collect();
    assert!(
        uncovered.is_empty(),
        "uncovered admin routes: {uncovered:?}"
    );
}

// QA-A14 (live QA I-14, original gap): a custom role granted only one page can
// still load its own identity (`/admin/authz/me`) and other self-service routes,
// while everything else stays forbidden.
#[tokio::test]
async fn qa_a14_self_service_routes_need_no_grant() {
    let app = App::new().await;
    data(
        &app.admin_call(
            "POST",
            "/api/v1/admin/authz/roles",
            Some(json!({"role": "orders_viewer"})),
        )
        .await,
    );
    data(
        &app.admin_call(
            "POST",
            "/api/v1/admin/authz/policies",
            Some(json!({"role": "role:orders_viewer", "object": "/admin/orders", "action": "GET"})),
        )
        .await,
    );
    let (_, token) = app
        .create_admin("viewer", "Viewer1234", &["role:orders_viewer"])
        .await;
    let me = app
        .call("GET", "/api/v1/admin/authz/me", None, Some(&token))
        .await;
    assert_eq!(data(&me)["is_super"], false, "{me}");
    assert_eq!(data(&me)["roles"], json!(["role:orders_viewer"]), "{me}");
    data(
        &app.call("GET", "/api/v1/admin/2fa/status", None, Some(&token))
            .await,
    );
    data(
        &app.call("GET", "/api/v1/admin/orders", None, Some(&token))
            .await,
    );
    err(
        &app.call("GET", "/api/v1/admin/authz/admins", None, Some(&token))
            .await,
        403,
        "无权限访问",
    );
    err(
        &app.call("GET", "/api/v1/admin/settings", None, Some(&token))
            .await,
        403,
        "无权限访问",
    );
}
