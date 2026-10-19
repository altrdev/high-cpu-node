//! What is open on the server: the connections, counted from the moment they are accepted until they
//! close, and the requests being answered. The listener also stops accepting when the server starts
//! draining, so new connections are refused while the ones already open keep being served.
use crate::app::AppState;
use crate::resources::shutdown::{Open, Shutdown};
use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;

/// A connection that counts as open for as long as it lives
pub struct Counted {
    io: TcpStream,
    _open: Open,
}

impl AsyncRead for Counted {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.io).poll_read(cx, buf)
    }
}

impl AsyncWrite for Counted {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.io).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.io).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.io).poll_shutdown(cx)
    }

    fn poll_write_vectored(mut self: Pin<&mut Self>, cx: &mut Context<'_>, bufs: &[io::IoSlice<'_>]) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.io).poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        self.io.is_write_vectored()
    }
}

/// The listener of the server. When the draining begins it closes the port, so that a new
/// connection is refused, and the connections already accepted are served as before.
pub struct Listener {
    inner: Option<TcpListener>,
    draining: watch::Receiver<bool>,
    shutdown: Arc<Shutdown>,
}

impl Listener {
    pub fn new(listener: TcpListener, shutdown: Arc<Shutdown>) -> Listener {
        Listener { inner: Some(listener), draining: shutdown.subscribe(), shutdown }
    }
}

// An error of one connection that did not come to be accepted says nothing about the others
fn is_connection_error(e: &io::Error) -> bool {
    matches!(e.kind(), io::ErrorKind::ConnectionRefused | io::ErrorKind::ConnectionAborted | io::ErrorKind::ConnectionReset)
}

enum Event {
    Accepted(io::Result<(TcpStream, SocketAddr)>),
    Draining,
}

impl axum::serve::Listener for Listener {
    type Io = Counted;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Counted, SocketAddr) {
        loop {
            let Listener { inner, draining, shutdown } = &mut *self;
            let Some(listener) = inner.as_ref() else {
                // closed: nothing is accepted any more, and the server goes on with what it has
                std::future::pending::<()>().await;
                unreachable!("pending never returns");
            };
            let event = tokio::select! {
                accepted = listener.accept() => Event::Accepted(accepted),
                _ = draining.wait_for(|draining| *draining) => Event::Draining,
            };
            match event {
                Event::Accepted(Ok((io, addr))) => return (Counted { io, _open: shutdown.connection() }, addr),
                Event::Accepted(Err(e)) if is_connection_error(&e) => {}
                // out of descriptors or memory: wait, so that the loop does not spin
                Event::Accepted(Err(_)) => tokio::time::sleep(Duration::from_secs(1)).await,
                Event::Draining => self.inner = None,
            }
        }
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        match &self.inner {
            Some(listener) => listener.local_addr(),
            None => Err(io::Error::new(io::ErrorKind::NotConnected, "the listener is closed")),
        }
    }
}

/// Goes around the whole router: the request counts as open until its answer is written
pub async fn track(State(state): State<Arc<AppState>>, request: Request, next: Next) -> Response {
    let _open = state.shutdown.request();
    next.run(request).await
}
