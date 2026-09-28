//! In-memory Casbin-compatible enforcer.

use std::collections::{BTreeSet, HashMap, HashSet};

use super::{Policy, Rule};

/// Maximum role inheritance depth (Casbin's default role manager uses 10).
const MAX_ROLE_DEPTH: usize = 10;

/// Policies and role links loaded from `casbin_rule`.
#[derive(Debug, Clone, Default)]
pub struct Enforcer {
    policies: Vec<Policy>,
    links: HashMap<String, BTreeSet<String>>,
}

impl Enforcer {
    pub fn from_rules(rules: &[Rule]) -> Self {
        let mut e = Self::default();
        for r in rules {
            match r.ptype.as_str() {
                "p" => e.policies.push(Policy {
                    subject: r.v0.clone(),
                    object: r.v1.clone(),
                    action: r.v2.clone(),
                }),
                "g" => {
                    e.links
                        .entry(r.v0.clone())
                        .or_default()
                        .insert(r.v1.clone());
                }
                _ => {}
            }
        }
        e
    }

    /// Direct parents of `subject`.
    pub fn direct_roles(&self, subject: &str) -> Vec<String> {
        self.links
            .get(subject)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// `subject` plus every role reachable through links.
    pub fn subjects_of(&self, subject: &str) -> HashSet<String> {
        let mut seen = HashSet::from([subject.to_owned()]);
        let mut frontier = vec![subject.to_owned()];
        for _ in 0..MAX_ROLE_DEPTH {
            let mut next = Vec::new();
            for s in &frontier {
                for parent in self.links.get(s).into_iter().flatten() {
                    if seen.insert(parent.clone()) {
                        next.push(parent.clone());
                    }
                }
            }
            if next.is_empty() {
                break;
            }
            frontier = next;
        }
        seen
    }

    /// Evaluates the RBAC matcher.
    pub fn enforce(&self, subject: &str, object: &str, action: &str) -> bool {
        let subjects = self.subjects_of(subject);
        self.policies.iter().any(|p| {
            subjects.contains(&p.subject)
                && key_match2(object, &p.object)
                && (p.action == action || p.action == "*")
        })
    }

    /// Policies whose subject is exactly `subject`.
    pub fn policies_of(&self, subject: &str) -> Vec<Policy> {
        self.policies
            .iter()
            .filter(|p| p.subject == subject)
            .cloned()
            .collect()
    }

    /// Every distinct subject that appears as a role (`role:*`, except the anchor).
    pub fn roles(&self) -> Vec<String> {
        let mut roles: BTreeSet<String> = BTreeSet::new();
        for (child, parents) in &self.links {
            for s in std::iter::once(child).chain(parents.iter()) {
                if s.starts_with(super::ROLE_PREFIX) && s != super::ROLE_ANCHOR {
                    roles.insert(s.clone());
                }
            }
        }
        for p in &self.policies {
            if p.subject.starts_with(super::ROLE_PREFIX) && p.subject != super::ROLE_ANCHOR {
                roles.insert(p.subject.clone());
            }
        }
        roles.into_iter().collect()
    }
}

/// Casbin `keyMatch2`: `:name` matches one path segment, `*` matches anything.
pub fn key_match2(key: &str, pattern: &str) -> bool {
    let pattern = pattern.replace("/*", "/.*");
    let mut regex = String::from("^");
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            ':' => {
                while chars
                    .peek()
                    .is_some_and(|n| n.is_ascii_alphanumeric() || *n == '_')
                {
                    chars.next();
                }
                regex.push_str("[^/]+");
            }
            _ => regex.push(c),
        }
    }
    regex.push('$');
    simple_regex_match(&regex, key)
}

/// Minimal matcher for the patterns produced by [`key_match2`]:
/// literals, `.*` and `[^/]+`, anchored with `^…$`.
fn simple_regex_match(regex: &str, text: &str) -> bool {
    #[derive(Debug)]
    enum Tok {
        Lit(char),
        Any,
        Segment,
    }
    let body = regex.trim_start_matches('^').trim_end_matches('$');
    let mut toks = Vec::new();
    let mut rest = body;
    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix(".*") {
            toks.push(Tok::Any);
            rest = r;
        } else if let Some(r) = rest.strip_prefix("[^/]+") {
            toks.push(Tok::Segment);
            rest = r;
        } else {
            let mut it = rest.chars();
            if let Some(c) = it.next() {
                toks.push(Tok::Lit(c));
            }
            rest = it.as_str();
        }
    }
    let text: Vec<char> = text.chars().collect();
    fn go(toks: &[Tok], text: &[char]) -> bool {
        match toks.split_first() {
            None => text.is_empty(),
            Some((Tok::Lit(c), rest)) => text.first() == Some(c) && go(rest, &text[1..]),
            Some((Tok::Any, rest)) => (0..=text.len()).any(|i| go(rest, &text[i..])),
            Some((Tok::Segment, rest)) => {
                let max = text.iter().position(|c| *c == '/').unwrap_or(text.len());
                (1..=max).any(|i| go(rest, &text[i..]))
            }
        }
    }
    go(&toks, &text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_match2_semantics() {
        assert!(key_match2("/admin/products/12", "/admin/products/:id"));
        assert!(!key_match2("/admin/products/12/x", "/admin/products/:id"));
        assert!(key_match2("/admin/products/:id", "/admin/products/:id"));
        assert!(key_match2("/admin/anything/deep", "/admin/*"));
        assert!(!key_match2("/admin/products", "/admin/products/:id"));
        assert!(key_match2("/admin/products", "/admin/products"));
    }

    #[test]
    fn roles_are_transitive() {
        let e = Enforcer::from_rules(&[
            Rule::link("admin:2", "role:operations"),
            Rule::link("role:operations", "role:readonly_auditor"),
            Rule::link("role:operations", "role:__anchor__"),
            Rule::policy("role:readonly_auditor", "/admin/dashboard/overview", "GET"),
            Rule::policy("role:operations", "/admin/products/:id", "*"),
        ]);
        assert!(e.enforce("admin:2", "/admin/dashboard/overview", "GET"));
        assert!(e.enforce("admin:2", "/admin/products/:id", "DELETE"));
        assert!(!e.enforce("admin:2", "/admin/orders", "GET"));
        assert!(!e.enforce("admin:3", "/admin/dashboard/overview", "GET"));
        assert_eq!(e.roles(), vec!["role:operations", "role:readonly_auditor"]);
    }
}
