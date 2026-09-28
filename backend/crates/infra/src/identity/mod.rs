//! Identity adapters: SMTP mailer, captcha image renderer, Turnstile client,
//! Telegram/Google OIDC and JWKS clients.

pub mod captcha_image;
pub mod mail;
pub mod oauth;
pub mod turnstile;
