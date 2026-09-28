//! Media library and file upload use cases (port of `media_service.go` and
//! `upload/application/service.go`). [`UploadService`] is reusable by other groups.

use std::sync::Arc;

use serde::Serialize;
use zs_domain::content::media::{
    FileStore, Media, MediaQuery, MediaRepo, NewMedia, is_safe_upload_path, name_without_extension,
};
use zs_domain::{Error, Id, Result};
use zs_shared::clock::Clock;
use zs_shared::page::Page;

use super::sniff::{MIME_SVG, image_dimensions, looks_like_svg, sniff, validate_svg};

const MEDIA_NOT_FOUND: &str = "media not found";
const NAME_EMPTY: &str = "media name empty";
/// Scenes accepted by the upload endpoint; anything else becomes `common`.
const SCENES: [&str; 8] = [
    "product", "post", "banner", "editor", "common", "category", "telegram", "reseller",
];
/// Attachment scene that skips the image allow-lists (UPL-04).
const SCENE_ATTACHMENT: &str = "telegram";
/// Extensions never accepted, even for attachments (renderable or executable, UPL-04).
const BLOCKED_EXTENSIONS: [&str; 22] = [
    ".html", ".htm", ".xhtml", ".shtml", ".svg", ".svgz", ".xml", ".xsl", ".js", ".mjs", ".php",
    ".phtml", ".jsp", ".asp", ".aspx", ".cgi", ".pl", ".py", ".sh", ".exe", ".bat", ".cmd",
];
const MAX_EXTENSION_LEN: usize = 10;

/// Upload validation policy (`config.upload`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadPolicy {
    pub max_size: u64,
    pub allowed_types: Vec<String>,
    pub allowed_extensions: Vec<String>,
    pub max_width: u32,
    pub max_height: u32,
}

/// A stored upload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoredFile {
    pub url: String,
    pub filename: String,
    pub mime_type: String,
    pub size: i64,
    pub width: i32,
    pub height: i32,
}

/// Normalizes an upload scene.
pub fn normalize_scene(raw: &str) -> String {
    let v = raw.trim().to_ascii_lowercase();
    if SCENES.contains(&v.as_str()) {
        v
    } else {
        "common".to_owned()
    }
}

// Upload validation failures (live QA I-15: these were hard-coded Chinese text in
// every locale; the zh-CN messages keep the original wording).
const KEY_TOO_LARGE: &str = "error.upload_file_too_large";
const KEY_EXTENSION: &str = "error.upload_extension_not_allowed";
const KEY_TYPE: &str = "error.upload_type_not_allowed";
const KEY_IMAGE_INVALID: &str = "error.upload_image_invalid";
const KEY_TOO_WIDE: &str = "error.upload_image_too_wide";
const KEY_TOO_TALL: &str = "error.upload_image_too_tall";
const KEY_SVG_UNSAFE: &str = "error.upload_svg_unsafe";

/// Lower-cased extension (with dot) of the file's base name, like Go's `filepath.Ext`.
fn extension(filename: &str) -> String {
    let base = filename.rsplit(['/', '\\']).next().unwrap_or(filename);
    match base.rfind('.') {
        Some(i) => base[i..].to_ascii_lowercase(),
        None => String::new(),
    }
}

fn base_name(filename: &str) -> String {
    filename
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(filename)
        .to_owned()
}

/// Validates and stores uploaded files.
#[derive(Clone)]
pub struct UploadService {
    policy: UploadPolicy,
    files: Arc<dyn FileStore>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for UploadService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UploadService")
            .field("policy", &self.policy)
            .finish_non_exhaustive()
    }
}

impl UploadService {
    pub fn new(policy: UploadPolicy, files: Arc<dyn FileStore>, clock: Arc<dyn Clock>) -> Self {
        Self {
            policy,
            files,
            clock,
        }
    }

    pub fn max_size(&self) -> u64 {
        self.policy.max_size
    }

    /// Error returned when a file exceeds the size limit.
    pub fn too_large(&self) -> Error {
        Error::bad_request(KEY_TOO_LARGE).arg(self.policy.max_size / 1024 / 1024)
    }

    /// Validates size, extension, magic bytes, dimensions and SVG safety, then stores
    /// the bytes under `/uploads/{scene}/{yyyy}/{mm}/{uuid}{ext}`.
    pub async fn save(&self, filename: &str, bytes: &[u8], scene: &str) -> Result<StoredFile> {
        let scene = normalize_scene(scene);
        let attachment = scene == SCENE_ATTACHMENT;
        if bytes.len() as u64 > self.policy.max_size {
            return Err(self.too_large());
        }
        let ext = extension(filename);
        let ext_ok = ext.len() <= MAX_EXTENSION_LEN
            && ext.chars().skip(1).all(|c| c.is_ascii_alphanumeric());
        if !ext_ok || (attachment && BLOCKED_EXTENSIONS.contains(&ext.as_str())) {
            return Err(Error::bad_request(KEY_EXTENSION).arg(&ext));
        }
        if !attachment && !self.policy.allowed_extensions.is_empty() {
            let allowed = !ext.is_empty()
                && self.policy.allowed_extensions.iter().any(|a| {
                    let a = a.trim().to_ascii_lowercase();
                    let a = if a.starts_with('.') {
                        a
                    } else {
                        format!(".{a}")
                    };
                    !a.is_empty() && a == ext
                });
            if !allowed {
                return Err(Error::bad_request(KEY_EXTENSION).arg(&ext));
            }
        }

        let mut mime = sniff(bytes);
        if ext == ".svg" && looks_like_svg(bytes) {
            mime = MIME_SVG.to_owned();
        }
        if attachment && (mime.starts_with("text/html") || mime.starts_with("text/xml")) {
            return Err(Error::bad_request(KEY_TYPE).arg(&mime));
        }
        if !attachment
            && !self.policy.allowed_types.is_empty()
            && !self
                .policy
                .allowed_types
                .iter()
                .any(|t| t.eq_ignore_ascii_case(&mime))
        {
            return Err(Error::bad_request(KEY_TYPE).arg(&mime));
        }

        let (mut width, mut height) = (0, 0);
        if mime.starts_with("image/") && mime != MIME_SVG {
            let (w, h) = image_dimensions(bytes, &mime)
                .map_err(|detail| Error::bad_request(KEY_IMAGE_INVALID).arg(detail))?;
            if self.policy.max_width > 0 && w > self.policy.max_width {
                return Err(Error::bad_request(KEY_TOO_WIDE).arg(self.policy.max_width));
            }
            if self.policy.max_height > 0 && h > self.policy.max_height {
                return Err(Error::bad_request(KEY_TOO_TALL).arg(self.policy.max_height));
            }
            width = i32::try_from(w).unwrap_or(i32::MAX);
            height = i32::try_from(h).unwrap_or(i32::MAX);
        }
        if mime == MIME_SVG {
            validate_svg(bytes).map_err(|detail| Error::bad_request(KEY_SVG_UNSAFE).arg(detail))?;
        }

        let now = self.clock.now();
        let stored_name = format!("{}{ext}", uuid::Uuid::new_v4());
        let url = self
            .files
            .save(
                &scene,
                &now.format("%Y").to_string(),
                &now.format("%m").to_string(),
                &stored_name,
                bytes,
            )
            .await
            .map_err(|e| e.or_internal("error.upload_failed"))?;
        Ok(StoredFile {
            url,
            filename: base_name(filename),
            mime_type: mime,
            size: i64::try_from(bytes.len()).unwrap_or(i64::MAX),
            width,
            height,
        })
    }
}

/// Media library queries and commands.
#[derive(Clone)]
pub struct MediaService {
    repo: Arc<dyn MediaRepo>,
    files: Arc<dyn FileStore>,
}

impl std::fmt::Debug for MediaService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MediaService")
    }
}

impl MediaService {
    pub fn new(repo: Arc<dyn MediaRepo>, files: Arc<dyn FileStore>) -> Self {
        Self { repo, files }
    }

    pub async fn list(&self, query: &MediaQuery) -> Result<Page<Media>> {
        self.repo.list(query).await
    }

    /// Records an upload in the library, de-duplicated by path.
    pub async fn record(&self, file: &StoredFile, scene: &str) -> Result<Media> {
        if let Some(existing) = self.repo.get_by_path(&file.url).await? {
            return Ok(existing);
        }
        self.repo
            .create(&NewMedia {
                name: name_without_extension(&file.filename),
                filename: file.filename.clone(),
                path: file.url.clone(),
                mime_type: file.mime_type.clone(),
                size: file.size,
                scene: normalize_scene(scene),
                width: file.width,
                height: file.height,
            })
            .await
    }

    pub async fn rename(&self, id: Id, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::internal_msg(NAME_EMPTY).or_internal("error.internal"));
        }
        if self.repo.get(id).await?.is_none() {
            return Err(Error::internal_msg(MEDIA_NOT_FOUND).or_internal("error.internal"));
        }
        self.repo.rename(id, name).await
    }

    /// Soft-deletes the record, then removes the file (best effort, never outside the upload root).
    pub async fn delete(&self, id: Id) -> Result<()> {
        let media =
            self.repo.get(id).await?.ok_or_else(|| {
                Error::internal_msg(MEDIA_NOT_FOUND).or_internal("error.internal")
            })?;
        if !is_safe_upload_path(&media.path) {
            tracing::warn!(id, path = %media.path, "refusing to delete media with unsafe path");
            return Err(Error::internal_msg("unsafe media path").or_internal("error.internal"));
        }
        self.repo.delete(id).await?;
        if let Err(error) = self.files.remove(&media.path).await {
            tracing::warn!(id, path = %media.path, %error, "media file delete failed");
        }
        Ok(())
    }

    /// Deletes each id through [`Self::delete`]; returns `(success_count, failed_ids)`.
    pub async fn batch_delete(&self, ids: &[Id]) -> (usize, Vec<Id>) {
        let mut ok = 0;
        let mut failed = Vec::new();
        for id in ids {
            match self.delete(*id).await {
                Ok(()) => ok += 1,
                Err(_) => failed.push(*id),
            }
        }
        (ok, failed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Mutex;
    use zs_shared::clock::SystemClock;

    #[derive(Default)]
    struct MemFiles(Mutex<Vec<String>>);

    #[async_trait]
    impl FileStore for MemFiles {
        async fn save(
            &self,
            scene: &str,
            y: &str,
            m: &str,
            name: &str,
            _: &[u8],
        ) -> Result<String> {
            let url = format!("/uploads/{scene}/{y}/{m}/{name}");
            self.0.lock().unwrap().push(url.clone());
            Ok(url)
        }
        async fn remove(&self, path: &str) -> Result<()> {
            self.0.lock().unwrap().retain(|p| p != path);
            Ok(())
        }
    }

    fn policy() -> UploadPolicy {
        UploadPolicy {
            max_size: 10 * 1024 * 1024,
            allowed_types: ["image/png", "image/svg+xml"].map(String::from).to_vec(),
            allowed_extensions: [".png", "svg"].map(String::from).to_vec(),
            max_width: 100,
            max_height: 100,
        }
    }

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut v = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR".to_vec();
        v.extend_from_slice(&w.to_be_bytes());
        v.extend_from_slice(&h.to_be_bytes());
        v
    }

    fn service() -> (UploadService, Arc<MemFiles>) {
        let files = Arc::new(MemFiles::default());
        (
            UploadService::new(policy(), files.clone(), Arc::new(SystemClock)),
            files,
        )
    }

    #[tokio::test]
    async fn stores_png_with_dimensions_under_scene_path() {
        let (svc, _) = service();
        let f = svc
            .save("dir/My Photo.PNG", &png(20, 10), "Banner")
            .await
            .unwrap();
        assert!(f.url.starts_with("/uploads/banner/"));
        assert!(f.url.ends_with(".png"));
        assert_eq!((f.width, f.height), (20, 10));
        assert_eq!(f.filename, "My Photo.PNG");
        assert_eq!(f.mime_type, "image/png");
    }

    // UPL-05: validation failures carry a readable message.
    #[tokio::test]
    async fn upl_05_validation_messages() {
        let (svc, _) = service();
        let big = vec![0u8; 11 * 1024 * 1024];
        // QA-A15: translatable keys with the detail as an argument
        let key_args = |e: Error| (e.key().to_owned(), e.args().to_vec());
        assert_eq!(
            key_args(svc.save("a.png", &big, "").await.unwrap_err()),
            (KEY_TOO_LARGE.to_owned(), vec!["10".to_owned()])
        );
        assert_eq!(
            key_args(svc.save("a.gif", b"GIF89a", "").await.unwrap_err()),
            (KEY_EXTENSION.to_owned(), vec![".gif".to_owned()])
        );
        assert_eq!(
            key_args(svc.save("a.png", b"hello", "").await.unwrap_err()),
            (
                KEY_TYPE.to_owned(),
                vec!["text/plain; charset=utf-8".to_owned()]
            )
        );
        assert_eq!(
            key_args(svc.save("a.png", &png(200, 1), "").await.unwrap_err()),
            (KEY_TOO_WIDE.to_owned(), vec!["100".to_owned()])
        );
        let err = svc
            .save("x.svg", b"<svg><script>alert(1)</script></svg>", "")
            .await
            .unwrap_err();
        assert_eq!(
            key_args(err),
            (
                KEY_SVG_UNSAFE.to_owned(),
                vec!["SVG 文件不允许包含 <script> 标签".to_owned()]
            )
        );
        // .svg extension with HTML content is not an SVG
        let err = svc
            .save("x.svg", b"<html><body>x</body></html>", "")
            .await
            .unwrap_err();
        assert_eq!(err.key(), KEY_TYPE);
        assert!(
            svc.save("ok.svg", b"<svg><circle/></svg>", "")
                .await
                .is_ok()
        );
    }

    // UPL-04: the attachment scene skips the image allow-list but never accepts renderable files.
    #[tokio::test]
    async fn upl_04_attachment_scene_blocklist() {
        let (svc, _) = service();
        let zip = b"PK\x03\x04rest";
        assert!(svc.save("bundle.zip", zip, "telegram").await.is_ok());
        for name in ["a.html", "b.svg", "c.js", "d.php"] {
            assert!(
                svc.save(name, b"<html></html>", "telegram").await.is_err(),
                "{name}"
            );
        }
        assert!(
            svc.save("page.txt", b"<html><body>", "telegram")
                .await
                .is_err()
        );
    }

    #[test]
    fn scene_normalization() {
        assert_eq!(normalize_scene(" POST "), "post");
        assert_eq!(normalize_scene("../etc"), "common");
        assert_eq!(normalize_scene(""), "common");
    }
}
