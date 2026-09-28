//! [`EmailSender`] for admin notifications on top of the identity SMTP mailer:
//! unlike verification mail, a disabled or unconfigured SMTP service is an error
//! so the delivery is logged as failed (original `SendCustomEmail`).

use async_trait::async_trait;
use zs_domain::identity::email;
use zs_domain::identity::mailer::{Email, Mailer};
use zs_domain::notify::ports::EmailSender;
use zs_domain::{Error, Result};

use crate::identity::mail::SmtpMailer;

/// Admin notification mailer.
#[derive(Debug, Clone)]
pub struct NotifyMailer {
    smtp: SmtpMailer,
}

impl NotifyMailer {
    pub fn new(smtp: SmtpMailer) -> Self {
        Self { smtp }
    }
}

#[async_trait]
impl EmailSender for NotifyMailer {
    async fn send(&self, to: &str, subject: &str, body: &str) -> Result<()> {
        let to = email::normalize(to)?;
        if email::is_placeholder(&to) {
            return Err(Error::bad_request(email::KEY_EMAIL_INVALID));
        }
        let settings = self.smtp.effective().await?;
        if !settings.enabled {
            return Err(Error::internal_msg("email service disabled")
                .or_internal("error.email_service_disabled"));
        }
        self.smtp
            .send(&Email {
                to,
                subject: subject.to_owned(),
                body: body.to_owned(),
                ..Email::default()
            })
            .await
    }
}
