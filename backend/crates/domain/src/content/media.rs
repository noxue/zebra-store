//! Media library records and the upload file store port.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use zs_shared::page::{Page, PageRequest};

use crate::{Id, Result};

/// Public URL prefix of uploaded files.
pub const UPLOADS_PREFIX: &str = "/uploads/";

/// A media library entry (`models.Media`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Media {
    pub id: Id,
    /// Editable display name (defaults to the file name without extension).
    pub name: String,
    /// Original file name (immutable).
    pub filename: String,
    /// `/uploads/{scene}/{yyyy}/{mm}/{uuid}.{ext}`.
    pub path: String,
    pub mime_type: String,
    pub size: i64,
    pub scene: String,
    pub width: i32,
    pub height: i32,
    pub created_at: DateTime<Utc>,
}

/// Fields of a new media record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewMedia {
    pub name: String,
    pub filename: String,
    pub path: String,
    pub mime_type: String,
    pub size: i64,
    pub scene: String,
    pub width: i32,
    pub height: i32,
}

/// Admin list filter.
#[derive(Debug, Clone)]
pub struct MediaQuery {
    pub page: PageRequest,
    pub scene: String,
    pub search: String,
}

/// Persistence port for media records (list ordered `created_at DESC`).
#[async_trait]
pub trait MediaRepo: Send + Sync {
    async fn list(&self, query: &MediaQuery) -> Result<Page<Media>>;
    async fn get(&self, id: Id) -> Result<Option<Media>>;
    async fn get_by_path(&self, path: &str) -> Result<Option<Media>>;
    async fn create(&self, media: &NewMedia) -> Result<Media>;
    async fn rename(&self, id: Id, name: &str) -> Result<()>;
    async fn delete(&self, id: Id) -> Result<()>;
}

/// Where uploaded bytes live (local `upload.dir` by default).
#[async_trait]
pub trait FileStore: Send + Sync {
    /// Writes `bytes` to `{scene}/{year}/{month}/{filename}` and returns its public URL path.
    async fn save(
        &self,
        scene: &str,
        year: &str,
        month: &str,
        filename: &str,
        bytes: &[u8],
    ) -> Result<String>;

    /// Removes the file behind a public `/uploads/...` path.
    ///
    /// Must refuse (return an error) for anything resolving outside the upload root.
    /// A missing file is not an error.
    async fn remove(&self, public_path: &str) -> Result<()>;
}

/// True for a well-formed `/uploads/...` path without traversal or odd characters.
pub fn is_safe_upload_path(path: &str) -> bool {
    let Some(rest) = path.strip_prefix(UPLOADS_PREFIX) else {
        return false;
    };
    !rest.is_empty()
        && !rest.contains('\\')
        && !rest.contains('\0')
        && rest
            .split('/')
            .all(|seg| !seg.is_empty() && seg != "." && seg != "..")
}

/// File name without its last extension (`a.b.png` → `a.b`; `.png` stays).
pub fn name_without_extension(filename: &str) -> String {
    match filename.rfind('.') {
        Some(i) if i > 0 => filename[..i].to_owned(),
        _ => filename.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // UPL-03 ④: traversal paths are never accepted.
    #[test]
    fn upl_03_rejects_traversal_paths() {
        assert!(is_safe_upload_path("/uploads/post/2026/01/a.png"));
        assert!(!is_safe_upload_path("/uploads/../../etc/passwd"));
        assert!(!is_safe_upload_path("/../../etc/passwd"));
        assert!(!is_safe_upload_path("/uploads/a//b.png"));
        assert!(!is_safe_upload_path("/uploads/a\\..\\b"));
        assert!(!is_safe_upload_path("/etc/passwd"));
        assert!(!is_safe_upload_path("/uploads/"));
    }

    #[test]
    fn strips_extension() {
        assert_eq!(name_without_extension("photo.final.png"), "photo.final");
        assert_eq!(name_without_extension(".env"), ".env");
        assert_eq!(name_without_extension("README"), "README");
    }
}
