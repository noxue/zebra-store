//! HTTP API (axum) of Zebra Store.
//!
//! Handlers are thin: parse the request, call a service from [`zs_app::Services`],
//! and wrap the result in the original response envelope.

pub mod client;
pub mod extract;
pub mod i18n;
pub mod middleware;
pub mod response;
pub mod routes;
pub mod state;

use axum::Router;
use axum::routing::get;
use serde_json::json;

pub use routes::Permission;
pub use state::AppState;

/// The assembled application: router plus the admin permission catalog.
#[derive(Debug)]
pub struct App {
    pub router: Router,
    pub admin_permissions: Vec<Permission>,
}

/// Builds the full HTTP application.
pub fn build(mut state: AppState) -> App {
    let set = routes::all();
    let (admin, admin_permissions) = set.admin.into_parts();
    state.admin_permissions = std::sync::Arc::new(admin_permissions.clone());
    // Authenticated admin routes: JWT auth (outer) then RBAC.
    let admin = admin
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::auth::rbac,
        ))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::auth::require_admin,
        ));
    let user_empty = set.user.permissions.is_empty();
    let mut user = set.user.into_parts().0;
    if !user_empty {
        // axum panics when a route layer is added to a router without routes.
        user = user.route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::auth::require_user,
        ));
        // Reseller tenant (outermost): runs before user authentication like the original.
        user = middleware::tenant::layer(user, &state);
    }

    let api_v1 = Router::new()
        .nest(
            "/public",
            middleware::tenant::storefront(set.public, &state),
        )
        .nest("/auth", middleware::tenant::storefront(set.auth, &state))
        .nest("/guest", middleware::tenant::storefront(set.guest, &state))
        .nest("/admin", set.admin_open.into_parts().0.merge(admin))
        .nest("/channel", channel_router(&state, set.channel))
        .nest("/upstream", set.upstream.into_parts().0)
        .merge(user)
        .merge(set.user_open.into_parts().0);

    let router = Router::new()
        .nest("/api/v1", api_v1)
        .merge(set.root.into_parts().0)
        .route(
            "/health",
            get(|| async { axum::Json(json!({ "status": "ok" })) }),
        )
        .layer(axum::middleware::from_fn(response::render_errors))
        .layer(axum::middleware::from_fn(middleware::request_id::layer))
        .with_state(state);

    App {
        router,
        admin_permissions,
    }
}

/// Channel API routes behind the channel HMAC middleware (notify group).
fn channel_router(state: &AppState, routes: routes::Routes) -> Router<AppState> {
    let empty = routes.permissions.is_empty();
    let router = routes.into_parts().0;
    if empty {
        return router;
    }
    router.route_layer(axum::middleware::from_fn_with_state(
        state.clone(),
        routes::notify::channel::require_channel,
    ))
}
