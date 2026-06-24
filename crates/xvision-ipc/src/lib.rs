//! Cross-platform local IPC sockets.
//!
//! xvision uses two newline-delimited-JSON IPC channels over a local socket:
//!
//! * the **sidecar IPC** between `xvision-agent-client` and the `xvision-agentd`
//!   Node sidecar (a connect side plus two listen sides), and
//! * the **optimizer IPC** between `xvn optimize run` and the dashboard
//!   (a connect side plus a listen side).
//!
//! Both were written directly against `tokio::net::Unix{Stream,Listener}`, which
//! does not exist on Windows and broke the `x86_64-pc-windows-msvc` release
//! build. This crate abstracts the transport behind [`LocalStream`] /
//! [`LocalListener`] so the same newline-JSON code compiles and runs on:
//!
//! * **unix** — Unix domain sockets (filesystem paths), and
//! * **windows** — named pipes (`\\.\pipe\<name>`).
//!
//! The Node sidecar needs no change: Node's `net` module speaks named pipes
//! natively when handed a `\\.\pipe\…` path. Generate matching addresses on
//! both ends with [`local_socket_path`].

use std::io;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

/// Build a platform-appropriate local-socket address from a directory and file
/// name.
///
/// * **unix** — `dir.join(file_name)`, a filesystem socket path.
/// * **windows** — `\\.\pipe\<file_name>`. Named pipes live in a global
///   namespace rather than the filesystem, so `dir` is ignored; pass the same
///   `file_name` on both ends so the client and server addresses match.
///
/// `file_name` may contain `.` (e.g. `agentd-01H….ev.sock`) — valid in a pipe
/// name — but must not contain a path separator on windows.
pub fn local_socket_path(dir: &Path, file_name: &str) -> PathBuf {
    #[cfg(unix)]
    {
        dir.join(file_name)
    }
    #[cfg(windows)]
    {
        let _ = dir;
        PathBuf::from(format!(r"\\.\pipe\{file_name}"))
    }
}

#[cfg(unix)]
type Inner = tokio::net::UnixStream;

#[cfg(windows)]
enum Inner {
    Client(tokio::net::windows::named_pipe::NamedPipeClient),
    Server(tokio::net::windows::named_pipe::NamedPipeServer),
}

/// A connected duplex local-IPC stream — a Unix socket (unix) or a named-pipe
/// instance (windows). Implements [`AsyncRead`] + [`AsyncWrite`].
pub struct LocalStream(Inner);

impl LocalStream {
    /// Connect to the local socket / named pipe at `path`.
    ///
    /// On windows a freshly-`accept`ed server may not yet have re-created its
    /// next pipe instance, so a transient `ERROR_PIPE_BUSY` is retried briefly.
    pub async fn connect(path: impl AsRef<Path>) -> io::Result<Self> {
        #[cfg(unix)]
        {
            Ok(LocalStream(tokio::net::UnixStream::connect(path).await?))
        }
        #[cfg(windows)]
        {
            use tokio::net::windows::named_pipe::ClientOptions;
            const ERROR_PIPE_BUSY: i32 = 231;
            let name = path.as_ref().as_os_str().to_owned();
            loop {
                match ClientOptions::new().open(&name) {
                    Ok(client) => return Ok(LocalStream(Inner::Client(client))),
                    Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY) => {
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    }
                    Err(e) => return Err(e),
                }
            }
        }
    }

    /// Split into owned read/write halves (e.g. to read and write from
    /// independently-locked tasks). Mirrors `UnixStream::into_split`.
    pub fn into_split(
        self,
    ) -> (
        tokio::io::ReadHalf<LocalStream>,
        tokio::io::WriteHalf<LocalStream>,
    ) {
        tokio::io::split(self)
    }
}

impl AsyncRead for LocalStream {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        #[cfg(unix)]
        {
            Pin::new(&mut this.0).poll_read(cx, buf)
        }
        #[cfg(windows)]
        {
            match &mut this.0 {
                Inner::Client(c) => Pin::new(c).poll_read(cx, buf),
                Inner::Server(s) => Pin::new(s).poll_read(cx, buf),
            }
        }
    }
}

impl AsyncWrite for LocalStream {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        #[cfg(unix)]
        {
            Pin::new(&mut this.0).poll_write(cx, buf)
        }
        #[cfg(windows)]
        {
            match &mut this.0 {
                Inner::Client(c) => Pin::new(c).poll_write(cx, buf),
                Inner::Server(s) => Pin::new(s).poll_write(cx, buf),
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        #[cfg(unix)]
        {
            Pin::new(&mut this.0).poll_flush(cx)
        }
        #[cfg(windows)]
        {
            match &mut this.0 {
                Inner::Client(c) => Pin::new(c).poll_flush(cx),
                Inner::Server(s) => Pin::new(s).poll_flush(cx),
            }
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        #[cfg(unix)]
        {
            Pin::new(&mut this.0).poll_shutdown(cx)
        }
        #[cfg(windows)]
        {
            match &mut this.0 {
                Inner::Client(c) => Pin::new(c).poll_shutdown(cx),
                Inner::Server(s) => Pin::new(s).poll_shutdown(cx),
            }
        }
    }
}

/// Accepts inbound [`LocalStream`] connections on a local socket / named pipe.
pub struct LocalListener {
    #[cfg(unix)]
    listener: tokio::net::UnixListener,
    #[cfg(windows)]
    name: std::ffi::OsString,
    #[cfg(windows)]
    next: Option<tokio::net::windows::named_pipe::NamedPipeServer>,
}

impl LocalListener {
    /// Bind a listener at `path`.
    ///
    /// On unix a stale socket file from an unclean shutdown is removed
    /// best-effort first (so `bind` doesn't fail with `EADDRINUSE`). On windows
    /// the first pipe instance is created eagerly.
    pub fn bind(path: impl AsRef<Path>) -> io::Result<Self> {
        #[cfg(unix)]
        {
            let _ = std::fs::remove_file(path.as_ref());
            Ok(LocalListener {
                listener: tokio::net::UnixListener::bind(path)?,
            })
        }
        #[cfg(windows)]
        {
            use tokio::net::windows::named_pipe::ServerOptions;
            let name = path.as_ref().as_os_str().to_owned();
            let server = ServerOptions::new().first_pipe_instance(true).create(&name)?;
            Ok(LocalListener {
                name,
                next: Some(server),
            })
        }
    }

    /// Wait for and accept the next inbound connection.
    pub async fn accept(&mut self) -> io::Result<LocalStream> {
        #[cfg(unix)]
        {
            let (stream, _addr) = self.listener.accept().await?;
            Ok(LocalStream(stream))
        }
        #[cfg(windows)]
        {
            use tokio::net::windows::named_pipe::ServerOptions;
            // `next` is always Some between accepts (set in bind and re-armed
            // below); a connected instance becomes the returned stream while a
            // fresh instance is armed for the subsequent accept.
            let server = self
                .next
                .take()
                .expect("LocalListener invariant: a pending pipe instance is always armed");
            server.connect().await?;
            self.next = Some(ServerOptions::new().create(&self.name)?);
            Ok(LocalStream(Inner::Server(server)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    #[test]
    fn local_socket_path_is_platform_appropriate() {
        let p = local_socket_path(Path::new("/var/run/xvn"), "agentd-abc.ev.sock");
        #[cfg(unix)]
        assert_eq!(p, PathBuf::from("/var/run/xvn/agentd-abc.ev.sock"));
        #[cfg(windows)]
        assert_eq!(p, PathBuf::from(r"\\.\pipe\agentd-abc.ev.sock"));
    }

    /// Full connect → accept → newline-JSON round-trip over the abstraction,
    /// exercising both `into_split` (server) and direct duplex (client). On
    /// unix this validates the real Unix-socket path; on windows it validates
    /// the named-pipe path.
    #[tokio::test]
    async fn round_trip_request_response() {
        let dir = tempfile::tempdir().unwrap();
        let addr = local_socket_path(dir.path(), "xvision-ipc-test.sock");

        let mut listener = LocalListener::bind(&addr).unwrap();

        let server = tokio::spawn(async move {
            let conn = listener.accept().await.unwrap();
            let (r, mut w) = conn.into_split();
            let mut br = BufReader::new(r);
            let mut line = String::new();
            br.read_line(&mut line).await.unwrap();
            assert_eq!(line, "{\"ping\":1}\n");
            w.write_all(b"{\"pong\":1}\n").await.unwrap();
            w.flush().await.unwrap();
        });

        let mut client = LocalStream::connect(&addr).await.unwrap();
        client.write_all(b"{\"ping\":1}\n").await.unwrap();
        client.flush().await.unwrap();

        let mut br = BufReader::new(client);
        let mut reply = String::new();
        br.read_line(&mut reply).await.unwrap();
        assert_eq!(reply, "{\"pong\":1}\n");

        server.await.unwrap();
    }
}
