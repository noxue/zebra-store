//! Catalog endpoints: categories, products, card secrets.

mod card_secret;
mod product;

use axum::extract::State;
use serde::{Deserialize, Serialize};
use zs_domain::Id;
use zs_domain::catalog::category::{Category, CategoryInput};
use zs_shared::i18n::LocalizedText;

use super::{RouteSet, Routes};
use crate::extract::{Bind, BindField, BindRules, Body, PathId, req};
use crate::response::{ApiResult, Data, ok};
use crate::state::AppState;
use zs_shared::page::PageRequest;

/// Routes of the catalog group.
pub fn routes() -> RouteSet {
    RouteSet {
        public: public().merge(product::public()),
        admin: admin().merge(product::admin()).merge(card_secret::admin()),
        ..RouteSet::default()
    }
}

/// `/api/v1/public/*` catalog routes.
fn public() -> Routes {
    Routes::new("/public").get("/categories", public_categories)
}

/// `/api/v1/admin/*` catalog routes.
fn admin() -> Routes {
    Routes::new("/admin")
        .get("/categories", admin_categories)
        .post("/categories", create_category)
        .put("/categories/{id}", update_category)
        .patch("/categories/{id}/active", patch_category_active)
        .delete("/categories/{id}", delete_category)
}

/// Public category shape (no timestamps or active flag).
#[derive(Debug, Serialize)]
struct PublicCategory {
    id: Id,
    parent_id: Id,
    slug: String,
    name: LocalizedText,
    #[serde(skip_serializing_if = "String::is_empty")]
    icon: String,
    sort_order: i32,
}

impl From<Category> for PublicCategory {
    fn from(c: Category) -> Self {
        Self {
            id: c.id,
            parent_id: c.parent_id,
            slug: c.slug,
            name: c.name,
            icon: c.icon,
            sort_order: c.sort_order,
        }
    }
}

async fn public_categories(State(s): State<AppState>) -> ApiResult<Data<Vec<PublicCategory>>> {
    let list = s.svc.catalog.category.list_active().await?;
    ok(list.into_iter().map(Into::into).collect())
}

async fn admin_categories(State(s): State<AppState>) -> ApiResult<Data<Vec<Category>>> {
    ok(s.svc.catalog.category.list().await?)
}

#[derive(Debug, Deserialize)]
struct CategoryRequest {
    #[serde(default)]
    parent_id: Id,
    slug: String,
    name: LocalizedText,
    #[serde(default)]
    icon: String,
    #[serde(default)]
    sort_order: i32,
}

/// Original `CreateCategoryRequest` (create and update).
impl BindRules for CategoryRequest {
    const FIELDS: &'static [BindField] = &[req("slug", "Slug"), req("name", "NameJSON")];
}

impl CategoryRequest {
    fn into_input(self) -> ApiResult<CategoryInput> {
        if self.slug.trim().is_empty() {
            return Err(zs_domain::Error::invalid().into());
        }
        Ok(CategoryInput {
            parent_id: self.parent_id,
            slug: self.slug.trim().to_owned(),
            name: self.name,
            icon: self.icon,
            sort_order: self.sort_order,
        })
    }
}

async fn create_category(
    State(s): State<AppState>,
    Bind(req): Bind<CategoryRequest>,
) -> ApiResult<Data<Category>> {
    ok(s.svc.catalog.category.create(req.into_input()?).await?)
}

async fn update_category(
    State(s): State<AppState>,
    PathId(id): PathId,
    Bind(req): Bind<CategoryRequest>,
) -> ApiResult<Data<Category>> {
    ok(s.svc.catalog.category.update(id, req.into_input()?).await?)
}

#[derive(Debug, Deserialize)]
struct ActiveRequest {
    is_active: bool,
}

async fn patch_category_active(
    State(s): State<AppState>,
    PathId(id): PathId,
    Body(req): Body<ActiveRequest>,
) -> ApiResult<Data<Category>> {
    ok(s.svc.catalog.category.set_active(id, req.is_active).await?)
}

async fn delete_category(State(s): State<AppState>, PathId(id): PathId) -> ApiResult<Data<()>> {
    s.svc.catalog.category.delete(id).await?;
    ok(())
}

/// Lenient `page` / `page_size` query (non-numeric values fall back to defaults like `Atoi`).
#[derive(Debug, Default, Deserialize)]
struct PageQuery {
    #[serde(default)]
    page: Option<String>,
    #[serde(default)]
    page_size: Option<String>,
}

impl PageQuery {
    fn request(&self) -> PageRequest {
        let parse = |v: &Option<String>| v.as_deref().and_then(|s| s.trim().parse::<u64>().ok());
        PageRequest::new(parse(&self.page), parse(&self.page_size))
    }
}

/// Parses an optional unsigned id query value (`""` → 0); `zero_invalid` rejects `0`
/// (original `ginutil.ParseQueryUint`).
fn parse_query_id(raw: Option<&str>, zero_invalid: bool) -> Option<Id> {
    let raw = raw.map(str::trim).unwrap_or_default();
    if raw.is_empty() {
        return Some(0);
    }
    let v = raw.parse::<u64>().ok()?;
    if zero_invalid && v == 0 {
        return None;
    }
    Id::try_from(v).ok()
}

/// Tri-state boolean filter (`""`/`all` → none; original `parseTriStateBoolFilter`).
fn parse_tri_state(raw: Option<&str>) -> Result<Option<bool>, ()> {
    match raw.map(|r| r.trim().to_ascii_lowercase()).as_deref() {
        None | Some("" | "all") => Ok(None),
        Some("1" | "true" | "yes" | "on" | "enabled" | "has" | "t") => Ok(Some(true)),
        Some("0" | "false" | "no" | "off" | "disabled" | "none" | "f") => Ok(Some(false)),
        Some(_) => Err(()),
    }
}

/// Go `strconv.ParseBool` for optional query/form values (`""` → none).
fn parse_bool_value(raw: Option<&str>) -> Result<Option<bool>, ()> {
    match raw.map(str::trim) {
        None | Some("") => Ok(None),
        Some("1" | "t" | "T" | "TRUE" | "true" | "True") => Ok(Some(true)),
        Some("0" | "f" | "F" | "FALSE" | "false" | "False") => Ok(Some(false)),
        Some(_) => Err(()),
    }
}
