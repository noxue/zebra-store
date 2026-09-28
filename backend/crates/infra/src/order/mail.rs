//! [`OrderMailer`] over SMTP (lettre): plain-text order e-mails, with the delivered content
//! as a `text/plain` attachment when it is large (NTF-10; lettre folds the base64 body so
//! no line exceeds the SMTP limit). Settings come from the identity group's SMTP mailer.

use async_trait::async_trait;
use lettre::message::header::ContentType;
use lettre::message::{Attachment, Mailbox, MultiPart, SinglePart};
use lettre::{AsyncTransport, Message};
use zs_domain::identity::email;
use zs_domain::identity::mailer;
use zs_domain::order::email::{OrderMail, OrderMailer};
use zs_domain::{Error, Result};

use crate::identity::mail::{SmtpMailer, is_recipient_rejected, message_id, smtp_transport};

/// SMTP [`OrderMailer`].
#[derive(Debug, Clone)]
pub struct SmtpOrderMailer {
    smtp: SmtpMailer,
}

impl SmtpOrderMailer {
    pub fn new(smtp: SmtpMailer) -> Self {
        Self { smtp }
    }
}

fn mailbox(address: &str, name: Option<&str>) -> Result<Mailbox> {
    let addr = address
        .trim()
        .parse()
        .map_err(|_| mailer::not_configured())?;
    let name = name
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_owned);
    Ok(Mailbox::new(name, addr))
}

fn build(from: Mailbox, mail: &OrderMail) -> Result<Message> {
    let id = message_id(from.email.as_ref());
    let to: Mailbox = mail
        .to
        .trim()
        .parse()
        .map_err(|_| Error::bad_request(email::KEY_EMAIL_INVALID))?;
    let mut builder = Message::builder()
        .from(from)
        .to(to)
        .subject(mail.subject.clone())
        .date_now()
        .message_id(Some(id));
    if let Some(reply) = mail
        .reply_to
        .as_deref()
        .and_then(|r| r.trim().parse::<Mailbox>().ok())
    {
        builder = builder.reply_to(reply);
    }
    match &mail.attachment {
        None => builder
            .header(ContentType::TEXT_PLAIN)
            .body(mail.body.clone())
            .map_err(Error::internal),
        Some((name, content)) => {
            let text = ContentType::parse("text/plain; charset=utf-8").map_err(Error::internal)?;
            builder
                .multipart(
                    MultiPart::mixed()
                        .singlepart(SinglePart::plain(mail.body.clone()))
                        .singlepart(
                            Attachment::new(name.clone()).body(content.clone().into_bytes(), text),
                        ),
                )
                .map_err(Error::internal)
        }
    }
}

#[async_trait]
impl OrderMailer for SmtpOrderMailer {
    async fn send(&self, mail: &OrderMail) -> Result<()> {
        if email::is_placeholder(&mail.to) {
            return Ok(());
        }
        let s = self.smtp.effective().await?;
        if !s.enabled {
            tracing::info!(to = %mail.to, subject = %mail.subject, "smtp disabled; order email not sent");
            return Ok(());
        }
        if s.host.trim().is_empty() || s.port == 0 || s.from.trim().is_empty() {
            return Err(mailer::not_configured());
        }
        let from_name = mail
            .from_name
            .clone()
            .unwrap_or_else(|| s.from_name.clone());
        let message = build(mailbox(&s.from, Some(&from_name))?, mail)?;
        let transport = smtp_transport(
            &s.host,
            s.port,
            s.use_ssl,
            s.use_tls,
            &s.username,
            &s.password,
            self.smtp.timeout(),
        )?;
        match transport.send(message).await {
            Ok(_) => Ok(()),
            Err(e) if is_recipient_rejected(&e.to_string()) => Err(mailer::recipient_rejected()),
            Err(e) => {
                tracing::warn!(error = %e, to = %mail.to, "smtp order mail failed");
                Err(Error::internal(e))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// NTF-10: the attachment round-trips and no line exceeds 998 characters.
    #[test]
    fn ntf_10_attachment_is_folded() {
        let content = "X".repeat(5000);
        let mail = OrderMail {
            to: "a@b.com".into(),
            subject: "s".into(),
            body: "body".into(),
            attachment: Some(("order_DJ1_delivery.txt".into(), content)),
            ..OrderMail::default()
        };
        let from = mailbox("shop@b.com", Some("Shop")).unwrap_or_else(|e| panic!("{e}"));
        let raw = build(from, &mail)
            .unwrap_or_else(|e| panic!("{e}"))
            .formatted();
        let text = String::from_utf8_lossy(&raw);
        assert!(text.contains("order_DJ1_delivery.txt"));
        assert!(text.contains("multipart/mixed"));
        assert!(text.lines().all(|l| l.len() <= 998));
    }
}
