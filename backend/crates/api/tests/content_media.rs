//! Uploads, media library and safe `/uploads` serving.

mod common;
mod content_common;

use axum::http::StatusCode;
use common::data;
use content_common::{app_with_uploads, get_raw, png, temp_dir, upload, upload_with_token};
use sea_orm::{ActiveModelTrait, EntityTrait, Set};
use serde_json::json;
use zs_api::routes::content::uploads::uploads_router;
use zs_infra::db::entity::media;

fn disk_path(root: &std::path::Path, url: &str) -> std::path::PathBuf {
    root.join(url.trim_start_matches("/uploads/"))
}

// UPL-03 ①②③, UPL-05: upload records media with dimensions; delete removes the file.
#[tokio::test]
async fn upl_03_upload_record_and_delete() {
    let root = temp_dir("media");
    let app = app_with_uploads(root.to_str().unwrap()).await;

    let v = upload(&app, "Cover Photo.png", &png(64, 32), Some("post")).await;
    let d = data(&v);
    let url = d["url"].as_str().unwrap().to_owned();
    assert!(url.starts_with("/uploads/post/"));
    assert!(url.ends_with(".png"));
    assert_eq!(d["filename"], "Cover Photo.png");
    let media_id = d["media_id"].as_i64().unwrap();
    assert!(media_id > 0);
    let file = disk_path(&root, &url);
    assert!(file.exists());

    let list = app.get("/api/v1/admin/media?scene=post&search=cover").await;
    let d = data(&list);
    assert_eq!(d["total"], 1);
    let item = &d["items"][0];
    assert_eq!(item["name"], "Cover Photo");
    assert_eq!(item["path"], url);
    assert_eq!(item["mime_type"], "image/png");
    assert_eq!(
        (item["width"].as_i64(), item["height"].as_i64()),
        (Some(64), Some(32))
    );

    // ② recording the same path again does not duplicate
    let stored = zs_app::content::media::StoredFile {
        url: url.clone(),
        filename: "x.png".into(),
        mime_type: "image/png".into(),
        size: 1,
        width: 1,
        height: 1,
    };
    let again = app
        .services
        .content
        .media
        .record(&stored, "post")
        .await
        .unwrap();
    assert_eq!(again.id, media_id);

    // rename
    let v = app
        .put(
            &format!("/api/v1/admin/media/{media_id}"),
            json!({"name": " 封面 "}),
        )
        .await;
    assert!(data(&v).is_null());
    let bad = app
        .put(
            &format!("/api/v1/admin/media/{media_id}"),
            json!({"name": ""}),
        )
        .await;
    assert_eq!(bad["status_code"], 400);

    // ③ delete removes the physical file
    assert!(data(&app.delete(&format!("/api/v1/admin/media/{media_id}")).await).is_null());
    assert!(!file.exists());
    let list = app.get("/api/v1/admin/media").await;
    assert_eq!(data(&list)["total"], 0);
}

// UPL-03 ④: a tampered media path is never used to delete files outside the upload root.
#[tokio::test]
async fn upl_03_tampered_path_refused() {
    let root = temp_dir("tamper");
    let victim = root.join("victim.txt");
    std::fs::write(&victim, b"keep").unwrap();
    let uploads = root.join("uploads");
    std::fs::create_dir_all(&uploads).unwrap();
    let app = app_with_uploads(uploads.to_str().unwrap()).await;
    let v = upload(&app, "a.png", &png(1, 1), None).await;
    let media_id = data(&v)["media_id"].as_i64().unwrap();

    for path in ["/uploads/../victim.txt", "/../../etc/passwd"] {
        let row = media::Entity::find_by_id(media_id)
            .one(&app.db)
            .await
            .unwrap()
            .unwrap();
        let mut active: media::ActiveModel = row.into();
        active.path = Set(path.into());
        active.update(&app.db).await.unwrap();
        let v = app.delete(&format!("/api/v1/admin/media/{media_id}")).await;
        assert_eq!(v["status_code"], 500, "{path}");
    }
    assert!(victim.exists());
}

// UPL-01 ⑤ / UPL-05: validation errors are 400 with a readable message.
#[tokio::test]
async fn upl_05_validation_errors() {
    let root = temp_dir("invalid");
    let app = app_with_uploads(root.to_str().unwrap()).await;

    let big = vec![0u8; 11 * 1024 * 1024];
    let v = upload(&app, "big.png", &big, None).await;
    assert_eq!(v["status_code"], 400);
    assert_eq!(v["msg"], "文件大小超过限制（最大 10 MB）");

    // SVG is disabled by default
    let v = upload(&app, "x.svg", b"<svg><script>alert(1)</script></svg>", None).await;
    assert_eq!(v["status_code"], 400);
    assert_eq!(v["msg"], "文件扩展名不被允许: .svg");

    let v = upload(&app, "x.png", b"not an image", None).await;
    assert_eq!(v["msg"], "文件类型不被允许: text/plain; charset=utf-8");

    let v = upload(&app, "huge.png", &png(5000, 10), None).await;
    assert_eq!(v["msg"], "图片宽度超过限制（最大 4096）");

    // QA-A15 (live QA I-15): the messages follow the request locale
    let token = app.admin_token.clone();
    let v = content_common::upload_at(
        &app,
        "/api/v1/admin/upload?lang=en-US",
        "x.php",
        b"<?php echo 1; ?>",
        None,
        token.as_deref(),
    )
    .await;
    assert_eq!(v["msg"], "File extension not allowed: .php");
    let v = content_common::upload_at(
        &app,
        "/api/v1/admin/upload?lang=zh-TW",
        "big.png",
        &big,
        None,
        token.as_deref(),
    )
    .await;
    assert_eq!(v["msg"], "檔案大小超過限制（最大 10 MB）");

    // missing file field
    let v = content_common::raw(
        &app.router,
        axum::http::Request::builder()
            .method("POST")
            .uri("/api/v1/admin/upload")
            .header(
                "authorization",
                format!("Bearer {}", app.admin_token.as_deref().unwrap()),
            )
            .header("content-type", "multipart/form-data; boundary=b")
            .body(axum::body::Body::from("--b--\r\n"))
            .unwrap(),
    )
    .await;
    let v: serde_json::Value = serde_json::from_slice(&v.2).unwrap();
    assert_eq!(v["msg"], "未上传文件");

    // UPL-04: uploads need an administrator
    let v = upload_with_token(&app, "a.zip", b"PK\x03\x04", Some("telegram"), None).await;
    assert_eq!(v["status_code"], 401);
}

// UPL-01 ⑤ with SVG explicitly enabled: script SVGs are rejected, clean ones accepted.
#[tokio::test]
async fn upl_01_svg_when_enabled() {
    let root = temp_dir("svg");
    let mut cfg = common::test_config();
    cfg.upload.dir = root.to_str().unwrap().to_owned();
    cfg.upload.allowed_types.push("image/svg+xml".into());
    cfg.upload.allowed_extensions.push(".svg".into());
    let db = zs_infra::db::connect(&cfg.database).await.unwrap();
    zs_infra::db::sync_schema(&db).await.unwrap();
    let ctx = zs_infra::wire::WireCtx::new(&db, &cfg);
    let services = zs_infra::wire::services(&ctx);
    zs_infra::wire::bootstrap(&ctx, &services).await.unwrap();
    let built = zs_api::build(zs_api::AppState::new(services.clone(), cfg));
    let mut app = common::TestApp {
        router: built.router,
        db,
        services,
        admin_token: None,
    };
    app.admin_token = Some(app.login("admin", "Admin12345").await);

    for (payload, msg) in [
        (
            "<svg><script>alert(1)</script></svg>",
            "SVG 文件不允许包含 <script> 标签",
        ),
        (
            "<svg onbegin=\"alert(1)\"></svg>",
            "SVG 文件不允许包含事件处理属性: onbegin",
        ),
        (
            "<svg><a href=\"&#106;avascript:alert(1)\"></a></svg>",
            "SVG 文件不允许包含 javascript: 协议",
        ),
        (
            "<!DOCTYPE svg [<!ENTITY x \"y\">]><svg>&x;</svg>",
            "SVG 文件不允许声明实体",
        ),
    ] {
        let v = upload(&app, "x.svg", payload.as_bytes(), None).await;
        assert_eq!(v["status_code"], 400, "{payload}");
        assert_eq!(v["msg"], msg);
    }
    let v = upload(&app, "x.svg", b"<html><svg></svg></html>", None).await;
    assert_eq!(v["status_code"], 400);
    let v = upload(
        &app,
        "ok.svg",
        b"<svg xmlns=\"http://www.w3.org/2000/svg\"><circle r=\"2\"/></svg>",
        None,
    )
    .await;
    assert!(data(&v)["url"].as_str().unwrap().ends_with(".svg"));
}

// UPL-04: the attachment scene accepts archives but never renderable files.
#[tokio::test]
async fn upl_04_attachment_scene() {
    let root = temp_dir("attach");
    let app = app_with_uploads(root.to_str().unwrap()).await;
    let v = upload(&app, "bundle.zip", b"PK\x03\x04data", Some("telegram")).await;
    assert!(
        data(&v)["url"]
            .as_str()
            .unwrap()
            .starts_with("/uploads/telegram/")
    );
    for name in ["a.html", "a.svg", "a.js"] {
        let v = upload(&app, name, b"<html></html>", Some("telegram")).await;
        assert_eq!(v["status_code"], 400, "{name}");
    }
}

#[tokio::test]
async fn batch_delete_reuses_single_delete() {
    let root = temp_dir("batch");
    let app = app_with_uploads(root.to_str().unwrap()).await;
    let a = upload(&app, "a.png", &png(1, 1), None).await;
    let b = upload(&app, "b.png", &png(1, 1), None).await;
    let (ida, idb) = (
        data(&a)["media_id"].as_i64().unwrap(),
        data(&b)["media_id"].as_i64().unwrap(),
    );
    let v = app
        .post(
            "/api/v1/admin/media/batch-delete",
            json!({"ids": [ida, 999, idb]}),
        )
        .await;
    assert_eq!(
        data(&v),
        &json!({"total": 3, "success_count": 2, "failed_ids": [999]})
    );
    assert!(!disk_path(&root, data(&a)["url"].as_str().unwrap()).exists());
    let v = app
        .post("/api/v1/admin/media/batch-delete", json!({"ids": []}))
        .await;
    assert_eq!(v["status_code"], 400);
}

// UPL-01 / UPL-04: /uploads forces download + sandbox for SVG and attachments, nosniff everywhere.
#[tokio::test]
async fn upl_01_uploads_serving_headers() {
    let root = temp_dir("serve");
    std::fs::write(root.join("a.svg"), b"<svg onload=\"alert(1)\"></svg>").unwrap();
    std::fs::write(root.join("b.png"), png(1, 1)).unwrap();
    std::fs::write(root.join("c.zip"), b"PK").unwrap();
    let router = uploads_router(root.to_str().unwrap());

    let (status, h, _) = get_raw(&router, "/a.svg", &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(h["content-disposition"], "attachment");
    assert!(
        h["content-security-policy"]
            .to_str()
            .unwrap()
            .contains("sandbox")
    );
    assert!(
        h["content-security-policy"]
            .to_str()
            .unwrap()
            .contains("script-src 'none'")
    );
    assert_eq!(h["x-content-type-options"], "nosniff");

    let (status, h, body) = get_raw(&router, "/b.png", &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert!(h.get("content-disposition").is_none());
    assert_eq!(h["x-content-type-options"], "nosniff");
    assert_eq!(body.len(), png(1, 1).len());

    let (_, h, _) = get_raw(&router, "/c.zip", &[]).await;
    assert_eq!(h["content-disposition"], "attachment");

    let (status, _, _) = get_raw(&router, "/../Cargo.toml", &[]).await;
    assert_ne!(status, StatusCode::OK);
}
