//! bcrypt hashing on the blocking pool.

use zs_domain::{Error, Result};

/// bcrypt cost used by the original (`bcrypt.DefaultCost`).
const BCRYPT_COST: u32 = 10;

/// A syntactically valid bcrypt hash compared against when the account does not
/// exist, so failed logins take the same time either way.
pub const DUMMY_HASH: &str = "$2y$10$65NFOY77jA4fEN6IINV0x.IzxS3MuaxHBoljdzkj4KY9h1VDa1iia";

pub async fn hash(password: &str) -> Result<String> {
    let password = password.to_owned();
    tokio::task::spawn_blocking(move || bcrypt::hash(password, BCRYPT_COST))
        .await
        .map_err(Error::internal)?
        .map_err(Error::internal)
}

pub async fn verify(password: &str, hash: &str) -> bool {
    let (password, hash) = (password.to_owned(), hash.to_owned());
    tokio::task::spawn_blocking(move || bcrypt::verify(password, &hash).unwrap_or(false))
        .await
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn hash_verify_and_dummy_is_valid_bcrypt() {
        let h = hash("Admin12345").await.unwrap();
        assert!(verify("Admin12345", &h).await);
        assert!(!verify("wrong", &h).await);
        assert!(bcrypt::verify("dummy-password-for-timing", DUMMY_HASH).unwrap());
    }
}
