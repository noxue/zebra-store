//! Command-line interface.

use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser, Subcommand};
use zs_app::config::Config;

#[derive(Debug, Parser)]
#[command(name = "zebra-store", version, about = "Zebra Store server")]
pub struct Cli {
    /// Path to the YAML configuration file.
    #[arg(short, long, default_value = "config.yml", global = true)]
    pub config: PathBuf,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Create a secure config.yml for a new installation.
    Init,
    /// Run the HTTP API and background worker (default).
    Serve,
    /// Write a consistent online copy of the SQLite database (`VACUUM INTO`);
    /// MySQL/PostgreSQL: use mysqldump / pg_dump.
    Backup {
        /// Target file; must not exist yet (default `data/backup/zebra-<UTC time>.db`).
        #[arg(long, short)]
        output: Option<PathBuf>,
    },
    /// Operator commands.
    #[command(subcommand)]
    Admin(AdminCommand),
}

#[derive(Debug, Subcommand)]
pub enum AdminCommand {
    /// List administrators.
    ListAdmins,
    /// Reset an administrator's password (all sessions are revoked). Without
    /// `--password` the new password is read from stdin.
    ResetPassword {
        #[arg(long)]
        username: String,
        #[arg(long)]
        password: Option<String>,
    },
    /// Disable an administrator's two-factor authentication (`reset2fa` is kept
    /// as an alias; the handbook documents `reset-2fa`, live QA I-6).
    #[command(name = "reset-2fa", alias = "reset2fa")]
    Reset2fa {
        #[arg(long)]
        username: String,
    },
    /// List (and with `--apply` delete) stored settings keys the application does
    /// not know, e.g. keys written through the old unrestricted settings endpoint.
    PruneSettings {
        /// Delete the listed keys (default: dry run).
        #[arg(long)]
        apply: bool,
    },
}

/// Operator-facing text of an application error: the zh-CN message plus its key.
fn describe(e: &zs_domain::Error) -> String {
    let key = e.key();
    let text = zs_api::i18n::translate("zh-CN", key);
    if text == key {
        key.to_owned()
    } else {
        format!("{text} ({key})")
    }
}

/// `zebra-store backup` (live QA I-16): online SQLite copy without a `sqlite3` binary.
#[expect(clippy::print_stdout, reason = "CLI output for operators")]
pub async fn run_backup(cfg: Config, output: Option<PathBuf>) -> anyhow::Result<()> {
    let db = zs_infra::db::connect(&cfg.database)
        .await
        .context("connect database")?;
    let dest = output.unwrap_or_else(|| {
        PathBuf::from(format!(
            "data/backup/zebra-{}.db",
            chrono::Utc::now().format("%Y%m%d-%H%M%S")
        ))
    });
    zs_infra::db::backup::backup_sqlite(&db, &dest)
        .await
        .with_context(|| format!("backup to {}", dest.display()))?;
    println!("OK: database copied to {}", dest.display());
    Ok(())
}

/// Runs an operator subcommand (original `internal/admincmd`).
#[expect(clippy::print_stdout, reason = "CLI output for operators")]
pub async fn run_admin(cfg: Config, cmd: AdminCommand) -> anyhow::Result<()> {
    let db = zs_infra::db::connect(&cfg.database)
        .await
        .context("connect database")?;
    zs_infra::db::sync_schema(&db)
        .await
        .context("sync database schema")?;
    let ctx = zs_infra::wire::WireCtx::new(&db, &cfg);
    let identity = zs_infra::wire::identity::build(&ctx);
    match cmd {
        AdminCommand::ListAdmins => {
            let admins = identity.admin_auth.repo().list().await?;
            println!(
                "{:<6}{:<24}{:<10}{:<20}LAST_LOGIN",
                "ID", "USERNAME", "IS_SUPER", "2FA_ENABLED"
            );
            for a in admins {
                let totp = a.totp_enabled_at.map_or_else(
                    || "no".to_owned(),
                    |t| format!("yes ({})", t.format("%Y-%m-%d")),
                );
                let last = a.last_login_at.map_or_else(
                    || "-".to_owned(),
                    |t| t.format("%Y-%m-%d %H:%M:%S").to_string(),
                );
                println!(
                    "{:<6}{:<24}{:<10}{:<20}{last}",
                    a.id, a.username, a.is_super, totp
                );
            }
        }
        AdminCommand::Reset2fa { username } => {
            let admin = identity.admin_2fa.cli_reset(&username).await.map_err(|e| {
                anyhow::anyhow!("reset-2fa failed for {username:?}: {}", describe(&e))
            })?;
            println!(
                "OK: 2FA reset for admin id={} username={} at {}",
                admin.id,
                admin.username,
                chrono::Utc::now().to_rfc3339()
            );
        }
        AdminCommand::ResetPassword { username, password } => {
            let password = match password {
                Some(p) => p,
                None => read_password_line()?,
            };
            let admin = identity
                .admin_auth
                .cli_reset_password(&username, &password)
                .await
                .map_err(|e| {
                    anyhow::anyhow!("reset-password failed for {username:?}: {}", describe(&e))
                })?;
            println!(
                "OK: password reset for admin id={} username={} at {}",
                admin.id,
                admin.username,
                chrono::Utc::now().to_rfc3339()
            );
            println!("All existing sessions of this administrator were revoked.");
        }
        AdminCommand::PruneSettings { apply } => {
            let store = zs_infra::db::repo::settings::SeaSettingsStore::new(db.clone());
            let unknown =
                zs_app::content::settings::SettingsService::unknown_keys(store.keys().await?);
            if unknown.is_empty() {
                println!("OK: no unknown settings keys");
            }
            for key in unknown {
                if apply {
                    store.delete(&key).await?;
                    println!("deleted {key}");
                } else {
                    println!("unknown {key} (re-run with --apply to delete)");
                }
            }
        }
    }
    Ok(())
}

/// Reads one line from stdin (the password), without the trailing newline.
fn read_password_line() -> anyhow::Result<String> {
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .context("read password from stdin")?;
    let pwd = line.trim_end_matches(['\r', '\n']).to_owned();
    anyhow::ensure!(!pwd.is_empty(), "password must not be empty");
    Ok(pwd)
}

#[cfg(test)]
mod tests {
    use super::*;

    // QA-A06 (live QA I-6): the documented `reset-2fa` and the old `reset2fa` both parse.
    #[test]
    fn qa_a06_reset_2fa_names() {
        for name in ["reset-2fa", "reset2fa"] {
            let cli = Cli::try_parse_from(["zebra-store", "admin", name, "--username", "a"])
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(matches!(
                cli.command,
                Some(Command::Admin(AdminCommand::Reset2fa { ref username })) if username == "a"
            ));
        }
        let cli = Cli::try_parse_from(["zebra-store", "admin", "prune-settings", "--apply"])
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(matches!(
            cli.command,
            Some(Command::Admin(AdminCommand::PruneSettings { apply: true }))
        ));
        let cli = Cli::try_parse_from(["zebra-store", "backup", "-o", "/tmp/x.db"])
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(matches!(
            cli.command,
            Some(Command::Backup { output: Some(_) })
        ));
        let cli = Cli::try_parse_from(["zebra-store", "init"]).unwrap_or_else(|e| panic!("{e}"));
        assert!(matches!(cli.command, Some(Command::Init)));
    }

    // QA-A07: CLI errors show the translated message, not only the raw key.
    #[test]
    fn qa_a07_cli_errors_are_translated() {
        let e = zs_domain::Error::bad_request("error.user_not_found");
        assert_eq!(describe(&e), "用户不存在 (error.user_not_found)");
    }
}
