//! Profile / domain state machines and domain-name validation (port of
//! `application/management.go`, RSL-03).

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use zs_shared::money::Amount;

use super::model::{
    DomainStatus, DomainType, Profile, ProfileStatus, ResellerDomain, SettlementStatus,
    VerificationStatus,
};
use super::tenant::normalize_host;
use super::{keys, profile_inactive};
use crate::{Error, Id, Result};

/// Longest DNS label accepted for a system subdomain.
const MAX_LABEL_LEN: usize = 63;

/// Reseller settings relevant to domain rules (`config.reseller`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DomainPolicy {
    pub main_hosts: Vec<String>,
    pub subdomain_base: String,
}

impl DomainPolicy {
    fn is_main_host(&self, domain: &str) -> bool {
        self.main_hosts.iter().any(|h| normalize_host(h) == domain)
    }
}

fn domain_invalid() -> Error {
    Error::bad_request(keys::DOMAIN_INVALID)
}

fn main_host_not_allowed() -> Error {
    Error::bad_request(keys::DOMAIN_MAIN_HOST)
}

fn status_invalid() -> Error {
    Error::bad_request(keys::BAD_REQUEST)
}

/// Validates a reseller-submitted custom domain (`normalizeAndValidateCustomDomain`).
pub fn validate_custom_domain(raw: &str, policy: &DomainPolicy) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.contains("://") || trimmed.contains(['/', '?', '#']) {
        return Err(domain_invalid());
    }
    let domain = normalize_host(trimmed);
    if domain.is_empty() {
        return Err(domain_invalid());
    }
    if policy.is_main_host(&domain) {
        return Err(main_host_not_allowed());
    }
    let base = normalize_host(&policy.subdomain_base);
    if !base.is_empty() && (domain == base || domain.ends_with(&format!(".{base}"))) {
        return Err(main_host_not_allowed());
    }
    Ok(domain)
}

/// `[a-z0-9-]{1,63}` not starting/ending with `-`.
pub fn is_valid_subdomain_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= MAX_LABEL_LEN
        && !label.starts_with('-')
        && !label.ends_with('-')
        && label
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Validates an admin-assigned system subdomain (`normalizeAndValidateSystemSubdomain`):
/// `hello` + base `shop.example.com` → `hello.shop.example.com`; one label only.
pub fn validate_system_subdomain(raw: &str, policy: &DomainPolicy) -> Result<String> {
    let base = normalize_host(&policy.subdomain_base);
    if base.is_empty() {
        return Err(Error::bad_request(keys::SUBDOMAIN_BASE_MISSING));
    }
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.contains("://")
        || trimmed.contains(':')
        || trimmed.contains(['/', '?', '#'])
        || trimmed.contains([' ', '\t', '\r', '\n'])
    {
        return Err(domain_invalid());
    }
    let normalized = normalize_host(trimmed);
    if normalized.is_empty() {
        return Err(domain_invalid());
    }
    let domain = if normalized.contains('.') {
        normalized
    } else {
        if !is_valid_subdomain_label(&normalized) {
            return Err(domain_invalid());
        }
        format!("{normalized}.{base}")
    };
    let suffix = format!(".{base}");
    let Some(label) = domain.strip_suffix(&suffix) else {
        return Err(domain_invalid());
    };
    if label.contains('.') || !is_valid_subdomain_label(label) {
        return Err(domain_invalid());
    }
    if policy.is_main_host(&domain) {
        return Err(main_host_not_allowed());
    }
    Ok(domain)
}

/// Markup percentages: non-negative, and `default <= max` when `max > 0`.
pub fn validate_markup(default_markup: Decimal, max_markup: Decimal) -> Result<()> {
    if default_markup < Decimal::ZERO || max_markup < Decimal::ZERO {
        return Err(status_invalid());
    }
    if max_markup > Decimal::ZERO && default_markup > max_markup {
        return Err(status_invalid());
    }
    Ok(())
}

/// Allowed profile transitions (RSL-03 whitelist).
pub fn check_profile_transition(from: ProfileStatus, to: ProfileStatus) -> Result<()> {
    use ProfileStatus::{Active, Disabled, PendingReview, Rejected};
    let allowed = match to {
        Rejected => from == PendingReview,
        Disabled => matches!(from, PendingReview | Active | Rejected),
        Active => from == Disabled,
        PendingReview => false,
    };
    if allowed {
        Ok(())
    } else {
        Err(status_invalid())
    }
}

/// Applies an admin review decision (reject / disable / restore) to `profile`.
pub fn apply_review(
    profile: &mut Profile,
    to: ProfileStatus,
    reason: &str,
    admin_id: Id,
    now: DateTime<Utc>,
) -> Result<()> {
    check_profile_transition(profile.status, to)?;
    profile.status = to;
    profile.reject_reason = if matches!(to, ProfileStatus::Rejected | ProfileStatus::Disabled) {
        reason.trim().to_owned()
    } else {
        String::new()
    };
    profile.reviewed_by = Some(admin_id);
    profile.reviewed_at = Some(now);
    Ok(())
}

/// Approval: only from `pending_review` / `rejected`.
pub fn apply_approval(
    profile: &mut Profile,
    default_markup: Decimal,
    max_markup: Decimal,
    admin_id: Id,
    now: DateTime<Utc>,
) -> Result<()> {
    if !matches!(
        profile.status,
        ProfileStatus::PendingReview | ProfileStatus::Rejected
    ) {
        return Err(status_invalid());
    }
    profile.status = ProfileStatus::Active;
    profile.reject_reason.clear();
    profile.default_markup_percent = Amount::new(default_markup);
    profile.max_markup_percent = Amount::new(max_markup);
    profile.settlement_status = SettlementStatus::Normal;
    profile.reviewed_by = Some(admin_id);
    profile.reviewed_at = Some(now);
    Ok(())
}

/// Result of a self-service application.
#[derive(Debug, Clone, PartialEq)]
pub enum ApplyOutcome {
    /// No profile yet: create a pending one.
    Create,
    /// Rejected profile re-applies (status back to pending review).
    Reapply,
    /// Pending or active: returned unchanged.
    Unchanged,
}

/// Decides what a self-service application does (`ApplyUserReseller`).
pub fn apply_outcome(existing: Option<&Profile>) -> Result<ApplyOutcome> {
    match existing.map(|p| p.status) {
        None => Ok(ApplyOutcome::Create),
        Some(ProfileStatus::Rejected) => Ok(ApplyOutcome::Reapply),
        Some(ProfileStatus::PendingReview | ProfileStatus::Active) => Ok(ApplyOutcome::Unchanged),
        Some(ProfileStatus::Disabled) => Err(profile_inactive()),
    }
}

fn has_live_primary(domains: &[ResellerDomain], exclude_id: Id) -> bool {
    domains
        .iter()
        .any(|d| d.id != exclude_id && d.is_primary && d.is_live())
}

/// Admin domain decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainAction {
    Approve,
    Disable,
    SetPrimary,
}

/// Computes the rows to write for a domain decision on `target` among the reseller's
/// `domains` (RSL-03): approve marks it verified and primary when no live primary
/// exists; disabling the primary promotes the first other live domain; only live
/// domains can become primary. Returns every changed row (target first).
pub fn plan_domain_action(
    action: DomainAction,
    target: &ResellerDomain,
    domains: &[ResellerDomain],
    now: DateTime<Utc>,
) -> Result<Vec<ResellerDomain>> {
    let mut changed = Vec::new();
    match action {
        DomainAction::Approve => {
            if !matches!(
                target.status,
                DomainStatus::PendingReview | DomainStatus::Disabled
            ) {
                return Err(status_invalid());
            }
            let mut next = target.clone();
            next.status = DomainStatus::Active;
            next.verification_status = VerificationStatus::Verified;
            next.verified_at = Some(now);
            if !has_live_primary(domains, target.id) {
                next.is_primary = true;
            }
            changed.push(next);
        }
        DomainAction::Disable => {
            if !matches!(
                target.status,
                DomainStatus::PendingReview | DomainStatus::Active
            ) {
                return Err(status_invalid());
            }
            let mut next = target.clone();
            next.status = DomainStatus::Disabled;
            next.is_primary = false;
            changed.push(next);
            if target.is_primary
                && let Some(candidate) = domains.iter().find(|d| d.id != target.id && d.is_live())
                && !candidate.is_primary
            {
                let mut promoted = candidate.clone();
                promoted.is_primary = true;
                changed.push(promoted);
            }
        }
        DomainAction::SetPrimary => {
            if !target.is_live() {
                return Err(status_invalid());
            }
            let mut next = target.clone();
            next.is_primary = true;
            if !target.is_primary {
                changed.push(next);
            }
            for d in domains.iter().filter(|d| d.id != target.id && d.is_primary) {
                let mut other = d.clone();
                other.is_primary = false;
                changed.push(other);
            }
        }
    }
    Ok(changed)
}

/// How to store an admin-assigned system subdomain.
#[derive(Debug, Clone, PartialEq)]
pub enum SystemDomainPlan {
    /// First system domain of the reseller.
    Insert { domain: String, is_primary: bool },
    /// Rename the existing system domain row.
    Update(ResellerDomain),
}

/// Plans `AssignSystemSubdomain`: the host must be free (or already this reseller's
/// system domain); the row becomes active + verified and keeps / takes primary.
pub fn plan_system_domain(
    reseller_id: Id,
    next_domain: &str,
    existing_host: Option<&ResellerDomain>,
    domains: &[ResellerDomain],
    now: DateTime<Utc>,
) -> Result<SystemDomainPlan> {
    let has_primary = domains.iter().any(|d| d.is_primary);
    let system = domains.iter().find(|d| d.kind == DomainType::Subdomain);
    if let Some(existing) = existing_host {
        if existing.reseller_id != reseller_id {
            return Err(Error::bad_request(keys::DOMAIN_CONFLICT));
        }
        if system.is_some_and(|s| s.id != existing.id) {
            return Err(Error::bad_request(keys::DOMAIN_CONFLICT));
        }
    }
    let should_be_primary = !has_primary || system.is_some_and(|s| s.is_primary);
    Ok(match system {
        None => SystemDomainPlan::Insert {
            domain: next_domain.to_owned(),
            is_primary: should_be_primary,
        },
        Some(s) => {
            let mut row = s.clone();
            row.domain = next_domain.to_owned();
            row.kind = DomainType::Subdomain;
            row.verification_token.clear();
            row.verification_status = VerificationStatus::Verified;
            row.status = DomainStatus::Active;
            row.is_primary = should_be_primary;
            row.verified_at = Some(now);
            SystemDomainPlan::Update(row)
        }
    })
}

/// `RequireActiveProfile`: active and settlement not frozen.
pub fn require_settleable(profile: &Profile) -> Result<()> {
    if !profile.is_active() {
        return Err(profile_inactive());
    }
    if profile.settlement_status != SettlementStatus::Normal {
        return Err(Error::bad_request(keys::SETTLEMENT_UNAVAILABLE));
    }
    Ok(())
}

/// Reason codes of `withdraw_disabled_reason` (RSL-02).
pub const WITHDRAW_DISABLED_PROFILE_INACTIVE: &str = "profile_inactive";
pub const WITHDRAW_DISABLED_SETTLEMENT_UNAVAILABLE: &str = "settlement_unavailable";

/// `WithdrawAvailability`: whether the dashboard enables the withdraw button.
pub fn withdraw_availability(profile: &Profile) -> (bool, &'static str) {
    if !profile.is_active() {
        return (false, WITHDRAW_DISABLED_PROFILE_INACTIVE);
    }
    if profile.settlement_status != SettlementStatus::Normal {
        return (false, WITHDRAW_DISABLED_SETTLEMENT_UNAVAILABLE);
    }
    (true, "")
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;

    /// Decimal literal for tests.
    pub fn d(s: &str) -> Decimal {
        s.parse().unwrap_or_default()
    }

    pub fn amt(s: &str) -> Amount {
        Amount::new(d(s))
    }

    pub fn at(sec: i64) -> DateTime<Utc> {
        DateTime::<Utc>::from_timestamp(1_780_000_000 + sec, 0).unwrap_or_default()
    }

    pub fn profile(status: ProfileStatus) -> Profile {
        Profile {
            id: 1,
            user_id: 9,
            status,
            apply_reason: String::new(),
            reject_reason: String::new(),
            default_markup_percent: Amount::ZERO,
            max_markup_percent: Amount::ZERO,
            settlement_status: SettlementStatus::Normal,
            reviewed_by: None,
            reviewed_at: None,
            created_at: at(0),
            updated_at: at(0),
            user: None,
        }
    }

    pub fn domain(id: Id, status: DomainStatus, verified: bool, primary: bool) -> ResellerDomain {
        ResellerDomain {
            id,
            reseller_id: 1,
            domain: format!("d{id}.example.test"),
            kind: DomainType::Custom,
            verification_token: String::new(),
            verification_status: if verified {
                VerificationStatus::Verified
            } else {
                VerificationStatus::Pending
            },
            status,
            is_primary: primary,
            verified_at: None,
            created_at: at(0),
            updated_at: at(0),
            profile: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;

    fn policy() -> DomainPolicy {
        DomainPolicy {
            main_hosts: vec!["localhost".into(), "Main.Example.com".into()],
            subdomain_base: "shop.example.com".into(),
        }
    }

    // RSL-03: system subdomain rules.
    #[test]
    fn rsl03_system_subdomain() {
        let p = policy();
        assert_eq!(
            validate_system_subdomain("hello", &p).unwrap_or_default(),
            "hello.shop.example.com"
        );
        assert_eq!(
            validate_system_subdomain("Hello.Shop.Example.com", &p).unwrap_or_default(),
            "hello.shop.example.com"
        );
        for bad in [
            "a.b",
            "-x",
            "x-",
            "a b",
            "https://x",
            "x:80",
            "",
            "shop.example.com",
            "a.b.shop.example.com",
            "under_score",
        ] {
            let err = validate_system_subdomain(bad, &p).unwrap_err();
            assert_eq!(err.key(), keys::DOMAIN_INVALID, "{bad}");
        }
        assert!(validate_system_subdomain(&"a".repeat(63), &p).is_ok());
        assert!(validate_system_subdomain(&"a".repeat(64), &p).is_err());
        let missing = DomainPolicy::default();
        assert_eq!(
            validate_system_subdomain("hello", &missing)
                .unwrap_err()
                .key(),
            keys::SUBDOMAIN_BASE_MISSING
        );
    }

    // RSL-03: custom domains cannot be main hosts or under the subdomain base.
    #[test]
    fn rsl03_custom_domain() {
        let p = policy();
        assert_eq!(
            validate_custom_domain(" Shop.Custom.test. ", &p).unwrap_or_default(),
            "shop.custom.test"
        );
        assert_eq!(
            validate_custom_domain("main.example.com", &p)
                .unwrap_err()
                .key(),
            keys::DOMAIN_MAIN_HOST
        );
        assert_eq!(
            validate_custom_domain("shop.example.com", &p)
                .unwrap_err()
                .key(),
            keys::DOMAIN_MAIN_HOST
        );
        assert_eq!(
            validate_custom_domain("x.shop.example.com", &p)
                .unwrap_err()
                .key(),
            keys::DOMAIN_MAIN_HOST
        );
        for bad in ["https://a.test", "a.test/x", "a.test?x", "a.test#x", "  "] {
            assert_eq!(
                validate_custom_domain(bad, &p).unwrap_err().key(),
                keys::DOMAIN_INVALID,
                "{bad}"
            );
        }
    }

    #[test]
    fn rsl03_markup_validation() {
        assert!(validate_markup(d("10"), d("50")).is_ok());
        assert!(validate_markup(d("10"), d("0")).is_ok());
        assert!(validate_markup(d("60"), d("50")).is_err());
        assert!(validate_markup(d("-1"), d("0")).is_err());
        assert!(validate_markup(d("0"), d("-1")).is_err());
    }

    // RSL-03: profile state machine whitelist.
    #[test]
    fn rsl03_profile_transitions() {
        use ProfileStatus::*;
        assert!(check_profile_transition(PendingReview, Rejected).is_ok());
        assert!(check_profile_transition(Active, Rejected).is_err());
        for from in [PendingReview, Active, Rejected] {
            assert!(check_profile_transition(from, Disabled).is_ok());
        }
        assert!(check_profile_transition(Disabled, Disabled).is_err());
        assert!(check_profile_transition(Disabled, Active).is_ok());
        assert!(check_profile_transition(Rejected, Active).is_err());

        let mut p = profile(Active);
        apply_review(&mut p, Disabled, " spam ", 7, at(5)).unwrap_or_default();
        assert_eq!(
            (p.status, p.reject_reason.as_str(), p.reviewed_by),
            (Disabled, "spam", Some(7))
        );
        apply_review(&mut p, Active, "ignored", 7, at(6)).unwrap_or_default();
        assert_eq!(p.reject_reason, "");

        let mut p = profile(Active);
        assert!(apply_approval(&mut p, d("0"), d("0"), 1, at(0)).is_err());
        let mut p = profile(Rejected);
        p.reject_reason = "x".into();
        apply_approval(&mut p, d("10"), d("50"), 1, at(0)).unwrap_or_default();
        assert_eq!(p.status, Active);
        assert_eq!(p.max_markup_percent.to_string(), "50.00");
        assert!(p.reject_reason.is_empty());
    }

    #[test]
    fn apply_outcomes() {
        assert_eq!(apply_outcome(None).ok(), Some(ApplyOutcome::Create));
        assert_eq!(
            apply_outcome(Some(&profile(ProfileStatus::Rejected))).ok(),
            Some(ApplyOutcome::Reapply)
        );
        assert_eq!(
            apply_outcome(Some(&profile(ProfileStatus::Active))).ok(),
            Some(ApplyOutcome::Unchanged)
        );
        assert_eq!(
            apply_outcome(Some(&profile(ProfileStatus::Disabled)))
                .unwrap_err()
                .key(),
            keys::PROFILE_INACTIVE
        );
    }

    // RSL-03: disabling the primary promotes another live domain.
    #[test]
    fn rsl03_disable_primary_promotes_next_live_domain() {
        let a = domain(1, DomainStatus::Active, true, true);
        let b = domain(2, DomainStatus::PendingReview, false, false);
        let c = domain(3, DomainStatus::Active, true, false);
        let all = vec![a.clone(), b, c];
        let changes =
            plan_domain_action(DomainAction::Disable, &a, &all, at(1)).unwrap_or_default();
        assert_eq!(changes.len(), 2);
        assert_eq!(
            (changes[0].id, changes[0].status, changes[0].is_primary),
            (1, DomainStatus::Disabled, false)
        );
        assert_eq!((changes[1].id, changes[1].is_primary), (3, true));
        // disabled → disabled is not allowed
        let disabled = domain(4, DomainStatus::Disabled, true, false);
        assert!(plan_domain_action(DomainAction::Disable, &disabled, &all, at(1)).is_err());
    }

    #[test]
    fn rsl03_approve_and_set_primary() {
        let pending = domain(2, DomainStatus::PendingReview, false, false);
        let live_primary = domain(1, DomainStatus::Active, true, true);
        let changes = plan_domain_action(
            DomainAction::Approve,
            &pending,
            &[live_primary.clone(), pending.clone()],
            at(1),
        )
        .unwrap_or_default();
        assert_eq!(changes.len(), 1);
        assert!(changes[0].is_live() && !changes[0].is_primary);
        let changes = plan_domain_action(
            DomainAction::Approve,
            &pending,
            std::slice::from_ref(&pending),
            at(1),
        )
        .unwrap_or_default();
        assert!(changes[0].is_primary);
        assert!(plan_domain_action(DomainAction::Approve, &live_primary, &[], at(1)).is_err());

        // only live domains can become primary
        assert!(plan_domain_action(DomainAction::SetPrimary, &pending, &[], at(1)).is_err());
        let other = domain(3, DomainStatus::Active, true, false);
        let changes = plan_domain_action(
            DomainAction::SetPrimary,
            &other,
            &[live_primary.clone(), other.clone()],
            at(1),
        )
        .unwrap_or_default();
        assert_eq!(
            changes
                .iter()
                .map(|d| (d.id, d.is_primary))
                .collect::<Vec<_>>(),
            vec![(3, true), (1, false)]
        );
    }

    #[test]
    fn system_domain_plan() {
        let custom = domain(1, DomainStatus::Active, true, true);
        let plan = plan_system_domain(1, "x.base.test", None, std::slice::from_ref(&custom), at(0))
            .unwrap_or(SystemDomainPlan::Insert {
                domain: String::new(),
                is_primary: true,
            });
        assert_eq!(
            plan,
            SystemDomainPlan::Insert {
                domain: "x.base.test".into(),
                is_primary: false
            }
        );
        let mut foreign = domain(9, DomainStatus::Active, true, false);
        foreign.reseller_id = 2;
        assert_eq!(
            plan_system_domain(1, "x.base.test", Some(&foreign), &[], at(0))
                .unwrap_err()
                .key(),
            keys::DOMAIN_CONFLICT
        );
        let mut sys = domain(5, DomainStatus::Disabled, false, false);
        sys.kind = DomainType::Subdomain;
        match plan_system_domain(1, "y.base.test", None, &[sys], at(0)) {
            Ok(SystemDomainPlan::Update(row)) => {
                assert_eq!(row.domain, "y.base.test");
                assert!(row.is_live() && row.is_primary);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    // RSL-02: withdraw availability reasons.
    #[test]
    fn rsl02_withdraw_availability() {
        assert_eq!(
            withdraw_availability(&profile(ProfileStatus::Active)),
            (true, "")
        );
        assert_eq!(
            withdraw_availability(&profile(ProfileStatus::Disabled)),
            (false, WITHDRAW_DISABLED_PROFILE_INACTIVE)
        );
        let mut frozen = profile(ProfileStatus::Active);
        frozen.settlement_status = SettlementStatus::Frozen;
        assert_eq!(
            withdraw_availability(&frozen),
            (false, WITHDRAW_DISABLED_SETTLEMENT_UNAVAILABLE)
        );
        assert_eq!(
            require_settleable(&frozen).unwrap_err().key(),
            keys::SETTLEMENT_UNAVAILABLE
        );
        assert_eq!(
            require_settleable(&profile(ProfileStatus::Rejected))
                .unwrap_err()
                .key(),
            keys::PROFILE_INACTIVE
        );
    }
}
