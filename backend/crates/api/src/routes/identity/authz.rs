//! RBAC management, permission catalog, audit logs and compliance.

use std::collections::{BTreeSet, HashMap};

use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use zs_app::identity::admin_accounts::{AdminPatch, Operator};
use zs_app::identity::compliance::Acknowledgement;
use zs_domain::Error;
use zs_domain::authz::normalize_object;
use zs_domain::identity::audit::AuthzAuditFilter;
use zs_domain::identity::compliance::KEY_SUPER_ADMIN_REQUIRED;
use zs_domain::identity::login_log::UserLoginFilter;
use zs_shared::page::Pagination;

use super::{opt_id, opt_time, page_of, parse_bool, parse_id, text};
use crate::client::Client;
use crate::extract::{Bind, BindField, BindRules, Body, Query, req};
use crate::middleware::auth::CurrentAdmin;
use crate::response::{ApiResult, Data, Paged, ok};
use crate::routes::Routes;
use crate::state::AppState;

pub(super) fn routes() -> Routes {
    Routes::new("/admin")
        .get("/authz/roles", list_roles)
        .post("/authz/roles", create_role)
        .delete("/authz/roles/{role}", delete_role)
        .get("/authz/roles/{role}/policies", role_policies)
        .post("/authz/policies", grant_policy)
        .delete("/authz/policies", revoke_policy)
        .get("/authz/admins", list_admins)
        .post("/authz/admins", create_admin)
        .put("/authz/admins/{id}", update_admin)
        .delete("/authz/admins/{id}", delete_admin)
        .get("/authz/admins/{id}/roles", admin_roles)
        .put("/authz/admins/{id}/roles", set_admin_roles)
        .get("/authz/permissions/catalog", permission_catalog)
        .get("/authz/audit-logs", audit_logs)
        .get("/user-login-logs", user_login_logs)
        .get("/compliance/status", compliance_status)
        .post("/compliance/acknowledge", compliance_acknowledge)
}

type Params = Query<HashMap<String, String>>;

async fn list_roles(State(s): State<AppState>, Query(q): Params) -> ApiResult<Data<Value>> {
    let items = s.svc.identity.admin_accounts.roles()?;
    if parse_bool(q.get("include_metadata"))? {
        ok(serde_json::to_value(items).map_err(Error::from)?)
    } else {
        // Legacy shape: plain role names.
        ok(json!(items.into_iter().map(|i| i.role).collect::<Vec<_>>()))
    }
}

#[derive(Debug, Deserialize)]
struct RoleRequest {
    #[serde(default)]
    role: String,
}

impl BindRules for RoleRequest {
    const FIELDS: &'static [BindField] = &[req("role", "Role")];
}

async fn create_role(
    State(s): State<AppState>,
    CurrentAdmin(me): CurrentAdmin,
    Client(client): Client,
    Bind(req): Bind<RoleRequest>,
) -> ApiResult<Data<Value>> {
    if req.role.is_empty() {
        return Err(Error::invalid().into());
    }
    let op = Operator {
        admin: &me,
        client: &client,
    };
    let role = s
        .svc
        .identity
        .admin_accounts
        .create_role(op, &req.role)
        .await?;
    ok(json!({ "role": role }))
}

async fn delete_role(
    State(s): State<AppState>,
    CurrentAdmin(me): CurrentAdmin,
    Client(client): Client,
    Path(role): Path<String>,
) -> ApiResult<Data<()>> {
    let op = Operator {
        admin: &me,
        client: &client,
    };
    s.svc
        .identity
        .admin_accounts
        .delete_role(op, role.trim())
        .await?;
    ok(())
}

async fn role_policies(
    State(s): State<AppState>,
    Path(role): Path<String>,
) -> ApiResult<Data<Value>> {
    let policies = s.svc.identity.admin_accounts.role_policies(role.trim())?;
    ok(serde_json::to_value(policies).map_err(Error::from)?)
}

#[derive(Debug, Deserialize)]
struct PolicyRequest {
    #[serde(default)]
    role: String,
    #[serde(default)]
    object: String,
    #[serde(default)]
    action: String,
}

impl BindRules for PolicyRequest {
    const FIELDS: &'static [BindField] = &[
        req("role", "Role"),
        req("object", "Object"),
        req("action", "Action"),
    ];
}

async fn change_policy(
    s: &AppState,
    me: &zs_app::identity::admin_auth::AdminPrincipal,
    client: &zs_app::identity::admin_auth::ClientInfo,
    req: &PolicyRequest,
    grant: bool,
) -> ApiResult<Data<()>> {
    let op = Operator { admin: me, client };
    s.svc
        .identity
        .admin_accounts
        .change_policy(op, grant, &req.role, &req.object, &req.action)
        .await?;
    ok(())
}

async fn grant_policy(
    State(s): State<AppState>,
    CurrentAdmin(me): CurrentAdmin,
    Client(client): Client,
    Bind(req): Bind<PolicyRequest>,
) -> ApiResult<Data<()>> {
    change_policy(&s, &me, &client, &req, true).await
}

async fn revoke_policy(
    State(s): State<AppState>,
    CurrentAdmin(me): CurrentAdmin,
    Client(client): Client,
    Bind(req): Bind<PolicyRequest>,
) -> ApiResult<Data<()>> {
    change_policy(&s, &me, &client, &req, false).await
}

async fn list_admins(State(s): State<AppState>) -> ApiResult<Data<Value>> {
    let items = s.svc.identity.admin_accounts.list().await?;
    ok(serde_json::to_value(items).map_err(Error::from)?)
}

#[derive(Debug, Deserialize)]
struct CreateAdminRequest {
    #[serde(default)]
    username: String,
    #[serde(default)]
    password: String,
    #[serde(default)]
    is_super: Option<bool>,
}

impl BindRules for CreateAdminRequest {
    const FIELDS: &'static [BindField] =
        &[req("username", "Username"), req("password", "Password")];
}

async fn create_admin(
    State(s): State<AppState>,
    CurrentAdmin(me): CurrentAdmin,
    Client(client): Client,
    Bind(req): Bind<CreateAdminRequest>,
) -> ApiResult<Data<Value>> {
    if req.username.is_empty() || req.password.is_empty() {
        return Err(Error::invalid().into());
    }
    let op = Operator {
        admin: &me,
        client: &client,
    };
    let admin = s
        .svc
        .identity
        .admin_accounts
        .create(op, &req.username, &req.password, req.is_super)
        .await?;
    ok(serde_json::to_value(admin).map_err(Error::from)?)
}

#[derive(Debug, Deserialize)]
struct UpdateAdminRequest {
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    password: Option<String>,
    #[serde(default)]
    is_super: Option<bool>,
}

async fn update_admin(
    State(s): State<AppState>,
    CurrentAdmin(me): CurrentAdmin,
    Client(client): Client,
    Path(id): Path<String>,
    Body(req): Body<UpdateAdminRequest>,
) -> ApiResult<Data<Value>> {
    let id = parse_id(&id, "error.admin_id_invalid")?;
    let op = Operator {
        admin: &me,
        client: &client,
    };
    let admin = s
        .svc
        .identity
        .admin_accounts
        .update(
            op,
            id,
            AdminPatch {
                username: req.username,
                password: req.password,
                is_super: req.is_super,
            },
        )
        .await?;
    ok(serde_json::to_value(admin).map_err(Error::from)?)
}

async fn delete_admin(
    State(s): State<AppState>,
    CurrentAdmin(me): CurrentAdmin,
    Client(client): Client,
    Path(id): Path<String>,
) -> ApiResult<Data<()>> {
    let id = parse_id(&id, "error.admin_id_invalid")?;
    let op = Operator {
        admin: &me,
        client: &client,
    };
    s.svc.identity.admin_accounts.delete(op, id).await?;
    ok(())
}

async fn admin_roles(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Data<Value>> {
    let id = parse_id(&id, "error.admin_id_invalid")?;
    ok(json!(s.svc.identity.admin_accounts.admin_roles(id).await?))
}

#[derive(Debug, Deserialize)]
struct SetRolesRequest {
    #[serde(default)]
    roles: Vec<String>,
}

async fn set_admin_roles(
    State(s): State<AppState>,
    CurrentAdmin(me): CurrentAdmin,
    Client(client): Client,
    Path(id): Path<String>,
    Body(req): Body<SetRolesRequest>,
) -> ApiResult<Data<()>> {
    let id = parse_id(&id, "error.admin_id_invalid")?;
    let op = Operator {
        admin: &me,
        client: &client,
    };
    s.svc
        .identity
        .admin_accounts
        .set_admin_roles(op, id, &req.roles)
        .await?;
    ok(())
}

/// One entry of the permission catalog.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct CatalogItem {
    pub module: String,
    pub method: String,
    pub object: String,
    pub permission: String,
}

/// Module of an admin object: `/admin/<module>/…` (`authz` for `/admin/authz/…`).
fn module_of(object: &str) -> String {
    let normalized = object.trim().trim_start_matches('/');
    if normalized.is_empty() {
        return "system".into();
    }
    let segments: Vec<&str> = normalized.split('/').collect();
    match segments.as_slice() {
        [only] => (*only).to_owned(),
        [first, ..] if *first != "admin" => (*first).to_owned(),
        [_, second, ..] => (*second).to_owned(),
        [] => "system".into(),
    }
}

/// Builds the catalog from the registered admin routes (sorted by module, object, method).
pub fn build_catalog(perms: &[crate::routes::Permission]) -> Vec<CatalogItem> {
    let mut seen = BTreeSet::new();
    let mut items: Vec<CatalogItem> = perms
        .iter()
        .filter(|p| p.path.starts_with("/admin/"))
        .filter(|p| p.path != "/admin/login" && p.path != "/admin/login/verify-2fa")
        .filter_map(|p| {
            let method = p.method.to_uppercase();
            let object = normalize_object(&p.path);
            let permission = format!("{method}:{object}");
            seen.insert(permission.clone()).then(|| CatalogItem {
                module: module_of(&object),
                method,
                object,
                permission,
            })
        })
        .collect();
    items.sort_by(|a, b| (&a.module, &a.object, &a.method).cmp(&(&b.module, &b.object, &b.method)));
    items
}

async fn permission_catalog(State(s): State<AppState>) -> ApiResult<Data<Vec<CatalogItem>>> {
    ok(build_catalog(&s.admin_permissions))
}

async fn audit_logs(State(s): State<AppState>, Query(q): Params) -> ApiResult<Paged<Value>> {
    let page = page_of(&q);
    let filter = AuthzAuditFilter {
        operator_admin_id: opt_id(q.get("operator_admin_id"))?,
        target_admin_id: opt_id(q.get("target_admin_id"))?,
        action: text(&q, "action"),
        role: text(&q, "role"),
        object: text(&q, "object"),
        method: text(&q, "method"),
        created_from: opt_time(q.get("created_from"))?,
        created_to: opt_time(q.get("created_to"))?,
    };
    let result = s
        .svc
        .identity
        .audit
        .list_authz(&filter, page)
        .await
        .map_err(|e| e.or_internal("error.config_fetch_failed"))?;
    let items = result
        .items
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<Vec<_>, _>>()
        .map_err(Error::from)?;
    Ok(Paged(items, Pagination::new(page, result.total)))
}

async fn user_login_logs(State(s): State<AppState>, Query(q): Params) -> ApiResult<Paged<Value>> {
    let page = page_of(&q);
    let filter = UserLoginFilter {
        user_id: opt_id(q.get("user_id"))?,
        email: text(&q, "email"),
        status: text(&q, "status"),
        fail_reason: text(&q, "fail_reason"),
        client_ip: text(&q, "client_ip"),
        created_from: opt_time(q.get("created_from"))?,
        created_to: opt_time(q.get("created_to"))?,
    };
    let result = s
        .svc
        .identity
        .audit
        .list_user_logins(&filter, page)
        .await
        .map_err(|e| e.or_internal("error.user_login_log_fetch_failed"))?;
    let items = result
        .items
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<Vec<_>, _>>()
        .map_err(Error::from)?;
    Ok(Paged(items, Pagination::new(page, result.total)))
}

async fn compliance_status(State(s): State<AppState>) -> ApiResult<Data<Value>> {
    let status = s
        .svc
        .identity
        .compliance
        .status()
        .await
        .map_err(|e| e.or_internal("error.internal"))?;
    ok(serde_json::to_value(status).map_err(Error::from)?)
}

#[derive(Debug, Deserialize)]
struct AcknowledgeRequest {
    #[serde(default)]
    segment1: String,
    #[serde(default)]
    segment2: String,
    #[serde(default)]
    segment3: String,
}

/// Original compliance `acknowledgeRequest`.
impl BindRules for AcknowledgeRequest {
    const FIELDS: &'static [BindField] = &[
        req("segment1", "Segment1"),
        req("segment2", "Segment2"),
        req("segment3", "Segment3"),
    ];
}

async fn compliance_acknowledge(
    State(s): State<AppState>,
    CurrentAdmin(me): CurrentAdmin,
    Client(client): Client,
    Bind(req): Bind<AcknowledgeRequest>,
) -> ApiResult<Data<Value>> {
    if !me.is_super {
        return Err(Error::forbidden(KEY_SUPER_ADMIN_REQUIRED).into());
    }
    if req.segment1.is_empty() || req.segment2.is_empty() || req.segment3.is_empty() {
        return Err(Error::invalid().into());
    }
    let already = s
        .svc
        .identity
        .compliance
        .acknowledge(
            &Acknowledgement {
                segment1: &req.segment1,
                segment2: &req.segment2,
                segment3: &req.segment3,
                admin_id: me.id,
                username: &me.username,
            },
            &client,
        )
        .await?;
    ok(json!({ "already_acknowledged": already }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::Permission;

    #[test]
    fn catalog_modules_and_order() {
        let perms = vec![
            Permission {
                method: "GET",
                path: "/admin/authz/roles".into(),
            },
            Permission {
                method: "DELETE",
                path: "/admin/categories/:id".into(),
            },
            Permission {
                method: "GET",
                path: "/admin/categories/:id".into(),
            },
            Permission {
                method: "GET",
                path: "/admin/categories/:id".into(),
            },
            Permission {
                method: "POST",
                path: "/admin/login".into(),
            },
        ];
        let items = build_catalog(&perms);
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].module, "authz");
        assert_eq!(items[1].permission, "DELETE:/admin/categories/:id");
        assert_eq!(items[2].method, "GET");
        assert_eq!(module_of("/admin"), "admin");
        assert_eq!(module_of("/x/y"), "x");
    }
}
