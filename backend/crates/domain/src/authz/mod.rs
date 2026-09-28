//! Role-based access control with Casbin semantics (ported from `internal/authz`).
//!
//! Model: `(g(r.sub, p.sub) || r.sub == p.sub) && keyMatch2(r.obj, p.obj) && (r.act == p.act || p.act == "*")`.
//! Subjects are `admin:{id}`; roles are `role:{name}` and are anchored to `role:__anchor__`.

mod enforcer;

use std::sync::OnceLock;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub use enforcer::{Enforcer, key_match2};

use crate::{Error, Id, Result};

/// Prefix stripped from request paths before matching.
pub const API_V1_PREFIX: &str = "/api/v1";
/// Prefix of role subjects.
pub const ROLE_PREFIX: &str = "role:";
/// Every role is linked to this anchor so empty roles still exist.
pub const ROLE_ANCHOR: &str = "role:__anchor__";

/// A permission rule.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Policy {
    pub subject: String,
    pub object: String,
    pub action: String,
}

/// A built-in role definition.
#[derive(Debug, Clone, Deserialize)]
pub struct RoleSeed {
    #[serde(rename = "Role")]
    pub role: String,
    #[serde(rename = "Inherits", default)]
    pub inherits: Option<Vec<String>>,
    #[serde(rename = "Policies")]
    pub policies: Vec<SeedPolicy>,
    #[serde(rename = "Immutable")]
    pub immutable: bool,
}

/// Object/action pair inside a [`RoleSeed`].
#[derive(Debug, Clone, Deserialize)]
pub struct SeedPolicy {
    pub object: String,
    pub action: String,
}

const BUILTIN_ROLES_JSON: &str = include_str!("builtin_roles.json");

/// The built-in role matrix (dumped from the original `BuiltinRoleSeeds()`).
pub fn builtin_roles() -> &'static [RoleSeed] {
    static SEEDS: OnceLock<Vec<RoleSeed>> = OnceLock::new();
    // Embedded at compile time and covered by a unit test.
    SEEDS.get_or_init(|| serde_json::from_str(BUILTIN_ROLES_JSON).unwrap_or_default())
}

/// Returns `admin:{id}`.
/// Routes about the caller's own account that every authenticated admin may use
/// whatever its roles (live QA I-14: a custom role without `GET:/admin/authz/me`
/// could not even finish logging in). The set equals the self-service part of the
/// built-in `readonly_auditor` role.
pub const SELF_SERVICE_ROUTES: [(&str, &str); 8] = [
    ("GET", "/admin/authz/me"),
    ("GET", "/admin/compliance/status"),
    ("GET", "/admin/2fa/status"),
    ("PUT", "/admin/password"),
    ("POST", "/admin/2fa/setup"),
    ("POST", "/admin/2fa/enable"),
    ("POST", "/admin/2fa/disable"),
    ("POST", "/admin/2fa/recovery-codes/regenerate"),
];

/// Whether `method object` (matched route pattern, with or without `/api/v1`)
/// is a [`SELF_SERVICE_ROUTES`] entry.
pub fn is_self_service(method: &str, object: &str) -> bool {
    let object = normalize_object(object);
    SELF_SERVICE_ROUTES
        .iter()
        .any(|(m, o)| m.eq_ignore_ascii_case(method) && *o == object)
}

pub fn admin_subject(id: Id) -> String {
    format!("admin:{id}")
}

/// Normalizes a role name to `role:{name}` (spaces → underscores).
pub fn normalize_role(role: &str) -> Result<String> {
    let trimmed = role.trim().replace(' ', "_");
    if trimmed.is_empty() {
        return Err(Error::bad_request("error.bad_request"));
    }
    let normalized = if trimmed.starts_with(ROLE_PREFIX) {
        trimmed
    } else {
        format!("{ROLE_PREFIX}{trimmed}")
    };
    if normalized.len() <= ROLE_PREFIX.len() {
        return Err(Error::bad_request("error.bad_request"));
    }
    Ok(normalized)
}

/// True for built-in roles managed by bootstrap (cannot be edited or deleted via the API).
pub fn is_immutable_builtin(role: &str) -> bool {
    let Ok(normalized) = normalize_role(role) else {
        return false;
    };
    builtin_roles()
        .iter()
        .any(|seed| seed.immutable && normalize_role(&seed.role).is_ok_and(|r| r == normalized))
}

/// Strips `/api/v1` and ensures a leading slash.
pub fn normalize_object(object: &str) -> String {
    let trimmed = object.trim();
    if trimmed.is_empty() {
        return "/".into();
    }
    let with_slash = if trimmed.starts_with('/') {
        trimmed.to_owned()
    } else {
        format!("/{trimmed}")
    };
    if let Some(rest) = with_slash.strip_prefix(API_V1_PREFIX) {
        if rest.is_empty() {
            return "/".into();
        }
        if rest.starts_with('/') {
            return rest.to_owned();
        }
    }
    with_slash
}

/// Upper-cases an HTTP method.
pub fn normalize_action(action: &str) -> String {
    action.trim().to_ascii_uppercase()
}

/// A raw `casbin_rule` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    /// `p` for policies, `g` for role links.
    pub ptype: String,
    pub v0: String,
    pub v1: String,
    pub v2: String,
}

impl Rule {
    pub fn policy(sub: &str, obj: &str, act: &str) -> Self {
        Self {
            ptype: "p".into(),
            v0: sub.into(),
            v1: obj.into(),
            v2: act.into(),
        }
    }

    pub fn link(child: &str, parent: &str) -> Self {
        Self {
            ptype: "g".into(),
            v0: child.into(),
            v1: parent.into(),
            v2: String::new(),
        }
    }
}

/// Persistence port for Casbin rules.
#[async_trait]
pub trait RuleRepo: Send + Sync {
    async fn load(&self) -> Result<Vec<Rule>>;
    /// Inserts the rule if absent; returns whether it was added.
    async fn add(&self, rule: &Rule) -> Result<bool>;
    /// Removes the exact rule; returns whether it existed.
    async fn remove(&self, rule: &Rule) -> Result<bool>;
    /// Removes every rule of `ptype` whose `v0` equals `v0`.
    async fn remove_by_v0(&self, ptype: &str, v0: &str) -> Result<u64>;
    /// Removes every rule of `ptype` whose `v1` equals `v1`.
    async fn remove_by_v1(&self, ptype: &str, v1: &str) -> Result<u64>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeds_parse() {
        let seeds = builtin_roles();
        assert_eq!(seeds.len(), 6);
        assert!(seeds.iter().all(|s| s.immutable));
        assert!(is_immutable_builtin("operations"));
        assert!(!is_immutable_builtin("custom"));
    }

    #[test]
    fn normalizes() {
        assert_eq!(normalize_role(" my role ").unwrap(), "role:my_role");
        assert_eq!(normalize_role("role:x").unwrap(), "role:x");
        assert!(normalize_role("role:").is_err());
        assert_eq!(
            normalize_object("/api/v1/admin/products/:id"),
            "/admin/products/:id"
        );
        assert_eq!(normalize_object("admin/x"), "/admin/x");
        assert_eq!(normalize_object("/api/v1"), "/");
        assert_eq!(normalize_action(" get "), "GET");
    }
}
