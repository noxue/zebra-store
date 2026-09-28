//! Outbound adapters of the notify group: SSRF-safe HTTP, Telegram Bot API,
//! Feishu and the admin notification mailer.

pub mod email;
pub mod feishu;
pub mod safe_http;
pub mod telegram;
