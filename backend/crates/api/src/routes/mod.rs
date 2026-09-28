//! Route tables, grouped like the original (`public`, `admin`, …).

pub mod affiliate;
pub mod catalog;
pub mod content;
pub mod dashboard;
pub mod identity;
pub mod integration;
pub mod marketing;
pub mod notify;
pub mod order;
pub mod payment;
pub mod reseller;
pub mod wallet;

use axum::Router;
use axum::handler::Handler;
use axum::routing::{MethodRouter, delete, get, patch, post, put};

use crate::state::AppState;

/// Every group's routes, bucketed by realm. Paths inside each bucket are
/// relative to the bucket's mount point:
///
/// | bucket | mount | middleware |
/// |---|---|---|
/// | `public` | `/api/v1/public` | – |
/// | `auth` | `/api/v1/auth` | – |
/// | `guest` | `/api/v1/guest` | – (guest credentials checked per handler) |
/// | `user` | `/api/v1` | user JWT |
/// | `user_open` | `/api/v1` | – (e.g. payment callbacks) |
/// | `admin_open` | `/api/v1/admin` | – |
/// | `admin` | `/api/v1/admin` | admin JWT + RBAC |
/// | `channel` | `/api/v1/channel` | channel HMAC (group-provided) |
/// | `upstream` | `/api/v1/upstream` | upstream HMAC (group-provided) |
/// | `root` | `/` | – (sitemap.xml, robots.txt, provider-compat `/shared/*`, `/plugin/open-api/*`) |
#[derive(Debug)]
pub struct RouteSet {
    pub public: Routes,
    pub auth: Routes,
    pub guest: Routes,
    pub user: Routes,
    pub user_open: Routes,
    pub admin_open: Routes,
    pub admin: Routes,
    pub channel: Routes,
    pub upstream: Routes,
    pub root: Routes,
}

impl Default for RouteSet {
    fn default() -> Self {
        Self {
            public: Routes::new("/public"),
            auth: Routes::new("/auth"),
            guest: Routes::new("/guest"),
            user: Routes::new(""),
            user_open: Routes::new(""),
            admin_open: Routes::new("/admin"),
            admin: Routes::new("/admin"),
            channel: Routes::new("/channel"),
            upstream: Routes::new("/upstream"),
            root: Routes::new(""),
        }
    }
}

impl RouteSet {
    pub fn merge(self, o: Self) -> Self {
        Self {
            public: self.public.merge(o.public),
            auth: self.auth.merge(o.auth),
            guest: self.guest.merge(o.guest),
            user: self.user.merge(o.user),
            user_open: self.user_open.merge(o.user_open),
            admin_open: self.admin_open.merge(o.admin_open),
            admin: self.admin.merge(o.admin),
            channel: self.channel.merge(o.channel),
            upstream: self.upstream.merge(o.upstream),
            root: self.root.merge(o.root),
        }
    }
}

/// Every group's routes merged.
pub fn all() -> RouteSet {
    [
        affiliate::routes(),
        catalog::routes(),
        content::routes(),
        dashboard::routes(),
        identity::routes(),
        integration::routes(),
        marketing::routes(),
        notify::routes(),
        order::routes(),
        payment::routes(),
        reseller::routes(),
        wallet::routes(),
    ]
    .into_iter()
    .fold(RouteSet::default(), RouteSet::merge)
}

/// An admin permission entry: HTTP method and path pattern in the original
/// `/admin/...:param` syntax (used for RBAC and the permission catalog).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Permission {
    pub method: &'static str,
    pub path: String,
}

/// Router builder that records every registered route for the permission catalog.
#[derive(Debug)]
pub struct Routes {
    prefix: &'static str,
    router: Router<AppState>,
    pub permissions: Vec<Permission>,
}

macro_rules! route_fn {
    ($name:ident, $method:literal, $ctor:ident) => {
        pub fn $name<H, T>(self, path: &str, handler: H) -> Self
        where
            H: Handler<T, AppState>,
            T: 'static,
        {
            self.add($method, path, $ctor(handler))
        }
    };
}

impl Routes {
    /// `prefix` is the permission prefix such as `/admin` (without `/api/v1`).
    pub fn new(prefix: &'static str) -> Self {
        Self {
            prefix,
            router: Router::new(),
            permissions: Vec::new(),
        }
    }

    route_fn!(get, "GET", get);
    route_fn!(post, "POST", post);
    route_fn!(put, "PUT", put);
    route_fn!(patch, "PATCH", patch);
    route_fn!(delete, "DELETE", delete);

    fn add(mut self, method: &'static str, path: &str, route: MethodRouter<AppState>) -> Self {
        self.permissions.push(Permission {
            method,
            path: format!("{}{}", self.prefix, to_colon(path)),
        });
        self.router = self.router.route(path, route);
        self
    }

    pub fn merge(mut self, other: Self) -> Self {
        self.permissions.extend(other.permissions);
        self.router = self.router.merge(other.router);
        self
    }

    pub fn into_parts(self) -> (Router<AppState>, Vec<Permission>) {
        (self.router, self.permissions)
    }
}

/// Converts axum `{param}` segments into the original `:param` syntax.
pub fn to_colon(path: &str) -> String {
    path.split('/')
        .map(
            |seg| match seg.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
                Some(name) => format!(":{}", name.trim_start_matches('*')),
                None => seg.to_owned(),
            },
        )
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::to_colon;

    #[test]
    fn converts_params() {
        assert_eq!(
            to_colon("/categories/{id}/active"),
            "/categories/:id/active"
        );
        assert_eq!(to_colon("/a"), "/a");
    }
}
