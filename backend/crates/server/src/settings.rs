//! Configuration loading: YAML file + `ZS__SECTION__KEY` environment overrides.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

use anyhow::Context;
use rand::RngCore;
use zs_app::config::Config;

const CONFIG_TEMPLATE: &str = include_str!("../../../config.example.yml");

/// Creates a secure configuration for a new one-binary installation.
#[expect(clippy::print_stdout, reason = "one-time credentials for the operator")]
pub fn init(path: &Path) -> anyhow::Result<()> {
    anyhow::ensure!(!path.exists(), "{} already exists", path.display());
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("create config directory {}", parent.display()))?;
    }

    let app_secret = random_hex(32);
    let jwt_secret = random_hex(32);
    let user_jwt_secret = random_hex(32);
    // The fixed prefix guarantees upper/lowercase letters, a digit and punctuation;
    // the random suffix supplies the entropy.
    let admin_password = format!("Zs9-{}", random_hex(12));
    let rendered = CONFIG_TEMPLATE
        .replace("change-me-app-secret-at-least-16-chars", &app_secret)
        .replace("change-me-admin-jwt-secret-16+", &jwt_secret)
        .replace("change-me-user-jwt-secret-16+", &user_jwt_secret)
        .replace(
            "default_admin_password: \"\"",
            &format!("default_admin_password: \"{admin_password}\""),
        )
        .replace("mode: debug", "mode: release");

    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options
        .open(path)
        .with_context(|| format!("create {}", path.display()))?;
    file.write_all(rendered.as_bytes())
        .with_context(|| format!("write {}", path.display()))?;

    fs::create_dir_all("data").context("create data directory")?;
    fs::create_dir_all("uploads").context("create uploads directory")?;
    println!("Created {}", path.display());
    println!("Initial admin: admin");
    println!("Initial password: {admin_password}");
    println!("Save this password now, then start: zebra-store serve");
    Ok(())
}

pub fn init_if_missing(path: &Path) -> anyhow::Result<()> {
    if path.exists() { Ok(()) } else { init(path) }
}

fn random_hex(bytes: usize) -> String {
    let mut value = vec![0_u8; bytes];
    rand::rng().fill_bytes(&mut value);
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Loads `path` (optional if missing) and applies environment overrides such as
/// `ZS__DATABASE__URL=postgres://…`.
pub fn load(path: &Path) -> anyhow::Result<Config> {
    let mut builder = config::Config::builder();
    if path.exists() {
        builder = builder.add_source(config::File::from(path));
    }
    builder = builder.add_source(environment());
    builder
        .build()
        .context("read configuration")?
        .try_deserialize()
        .with_context(|| format!("parse configuration {}", path.display()))
}

/// `ZS__SECTION__KEY` overrides; every list-valued key accepts a comma-separated
/// value (live QA I-27: `ZS__SERVER__TRUSTED_PROXIES=10.0.0.0/8` failed to parse).
fn environment() -> config::Environment {
    config::Environment::with_prefix("ZS")
        .prefix_separator("__")
        .separator("__")
        .try_parsing(true)
        .list_separator(",")
        .with_list_parse_key("cors.allowed_origins")
        .with_list_parse_key("server.trusted_proxies")
        .with_list_parse_key("upload.allowed_types")
        .with_list_parse_key("upload.allowed_extensions")
        .with_list_parse_key("reseller.main_hosts")
}

#[cfg(test)]
mod tests {
    use super::*;

    // QA-A27 (live QA I-27): list-valued keys can be set from the environment.
    #[test]
    fn qa_a27_list_values_from_env() {
        let vars = [
            ("ZS__SERVER__TRUSTED_PROXIES", "10.0.0.0/8,127.0.0.1"),
            ("ZS__RESELLER__MAIN_HOSTS", "a.example.com"),
        ];
        let cfg = config::Config::builder()
            .add_source(
                environment().source(Some(
                    vars.iter()
                        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                        .collect(),
                )),
            )
            .build()
            .unwrap_or_else(|e| panic!("{e}"));
        let parsed: Config = cfg.try_deserialize().unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            parsed.server.trusted_proxies,
            vec!["10.0.0.0/8", "127.0.0.1"]
        );
        assert_eq!(parsed.reseller.main_hosts, vec!["a.example.com"]);
    }
}
