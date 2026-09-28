//! Configuration loading: YAML file + `ZS__SECTION__KEY` environment overrides.

use std::path::Path;

use anyhow::Context;
use zs_app::config::Config;

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
