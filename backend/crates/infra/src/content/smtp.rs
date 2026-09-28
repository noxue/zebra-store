//! [`SmtpSender`] over lettre, used by the admin SMTP test.

use std::time::Duration;

use async_trait::async_trait;
use lettre::message::Mailbox;
use lettre::message::header::ContentType;
use lettre::{AsyncTransport, Message};
use zs_domain::content::public::SmtpSender;
use zs_domain::identity::mailer::{self, Email};
use zs_domain::settings::schema::smtp::SmtpSetting;
use zs_domain::{Error, Result};

use crate::identity::mail::{is_recipient_rejected, message_id, smtp_transport};

/// SMTP connection/command timeout (DB-01: never hang on external I/O).
const SMTP_TIMEOUT: Duration = Duration::from_secs(15);

/// Sends mail with an explicit [`SmtpSetting`].
#[derive(Debug, Clone, Default)]
pub struct LettreSmtpSender;

fn message(s: &SmtpSetting, mail: &Email) -> Result<Message> {
    let from_addr = s
        .from
        .trim()
        .parse()
        .map_err(|_| Error::bad_request("error.email_service_not_configured"))?;
    let name = s.from_name.trim();
    let from = Mailbox::new((!name.is_empty()).then(|| name.to_owned()), from_addr);
    let to: Mailbox = mail
        .to
        .parse()
        .map_err(|_| Error::bad_request("error.email_invalid"))?;
    Message::builder()
        .from(from)
        .to(to)
        .subject(mail.subject.clone())
        .date_now()
        .message_id(Some(message_id(&s.from)))
        .header(ContentType::TEXT_PLAIN)
        .body(mail.body.clone())
        .map_err(Error::internal)
}

#[async_trait]
impl SmtpSender for LettreSmtpSender {
    async fn send(&self, s: &SmtpSetting, mail: &Email) -> Result<()> {
        let msg = message(s, mail)?;
        let port = u16::try_from(s.port).map_err(Error::internal)?;
        let transport = smtp_transport(
            &s.host,
            port,
            s.use_ssl,
            s.use_tls,
            &s.username,
            &s.password,
            SMTP_TIMEOUT,
        )?;
        match transport.send(msg).await {
            Ok(_) => Ok(()),
            Err(e) if is_recipient_rejected(&e.to_string()) => Err(mailer::recipient_rejected()),
            Err(e) => {
                tracing::warn!(error = %e, to = %mail.to, "smtp test send failed");
                Err(Error::internal(e))
            }
        }
    }
}
