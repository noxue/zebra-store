//! HTTP serving with connection-level limits (MISC-01, original `http_service.go`:
//! `ReadHeaderTimeout 10s`, `IdleTimeout 120s`, `MaxHeaderBytes 1MB`).
//!
//! `axum::serve` exposes none of these knobs, so connections are served with hyper-util's
//! auto (HTTP/1 + h2c) builder:
//! - HTTP/1 request heads must arrive within [`Limits::header_read`]; hyper also applies
//!   this to the wait for the next request on a keep-alive connection, so idle HTTP/1
//!   connections close after it (stricter than the original's 120 s idle timeout);
//! - request heads are capped at [`Limits::max_header_bytes`] (HTTP 431 beyond);
//! - any connection with no read/write progress for [`Limits::idle`] is closed.
//!
//! Per-request time stays bounded by the `TimeoutLayer` in `main.rs`.

use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use axum::Router;
use axum::extract::ConnectInfo;
use hyper::body::Incoming;
use hyper_util::rt::{TokioExecutor, TokioIo, TokioTimer};
use hyper_util::server::conn::auto::Builder;
use hyper_util::server::graceful::GracefulShutdown;
use hyper_util::service::TowerToHyperService;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpListener;
use tokio::time::{Instant, Sleep};
use tower::ServiceExt;

/// Connection limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Time allowed to receive a complete request head (`ReadHeaderTimeout`).
    pub header_read: Duration,
    /// Time a connection may make no progress at all (`IdleTimeout`).
    pub idle: Duration,
    /// Largest accepted request head (`MaxHeaderBytes`).
    pub max_header_bytes: usize,
}

impl Limits {
    /// The original server's values.
    pub const ORIGINAL: Self = Self {
        header_read: Duration::from_secs(10),
        idle: Duration::from_secs(120),
        max_header_bytes: 1 << 20,
    };
}

/// hyper's lower bound for its read buffer.
const MIN_BUF_SIZE: usize = 8192;
/// Slack over `max_header_bytes`, like Go's `http.Server` (+4096 bytes).
const HEADER_SLACK: usize = 4096;

/// Serves `router` on `listener` until `shutdown` resolves, then waits for open
/// connections to finish their in-flight requests. Handlers see the peer address as
/// `ConnectInfo<SocketAddr>`.
pub async fn serve(
    listener: TcpListener,
    router: Router,
    limits: Limits,
    shutdown: impl Future<Output = ()>,
) {
    let mut builder = Builder::new(TokioExecutor::new());
    builder
        .http1()
        .timer(TokioTimer::new())
        .keep_alive(true)
        .header_read_timeout(limits.header_read)
        .max_buf_size((limits.max_header_bytes + HEADER_SLACK).max(MIN_BUF_SIZE));
    builder
        .http2()
        .timer(TokioTimer::new())
        .max_header_list_size(u32::try_from(limits.max_header_bytes).unwrap_or(u32::MAX));
    let graceful = GracefulShutdown::new();
    tokio::pin!(shutdown);
    loop {
        let (stream, peer) = tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok(conn) => conn,
                Err(error) => {
                    // Transient (e.g. EMFILE): back off briefly instead of spinning.
                    tracing::warn!(%error, "accept failed");
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    continue;
                }
            },
            () = &mut shutdown => break,
        };
        let service = router
            .clone()
            .map_request(move |mut req: axum::http::Request<Incoming>| {
                req.extensions_mut().insert(ConnectInfo::<SocketAddr>(peer));
                req
            });
        let io = TokioIo::new(IdleIo::new(stream, limits.idle));
        let conn = builder
            .serve_connection_with_upgrades(io, TowerToHyperService::new(service))
            .into_owned();
        let conn = graceful.watch(conn);
        tokio::spawn(async move {
            if let Err(error) = conn.await {
                tracing::debug!(%error, %peer, "connection closed with error");
            }
        });
    }
    drop(listener);
    graceful.shutdown().await;
}

/// A stream closed with `TimedOut` after `idle` without any read or write progress.
#[derive(Debug)]
struct IdleIo<T> {
    inner: T,
    idle: Duration,
    deadline: Pin<Box<Sleep>>,
}

impl<T> IdleIo<T> {
    fn new(inner: T, idle: Duration) -> Self {
        Self {
            inner,
            idle,
            deadline: Box::pin(tokio::time::sleep(idle)),
        }
    }

    fn touch(&mut self) {
        let next = Instant::now() + self.idle;
        self.deadline.as_mut().reset(next);
    }

    fn expired(&mut self, cx: &mut Context<'_>) -> bool {
        self.deadline.as_mut().poll(cx).is_ready()
    }
}

fn idle_error() -> io::Error {
    io::Error::new(io::ErrorKind::TimedOut, "connection idle timeout")
}

impl<T: AsyncRead + Unpin> AsyncRead for IdleIo<T> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let before = buf.filled().len();
        match Pin::new(&mut this.inner).poll_read(cx, buf) {
            Poll::Ready(result) => {
                if buf.filled().len() > before {
                    this.touch();
                }
                Poll::Ready(result)
            }
            Poll::Pending if this.expired(cx) => Poll::Ready(Err(idle_error())),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<T: AsyncWrite + Unpin> AsyncWrite for IdleIo<T> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        match Pin::new(&mut this.inner).poll_write(cx, buf) {
            Poll::Ready(Ok(n)) => {
                if n > 0 {
                    this.touch();
                }
                Poll::Ready(Ok(n))
            }
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Pending if this.expired(cx) => Poll::Ready(Err(idle_error())),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        match Pin::new(&mut this.inner).poll_write_vectored(cx, bufs) {
            Poll::Ready(Ok(n)) => {
                if n > 0 {
                    this.touch();
                }
                Poll::Ready(Ok(n))
            }
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Pending if this.expired(cx) => Poll::Ready(Err(idle_error())),
            Poll::Pending => Poll::Pending,
        }
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant as StdInstant;

    use axum::routing::get;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    use super::*;

    const TEST_LIMITS: Limits = Limits {
        header_read: Duration::from_millis(300),
        idle: Duration::from_millis(600),
        max_header_bytes: 16 * 1024,
    };

    async fn start(limits: Limits) -> (SocketAddr, tokio::sync::oneshot::Sender<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let router = Router::new().route(
            "/",
            get(|ConnectInfo(peer): ConnectInfo<SocketAddr>| async move { peer.ip().to_string() }),
        );
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(serve(listener, router, limits, async {
            let _ = rx.await;
        }));
        (addr, tx)
    }

    /// Reads until EOF / error; returns what was read and how long it took.
    async fn read_to_close(stream: &mut TcpStream, limit: Duration) -> (String, Duration) {
        let started = StdInstant::now();
        let mut out = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            match tokio::time::timeout(limit, stream.read(&mut buf)).await {
                Ok(Ok(0) | Err(_)) => break,
                Ok(Ok(n)) => out.extend_from_slice(&buf[..n]),
                Err(_) => panic!("connection still open after {limit:?}"),
            }
        }
        (
            String::from_utf8_lossy(&out).into_owned(),
            started.elapsed(),
        )
    }

    /// MISC-01: a client that never finishes its request head (Slowloris) is disconnected
    /// once the header read timeout elapses.
    #[tokio::test]
    async fn misc_01_slow_header_client_is_disconnected() {
        let (addr, _stop) = start(TEST_LIMITS).await;
        let mut stream = TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(b"GET / HTTP/1.1\r\nHost: x\r\nX-Slow: 1")
            .await
            .unwrap();
        let (_, took) = read_to_close(&mut stream, Duration::from_secs(5)).await;
        assert!(
            took >= Duration::from_millis(250),
            "closed too early: {took:?}"
        );
    }

    /// MISC-01: a complete request is served (with the peer address) on the same limits,
    /// and an oversized request head is refused.
    #[tokio::test]
    async fn misc_01_normal_requests_and_header_cap() {
        let (addr, _stop) = start(TEST_LIMITS).await;
        let mut stream = TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        let (text, _) = read_to_close(&mut stream, Duration::from_secs(5)).await;
        assert!(text.starts_with("HTTP/1.1 200"), "{text}");
        assert!(text.ends_with("127.0.0.1"), "{text}");

        let mut stream = TcpStream::connect(addr).await.unwrap();
        let big = format!(
            "GET / HTTP/1.1\r\nHost: x\r\nX-Big: {}\r\n\r\n",
            "a".repeat(64 * 1024)
        );
        let _ = stream.write_all(big.as_bytes()).await;
        let (text, _) = read_to_close(&mut stream, Duration::from_secs(5)).await;
        assert!(!text.starts_with("HTTP/1.1 200"), "{text}");
    }

    /// MISC-01: an idle keep-alive connection does not stay open indefinitely.
    #[tokio::test]
    async fn misc_01_idle_keep_alive_connection_is_closed() {
        let (addr, _stop) = start(TEST_LIMITS).await;
        let mut stream = TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n")
            .await
            .unwrap();
        let (text, took) = read_to_close(&mut stream, Duration::from_secs(5)).await;
        assert!(text.starts_with("HTTP/1.1 200"), "{text}");
        assert!(took < Duration::from_secs(2), "{took:?}");
    }

    /// Graceful shutdown stops accepting new connections.
    #[tokio::test]
    async fn shutdown_stops_accepting() {
        let (addr, stop) = start(TEST_LIMITS).await;
        stop.send(()).unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(TcpStream::connect(addr).await.is_err());
    }
}
