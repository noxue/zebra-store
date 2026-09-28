//! Password strength policy (ported from the original `passwordpolicy`).

use crate::{Error, Result};

/// Configurable password strength rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PasswordPolicy {
    pub min_length: usize,
    pub require_upper: bool,
    pub require_lower: bool,
    pub require_number: bool,
    pub require_special: bool,
}

impl PasswordPolicy {
    /// Validates `password`; errors carry the original i18n keys (`error.password_*`).
    pub fn validate(&self, password: &str) -> Result<()> {
        if self.min_length == 0
            && !self.require_upper
            && !self.require_lower
            && !self.require_number
            && !self.require_special
        {
            return Ok(());
        }
        if self.min_length > 0 && password.chars().count() < self.min_length {
            return Err(Error::bad_request("error.password_min_length").arg(self.min_length));
        }
        let (mut upper, mut lower, mut number, mut special) = (false, false, false, false);
        for c in password.chars() {
            if c.is_uppercase() {
                upper = true;
            } else if c.is_lowercase() {
                lower = true;
            } else if c.is_numeric() {
                number = true;
            } else {
                special = true;
            }
        }
        let checks = [
            (self.require_upper && !upper, "error.password_require_upper"),
            (self.require_lower && !lower, "error.password_require_lower"),
            (
                self.require_number && !number,
                "error.password_require_number",
            ),
            (
                self.require_special && !special,
                "error.password_require_special",
            ),
        ];
        match checks.into_iter().find(|(failed, _)| *failed) {
            Some((_, key)) => Err(Error::bad_request(key)),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT: PasswordPolicy = PasswordPolicy {
        min_length: 8,
        require_upper: true,
        require_lower: true,
        require_number: true,
        require_special: false,
    };

    #[test]
    fn enforces_each_rule() {
        assert_eq!(
            DEFAULT.validate("Ab1").unwrap_err().key(),
            "error.password_min_length"
        );
        assert_eq!(
            DEFAULT.validate("abcdefg1").unwrap_err().key(),
            "error.password_require_upper"
        );
        assert_eq!(
            DEFAULT.validate("ABCDEFG1").unwrap_err().key(),
            "error.password_require_lower"
        );
        assert_eq!(
            DEFAULT.validate("Abcdefgh").unwrap_err().key(),
            "error.password_require_number"
        );
        assert!(DEFAULT.validate("Admin12345").is_ok());
    }
}
