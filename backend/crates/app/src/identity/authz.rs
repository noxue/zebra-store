//! RBAC service: keeps the Casbin rules in memory and persists changes.

use std::collections::BTreeSet;
use std::sync::{Arc, RwLock};

use zs_domain::authz::{
    self, Enforcer, Policy, ROLE_ANCHOR, ROLE_PREFIX, Rule, RuleRepo, admin_subject, builtin_roles,
    normalize_action, normalize_object, normalize_role,
};
use zs_domain::{Error, Id, Result};

/// Authorization service (thread-safe, cheap to clone).
#[derive(Clone)]
pub struct AuthzService {
    repo: Arc<dyn RuleRepo>,
    enforcer: Arc<RwLock<Enforcer>>,
}

impl std::fmt::Debug for AuthzService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AuthzService")
    }
}

impl AuthzService {
    pub fn new(repo: Arc<dyn RuleRepo>) -> Self {
        Self {
            repo,
            enforcer: Arc::new(RwLock::new(Enforcer::default())),
        }
    }

    /// Reloads all rules from storage.
    pub async fn reload(&self) -> Result<()> {
        let rules = self.repo.load().await?;
        let fresh = Enforcer::from_rules(&rules);
        *self
            .enforcer
            .write()
            .map_err(|_| Error::internal_msg("authz lock poisoned"))? = fresh;
        Ok(())
    }

    fn read(&self) -> Result<std::sync::RwLockReadGuard<'_, Enforcer>> {
        self.enforcer
            .read()
            .map_err(|_| Error::internal_msg("authz lock poisoned"))
    }

    /// Checks `admin:{id}` against `object` (request path or route pattern) and HTTP `action`.
    pub fn enforce_admin(&self, admin_id: Id, object: &str, action: &str) -> Result<bool> {
        Ok(self.read()?.enforce(
            &admin_subject(admin_id),
            &normalize_object(object),
            &normalize_action(action),
        ))
    }

    /// Creates/updates built-in roles; immutable roles are reset to their seed.
    pub async fn bootstrap_builtin_roles(&self) -> Result<()> {
        self.reload().await?;
        for seed in builtin_roles() {
            let role = normalize_role(&seed.role)?;
            self.repo.add(&Rule::link(&role, ROLE_ANCHOR)).await?;
            let mut parents = BTreeSet::from([ROLE_ANCHOR.to_owned()]);
            for parent in seed.inherits.iter().flatten() {
                let parent = normalize_role(parent)?;
                self.repo.add(&Rule::link(&role, &parent)).await?;
                parents.insert(parent);
            }
            let desired: BTreeSet<(String, String)> = seed
                .policies
                .iter()
                .map(|p| (normalize_object(&p.object), normalize_action(&p.action)))
                .collect();
            for (obj, act) in &desired {
                self.repo.add(&Rule::policy(&role, obj, act)).await?;
            }
            if seed.immutable {
                let (links, policies) = {
                    let e = self.read()?;
                    (e.direct_roles(&role), e.policies_of(&role))
                };
                for link in links.iter().filter(|l| !parents.contains(*l)) {
                    self.repo.remove(&Rule::link(&role, link)).await?;
                }
                for p in policies {
                    if !desired
                        .contains(&(normalize_object(&p.object), normalize_action(&p.action)))
                    {
                        self.repo
                            .remove(&Rule::policy(&role, &p.object, &p.action))
                            .await?;
                    }
                }
            }
        }
        self.reload().await
    }

    /// Roles assigned to an administrator (sorted, anchor excluded).
    pub fn admin_roles(&self, admin_id: Id) -> Result<Vec<String>> {
        let mut roles: Vec<String> = self
            .read()?
            .direct_roles(&admin_subject(admin_id))
            .into_iter()
            .filter(|r| r.starts_with(ROLE_PREFIX) && r != ROLE_ANCHOR)
            .collect();
        roles.sort();
        Ok(roles)
    }

    /// Effective policies (direct + every assigned role), sorted by subject/object/action.
    pub fn admin_policies(&self, admin_id: Id) -> Result<Vec<Policy>> {
        let subject = admin_subject(admin_id);
        let e = self.read()?;
        let mut set: BTreeSet<Policy> = e.policies_of(&subject).into_iter().collect();
        for role in e
            .direct_roles(&subject)
            .into_iter()
            .filter(|r| r != ROLE_ANCHOR)
        {
            set.extend(e.policies_of(&role));
        }
        Ok(set.into_iter().collect())
    }

    /// Every defined role.
    pub fn roles(&self) -> Result<Vec<String>> {
        Ok(self.read()?.roles())
    }

    pub fn role_policies(&self, role: &str) -> Result<Vec<Policy>> {
        Ok(self.read()?.policies_of(&normalize_role(role)?))
    }

    pub async fn ensure_role(&self, role: &str) -> Result<String> {
        let role = normalize_role(role)?;
        if role == ROLE_ANCHOR {
            return Err(Error::invalid());
        }
        self.repo.add(&Rule::link(&role, ROLE_ANCHOR)).await?;
        self.reload().await?;
        Ok(role)
    }

    pub async fn delete_role(&self, role: &str) -> Result<()> {
        let role = normalize_role(role)?;
        if authz::is_immutable_builtin(&role) {
            return Err(Error::bad_request("error.authz_builtin_role_immutable"));
        }
        self.repo.remove_by_v0("p", &role).await?;
        self.repo.remove_by_v0("g", &role).await?;
        self.repo.remove_by_v1("g", &role).await?;
        self.reload().await
    }

    pub async fn grant(&self, role: &str, object: &str, action: &str) -> Result<()> {
        let role = self.mutable_role(role)?;
        self.repo.add(&Rule::link(&role, ROLE_ANCHOR)).await?;
        self.repo
            .add(&Rule::policy(
                &role,
                &normalize_object(object),
                &normalize_action(action),
            ))
            .await?;
        self.reload().await
    }

    pub async fn revoke(&self, role: &str, object: &str, action: &str) -> Result<()> {
        let role = self.mutable_role(role)?;
        self.repo
            .remove(&Rule::policy(
                &role,
                &normalize_object(object),
                &normalize_action(action),
            ))
            .await?;
        self.reload().await
    }

    /// Replaces the roles of an administrator.
    pub async fn set_admin_roles(&self, admin_id: Id, roles: &[String]) -> Result<()> {
        let subject = admin_subject(admin_id);
        self.repo.remove_by_v0("g", &subject).await?;
        for role in roles {
            let role = normalize_role(role)?;
            self.repo.add(&Rule::link(&role, ROLE_ANCHOR)).await?;
            self.repo.add(&Rule::link(&subject, &role)).await?;
        }
        self.reload().await
    }

    fn mutable_role(&self, role: &str) -> Result<String> {
        let role = normalize_role(role)?;
        if authz::is_immutable_builtin(&role) {
            return Err(Error::bad_request("error.authz_builtin_role_immutable"));
        }
        Ok(role)
    }
}
