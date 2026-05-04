//! DevPanel-side bridge server.
//!
//! `BridgeServer` is used by the `diatom-devpanel` process. It binds the Unix
//! socket, accepts exactly one connection from the Diatom backend, completes
//! the authentication handshake, and then forwards messages in both directions.

use anyhow::{Context, Result, bail};
use std::path::Path;
use tokio::sync::mpsc;
use tokio::time::{Duration, timeout};

use crate::protocol::{BrowserMessage, DevPanelMessage, HANDSHAKE_TIMEOUT_MS, HandshakeMessage};
use crate::transport;

/// Capacity of the inbound message channel.
const CHAN_CAP: usize = 256;

/// Handle returned by [`BridgeServer::start`].
pub struct BridgeServer {
    /// Receive BrowserMessages arriving from the Diatom backend.
    pub inbound: mpsc::Receiver<BrowserMessage>,
    /// Send DevPanelMessages back to the Diatom backend.
    pub outbound: mpsc::Sender<DevPanelMessage>,
}

impl BridgeServer {
    /// Bind to `socket_path` and begin accepting exactly one connection
    /// (Diatom and DevPanel are always 1:1).
    #[cfg(unix)]
    pub async fn start(socket_path: impl AsRef<Path>, auth_token: String) -> Result<Self> {
        use tokio::net::UnixListener;

        let path = socket_path.as_ref();

        if path.exists() {
            std::fs::remove_file(path).context("remove stale socket")?;
        }

        let listener = UnixListener::bind(path).context("bind Unix socket")?;

        // Restrict socket to owner only
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                .context("set socket permissions")?;
        }

        let (inbound_tx, inbound_rx) = mpsc::channel::<BrowserMessage>(CHAN_CAP);
        let (outbound_tx, outbound_rx) = mpsc::channel::<DevPanelMessage>(CHAN_CAP);

        tokio::spawn(accept_unix(listener, auth_token, inbound_tx, outbound_rx));

        Ok(Self {
            inbound: inbound_rx,
            outbound: outbound_tx,
        })
    }

    #[cfg(windows)]
    pub async fn start(pipe_name: impl AsRef<str>, auth_token: String) -> Result<Self> {
        use tokio::net::windows::named_pipe::ServerOptions;

        let name = pipe_name.as_ref();
        let server = ServerOptions::new()
            .first_pipe_instance(true)
            .create(name)
            .context("create named pipe")?;

        let (inbound_tx, inbound_rx) = mpsc::channel::<BrowserMessage>(CHAN_CAP);
        let (outbound_tx, outbound_rx) = mpsc::channel::<DevPanelMessage>(CHAN_CAP);

        tokio::spawn(accept_windows(server, auth_token, inbound_tx, outbound_rx));

        Ok(Self {
            inbound: inbound_rx,
            outbound: outbound_tx,
        })
    }
}

// ── Handshake helper ──────────────────────────────────────────────────────────

async fn perform_handshake<R, W>(reader: &mut R, writer: &mut W, auth_token: &str) -> Result<bool>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    let deadline = Duration::from_millis(HANDSHAKE_TIMEOUT_MS);

    // Step 1 — send Challenge.
    transport::send(writer, &HandshakeMessage::Challenge)
        .await
        .context("handshake: send Challenge")?;

    // Step 2 — wait for Response.
    let frame = timeout(deadline, transport::recv::<_, HandshakeMessage>(reader))
        .await
        .context("handshake: timeout waiting for Response")?
        .context("handshake: recv Response")?;

    let Some(msg) = frame else {
        bail!("handshake: peer closed connection before sending Response");
    };

    // Step 3 — validate token in constant time.
    let token_ok = match msg {
        HandshakeMessage::Response { token } => {
            constant_time_eq(token.as_bytes(), auth_token.as_bytes())
        }
        other => {
            log::warn!("[bridge-server] unexpected handshake frame: {other:?}");
            false
        }
    };

    // Step 4 — send verdict.
    if token_ok {
        transport::send(writer, &HandshakeMessage::Accepted)
            .await
            .context("handshake: send Accepted")?;
        Ok(true)
    } else {
        let _ = transport::send(
            writer,
            &HandshakeMessage::Rejected {
                reason: "invalid token".to_owned(),
            },
        )
        .await;
        log::warn!("[bridge-server] authentication failed — connection rejected");
        Ok(false)
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

// ── Unix accept loop ──────────────────────────────────────────────────────────

#[cfg(unix)]
async fn accept_unix(
    listener: tokio::net::UnixListener,
    auth_token: String,
    inbound_tx: mpsc::Sender<BrowserMessage>,
    outbound_rx: mpsc::Receiver<DevPanelMessage>,
) {
    let (stream, _addr) = match listener.accept().await {
        Ok(x) => x,
        Err(e) => {
            log::error!("[bridge-server] accept failed: {e}");
            return;
        }
    };

    let (mut reader, mut writer) = stream.into_split();

    match perform_handshake(&mut reader, &mut writer, &auth_token).await {
        Ok(true) => log::info!("[bridge-server] handshake accepted"),
        Ok(false) => {
            log::warn!("[bridge-server] handshake rejected — dropping connection");
            return;
        }
        Err(e) => {
            log::error!("[bridge-server] handshake error: {e}");
            return;
        }
    }

    run_io_loop(reader, writer, inbound_tx, outbound_rx).await;
}

// ── Windows accept loop ───────────────────────────────────────────────────────

#[cfg(windows)]
async fn accept_windows(
    server: tokio::net::windows::named_pipe::NamedPipeServer,
    auth_token: String,
    inbound_tx: mpsc::Sender<BrowserMessage>,
    outbound_rx: mpsc::Receiver<DevPanelMessage>,
) {
    if let Err(e) = server.connect().await {
        log::error!("[bridge-server] pipe connect failed: {e}");
        return;
    }

    let (mut reader, mut writer) = tokio::io::split(server);

    match perform_handshake(&mut reader, &mut writer, &auth_token).await {
        Ok(true) => log::info!("[bridge-server] handshake accepted"),
        Ok(false) => {
            log::warn!("[bridge-server] handshake rejected — dropping connection");
            return;
        }
        Err(e) => {
            log::error!("[bridge-server] handshake error: {e}");
            return;
        }
    }

    run_io_loop(reader, writer, inbound_tx, outbound_rx).await;
}

// ── Shared post-handshake I/O loop ───────────────────────────────────────────

async fn run_io_loop<R, W>(
    mut reader: R,
    mut writer: W,
    inbound_tx: mpsc::Sender<BrowserMessage>,
    mut outbound_rx: mpsc::Receiver<DevPanelMessage>,
) where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let read_task = tokio::spawn(async move {
        loop {
            match transport::recv::<_, BrowserMessage>(&mut reader).await {
                Ok(Some(msg)) => {
                    if inbound_tx.send(msg).await.is_err() {
                        break;
                    }
                }
                Ok(None) => {
                    log::info!("[bridge-server] peer closed connection");
                    break;
                }
                Err(e) => {
                    log::error!("[bridge-server] recv error: {e}");
                    break;
                }
            }
        }
    });

    let write_task = tokio::spawn(async move {
        while let Some(msg) = outbound_rx.recv().await {
            if let Err(e) = transport::send(&mut writer, &msg).await {
                log::error!("[bridge-server] send error: {e}");
                break;
            }
        }
    });

    tokio::select! {
        _ = read_task  => {},
        _ = write_task => {},
    }
}
