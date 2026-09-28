//! Test support shared by unit and integration tests (never compiled into production
//! builds): a minimal in-process SMTP server that records what clients send.

use std::sync::{Arc, Mutex};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

/// One message accepted by [`MockSmtp`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SmtpMessage {
    /// `AUTH` mechanism the client chose (`LOGIN` / `PLAIN`), empty when it did not log in.
    pub auth: String,
    /// Decoded `(username, password)` of the login.
    pub credentials: (String, String),
    pub mail_from: String,
    pub rcpt_to: Vec<String>,
    /// Raw `DATA` (headers and body, dot-unstuffed).
    pub data: String,
}

impl SmtpMessage {
    /// Value of the first header `name` (case-insensitive), unfolded.
    pub fn header(&self, name: &str) -> Option<String> {
        let head = self.data.split("\r\n\r\n").next().unwrap_or_default();
        let mut out: Option<String> = None;
        for line in head.split("\r\n") {
            if line.starts_with([' ', '\t']) {
                if let Some(v) = out.as_mut() {
                    v.push_str(line.trim_start());
                }
                continue;
            }
            if out.is_some() {
                break;
            }
            if let Some((k, v)) = line.split_once(':')
                && k.eq_ignore_ascii_case(name)
            {
                out = Some(v.trim().to_owned());
            }
        }
        out
    }

    /// The body after the header block.
    pub fn body(&self) -> &str {
        self.data.split_once("\r\n\r\n").map_or("", |(_, b)| b)
    }
}

/// A local SMTP server: advertises the given `AUTH` mechanisms (e.g. `"LOGIN"` or
/// `"PLAIN LOGIN"`, empty for none) and accepts every message.
#[derive(Debug, Clone)]
pub struct MockSmtp {
    pub port: u16,
    messages: Arc<Mutex<Vec<SmtpMessage>>>,
}

impl MockSmtp {
    /// Starts the server on `127.0.0.1:<random port>`.
    ///
    /// # Panics
    /// When the listener cannot be bound (test environment problem).
    pub async fn start(auth_mechanisms: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .unwrap_or_else(|e| panic!("mock smtp bind: {e}"));
        let port = listener
            .local_addr()
            .unwrap_or_else(|e| panic!("mock smtp addr: {e}"))
            .port();
        let messages = Arc::new(Mutex::new(Vec::new()));
        let store = messages.clone();
        let mechanisms = auth_mechanisms.to_owned();
        tokio::spawn(async move {
            while let Ok((socket, _)) = listener.accept().await {
                let store = store.clone();
                let mechanisms = mechanisms.clone();
                tokio::spawn(async move {
                    let _ = session(socket, &mechanisms, &store).await;
                });
            }
        });
        Self { port, messages }
    }

    /// Messages received so far.
    ///
    /// # Panics
    /// When the recording mutex is poisoned.
    pub fn messages(&self) -> Vec<SmtpMessage> {
        self.messages
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

fn decode(b64: &str) -> String {
    STANDARD
        .decode(b64.trim())
        .map(|v| String::from_utf8_lossy(&v).into_owned())
        .unwrap_or_default()
}

async fn session(
    socket: tokio::net::TcpStream,
    mechanisms: &str,
    store: &Mutex<Vec<SmtpMessage>>,
) -> std::io::Result<()> {
    let (read, mut write) = socket.into_split();
    let mut lines = BufReader::new(read).lines();
    write.write_all(b"220 mock ESMTP ready\r\n").await?;
    let mut msg = SmtpMessage::default();
    let mut auth = (String::new(), (String::new(), String::new()));
    while let Some(line) = lines.next_line().await? {
        let upper = line.to_ascii_uppercase();
        if upper.starts_with("EHLO") || upper.starts_with("HELO") {
            let mut reply = String::from("250-mock\r\n250-8BITMIME\r\n");
            if !mechanisms.is_empty() {
                reply.push_str(&format!("250-AUTH {mechanisms}\r\n"));
            }
            reply.push_str("250 SIZE 10485760\r\n");
            write.write_all(reply.as_bytes()).await?;
        } else if upper.starts_with("AUTH LOGIN") {
            // Username may come as an initial response.
            let user = match line.split_whitespace().nth(2) {
                Some(initial) => decode(initial),
                None => {
                    write.write_all(b"334 VXNlcm5hbWU6\r\n").await?;
                    decode(&lines.next_line().await?.unwrap_or_default())
                }
            };
            write.write_all(b"334 UGFzc3dvcmQ6\r\n").await?;
            let pass = decode(&lines.next_line().await?.unwrap_or_default());
            auth = ("LOGIN".into(), (user, pass));
            write.write_all(b"235 2.7.0 ok\r\n").await?;
        } else if upper.starts_with("AUTH PLAIN") {
            let payload = match line.split_whitespace().nth(2) {
                Some(p) => p.to_owned(),
                None => {
                    write.write_all(b"334 \r\n").await?;
                    lines.next_line().await?.unwrap_or_default()
                }
            };
            let decoded = decode(&payload);
            let mut parts = decoded.split('\0').skip(1);
            let user = parts.next().unwrap_or_default().to_owned();
            let pass = parts.next().unwrap_or_default().to_owned();
            auth = ("PLAIN".into(), (user, pass));
            write.write_all(b"235 2.7.0 ok\r\n").await?;
        } else if upper.starts_with("MAIL FROM:") {
            msg = SmtpMessage {
                auth: auth.0.clone(),
                credentials: auth.1.clone(),
                mail_from: line[10..].trim().to_owned(),
                ..SmtpMessage::default()
            };
            write.write_all(b"250 ok\r\n").await?;
        } else if upper.starts_with("RCPT TO:") {
            msg.rcpt_to.push(line[8..].trim().to_owned());
            write.write_all(b"250 ok\r\n").await?;
        } else if upper == "DATA" {
            write.write_all(b"354 go ahead\r\n").await?;
            let mut data = String::new();
            while let Some(l) = lines.next_line().await? {
                if l == "." {
                    break;
                }
                data.push_str(l.strip_prefix('.').unwrap_or(&l));
                data.push_str("\r\n");
            }
            msg.data = data;
            store
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(std::mem::take(&mut msg));
            write.write_all(b"250 queued\r\n").await?;
        } else if upper == "QUIT" {
            write.write_all(b"221 bye\r\n").await?;
            break;
        } else {
            write.write_all(b"250 ok\r\n").await?;
        }
    }
    Ok(())
}
