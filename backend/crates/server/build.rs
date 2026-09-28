use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_EMBEDDED_WEB");
    if env::var_os("CARGO_FEATURE_EMBEDDED_WEB").is_none() {
        return;
    }

    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap_or_default());
    let storefront = manifest.join("../../../storefront/dist");
    let admin = manifest.join("../../../admin/dist");
    println!("cargo:rerun-if-changed={}", storefront.display());
    println!("cargo:rerun-if-changed={}", admin.display());

    let storefront_assets = assets(&storefront);
    let admin_assets = assets(&admin);
    require_index("storefront", &storefront_assets);
    require_index("admin", &admin_assets);

    let mut generated = String::from(
        "#[derive(Clone, Copy, Debug)]\n\
         pub struct EmbeddedAsset { pub path: &'static str, pub bytes: &'static [u8] }\n",
    );
    write_assets(&mut generated, "STOREFRONT_ASSETS", &storefront_assets);
    write_assets(&mut generated, "ADMIN_ASSETS", &admin_assets);

    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap_or_default());
    fs::write(out.join("embedded_web_assets.rs"), generated)
        .unwrap_or_else(|error| panic!("write embedded web asset table: {error}"));
}

fn assets(root: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    visit(root, root, &mut out);
    out.sort_by(|left, right| left.0.cmp(&right.0));
    out
}

fn visit(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) {
    let entries = fs::read_dir(dir).unwrap_or_else(|error| {
        panic!(
            "read web build directory {}: {error}; build storefront and admin before enabling embedded-web",
            dir.display()
        )
    });
    for entry in entries {
        let entry = entry.unwrap_or_else(|error| panic!("read web build entry: {error}"));
        let path = entry.path();
        if path.is_dir() {
            visit(root, &path, out);
        } else if path.is_file() {
            let relative = path
                .strip_prefix(root)
                .unwrap_or_else(|error| panic!("strip web build prefix: {error}"))
                .to_string_lossy()
                .replace('\\', "/");
            out.push((relative, path));
        }
    }
}

fn require_index(name: &str, assets: &[(String, PathBuf)]) {
    assert!(
        assets.iter().any(|(path, _)| path == "index.html"),
        "{name}/dist/index.html is missing; build the frontend before enabling embedded-web"
    );
}

fn write_assets(output: &mut String, name: &str, assets: &[(String, PathBuf)]) {
    output.push_str(&format!("pub static {name}: &[EmbeddedAsset] = &[\n"));
    for (path, source) in assets {
        output.push_str(&format!(
            "    EmbeddedAsset {{ path: {path:?}, bytes: include_bytes!({source:?}) }},\n",
            source = source.to_string_lossy()
        ));
    }
    output.push_str("];\n");
}
