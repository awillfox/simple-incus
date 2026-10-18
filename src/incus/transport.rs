use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;

use super::client::IncusError;
use crate::config::IncusEndpoint;

pub(crate) const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

trait Io: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Io for T {}

/// Boxed duplex byte stream (unix or tcp), usable by hyper and tokio-tungstenite.
pub struct BoxedIo(Box<dyn Io>);

impl AsyncRead for BoxedIo {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut *self.0).poll_read(cx, buf)
    }
}

impl AsyncWrite for BoxedIo {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut *self.0).poll_write(cx, buf)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut *self.0).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut *self.0).poll_shutdown(cx)
    }
}

fn unreachable(e: impl std::fmt::Display) -> IncusError {
    IncusError::Unreachable(e.to_string())
}

/// Opens a fresh connection to the endpoint (10 s connect timeout).
pub(crate) async fn connect(ep: &IncusEndpoint) -> Result<BoxedIo, IncusError> {
    let fut = async {
        match ep {
            IncusEndpoint::Tcp(addr) => {
                let s = TcpStream::connect(addr).await.map_err(unreachable)?;
                Ok(BoxedIo(Box::new(s)))
            }
            #[cfg(unix)]
            IncusEndpoint::Unix(path) => {
                let s = tokio::net::UnixStream::connect(path).await.map_err(unreachable)?;
                Ok(BoxedIo(Box::new(s)))
            }
            #[cfg(not(unix))]
            IncusEndpoint::Unix(_) => Err(IncusError::Unreachable(
                "unix sockets unsupported on this platform; use SI_INCUS=tcp://…".into(),
            )),
        }
    };
    tokio::time::timeout(CONNECT_TIMEOUT, fut)
        .await
        .map_err(|_| IncusError::Unreachable("connect timed out".into()))?
}
