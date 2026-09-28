//! [`FileStore`] on the local file system (`upload.dir`).

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use zs_domain::content::media::{FileStore, UPLOADS_PREFIX, is_safe_upload_path};
use zs_domain::{Error, Result};

/// Stores uploads below a root directory served at `/uploads`.
#[derive(Debug, Clone)]
pub struct LocalFileStore {
    root: PathBuf,
}

impl LocalFileStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Resolves a public path to a file inside the root; `None` when it would escape.
    async fn resolve(&self, public_path: &str) -> Option<PathBuf> {
        if !is_safe_upload_path(public_path) {
            return None;
        }
        let rel = public_path.strip_prefix(UPLOADS_PREFIX)?;
        let candidate = self.root.join(rel);
        let root = tokio::fs::canonicalize(&self.root).await.ok()?;
        // Canonicalize the parent (the file itself may already be gone).
        let parent = tokio::fs::canonicalize(candidate.parent()?).await.ok()?;
        let resolved = parent.join(candidate.file_name()?);
        resolved.starts_with(&root).then_some(resolved)
    }
}

fn segment_ok(s: &str) -> bool {
    !s.is_empty() && s != "." && s != ".." && !s.contains(['/', '\\', '\0'])
}

#[async_trait]
impl FileStore for LocalFileStore {
    async fn save(
        &self,
        scene: &str,
        year: &str,
        month: &str,
        filename: &str,
        bytes: &[u8],
    ) -> Result<String> {
        if ![scene, year, month, filename].iter().all(|s| segment_ok(s)) {
            return Err(Error::internal_msg("invalid upload path segment"));
        }
        let dir: PathBuf = [
            self.root.as_path(),
            Path::new(scene),
            Path::new(year),
            Path::new(month),
        ]
        .iter()
        .collect();
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(Error::internal)?;
        tokio::fs::write(dir.join(filename), bytes)
            .await
            .map_err(Error::internal)?;
        Ok(format!("{UPLOADS_PREFIX}{scene}/{year}/{month}/{filename}"))
    }

    async fn remove(&self, public_path: &str) -> Result<()> {
        let Some(path) = self.resolve(public_path).await else {
            // Missing parent directory means there is nothing to delete; anything
            // else (traversal, bad prefix) is refused.
            if is_safe_upload_path(public_path) {
                return Ok(());
            }
            return Err(Error::internal_msg(
                "refusing to delete outside the upload root",
            ));
        };
        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(Error::internal(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("zs-files-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    // UPL-03 ③④: files are removed, traversal outside the root is refused.
    #[tokio::test]
    async fn upl_03_save_remove_and_refuse_traversal() {
        let root = temp_root("a");
        let outside = root
            .parent()
            .unwrap()
            .join(format!("zs-outside-{}", std::process::id()));
        std::fs::write(&outside, b"keep").unwrap();
        let store = LocalFileStore::new(root.join("uploads"));
        let url = store
            .save("post", "2026", "01", "a.png", b"x")
            .await
            .unwrap();
        assert_eq!(url, "/uploads/post/2026/01/a.png");
        let file = root.join("uploads/post/2026/01/a.png");
        assert!(file.exists());
        store.remove(&url).await.unwrap();
        assert!(!file.exists());
        // already gone → ok
        store.remove(&url).await.unwrap();
        let name = outside.file_name().unwrap().to_str().unwrap().to_owned();
        assert!(
            store
                .remove(&format!("/uploads/../../{name}"))
                .await
                .is_err()
        );
        assert!(store.remove("/../../etc/passwd").await.is_err());
        assert!(outside.exists());
        assert!(store.save("..", "2026", "01", "a.png", b"x").await.is_err());
        std::fs::remove_file(outside).unwrap();
    }
}
