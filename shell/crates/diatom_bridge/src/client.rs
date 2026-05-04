//! Diatom-backend-side bridge client.
//!
//! `BridgeClient` is used by the main `diatom` (Tauri) process. It connects
//! to the DevPanel's Unix socket, completes the authentication handshake, and
//! then forwards messages in both directions.

use anyhow::{Context, Result, bail};
use tokio::sync::mpsc;
use tokio::time::{Duration, timeout};

use crate::protocol::{BrowserMessage, DevPanelMessage, HANDSHAKE_TIMEOUT_MS, HandshakeMessage};
use crate::transport;

const CHAN_CAP: usize = 256;

/// Handle returned by [`BridgeClient::connect`].
pub struct BridgeClient {
    /// Messages arriving from the DevPanel (e.g. `EvalJs`, `SlmRequest`).
    pub inbound: mpsc::Receiver<DevPanelMessage>,
    /// Queue a message to be sent to the DevPanel.
    pub outbound: mpsc::Sender<BrowserMessage>,
}

impl BridgeClient {
    /// Connect to a DevPanel listening at `socket_path`.
    #[cfg(unix)]
    pub async fn connect(socket_path: &str, auth_token: &str, mut retries: u8) -> Result<Self> {
        use tokio::net::UnixStream;
        use tokio::time::sleep;

        let stream = loop {
            match UnixStream::connect(socket_path).await {
                Ok(s) => break s,
                Err(e) if retries > 0 => {
                    log::debug!("[bridge-client] connect failed ({e}), retrying...");
                    sleep(Duration::from_millis(100)).await;
                    retries -= 1;
                }
                Err(e) => return Err(e).context("connect to DevPanel socket"),
            }
        };

        let (mut reader, mut writer) = stream.into_split();

        // --- Handshake before any message traffic ---
        perform_handshake(&mut reader, &mut writer, auth_token).await?;

        let (inbound, outbound) = spawn_io_tasks(reader, writer);

        Ok(Self { inbound, outbound })
    }

    #[cfg(windows)]
    pub async fn connect(pipe_name: &str, auth_token: &str, mut retries: u8) -> Result<Self> {
        use tokio::net::windows::named_pipe::ClientOptions;
        use tokio::time::sleep;

        let pipe = loop {
            match ClientOptions::new().open(pipe_name) {
                Ok(p) => break p,
                Err(e) if retries > 0 => {
                    sleep(Duration::from_millis(100)).await;
                    retries -= 1;
                }
                Err(e) => return Err(e).context("connect to DevPanel pipe"),
            }
        };

        let (mut reader, mut writer) = tokio::io::split(pipe);

        perform_handshake(&mut reader, &mut writer, auth_token).await?;

        let (inbound, outbound) = spawn_io_tasks(reader, writer);

        Ok(Self { inbound, outbound })
    }

    /// Convenience wrapper — fire-and-forget a message to the DevPanel.
    pub async fn send(&self, msg: BrowserMessage) -> Result<()> {
        self.outbound
            .send(msg)
            .await
            .context("DevPanel outbound channel closed")
    }
}

// ── Common IO Tasks Spawner ──────────────────────────────────────────────

fn spawn_io_tasks<R, W>(
    mut reader: R,
    mut writer: W,
) -> (mpsc::Receiver<DevPanelMessage>, mpsc::Sender<BrowserMessage>)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let (inbound_tx, inbound_rx) = mpsc::channel::<DevPanelMessage>(CHAN_CAP);
    let (outbound_tx, outbound_rx) = mpsc::channel::<BrowserMessage>(CHAN_CAP);

    // Read task
    tokio::spawn(async move {
        loop {
            match transport::recv::<_, DevPanelMessage>(&mut reader).await {
                Ok(Some(msg)) => {
                    if inbound_tx.send(msg).await.is_err() {
                        break; // Receiver dropped, exit cleanly
                    }
                }
                Ok(None) => {
                    log::info!("[bridge-client] DevPanel disconnected");
                    break;
                }
                Err(e) => {
                    log::error!("[bridge-client] recv: {e}");
                    break;
                }
            }
        }
    });

    // Write task
    tokio::spawn(async move {
        let mut rx = outbound_rx;
        while let Some(msg) = rx.recv().await {
            if let Err(e) = transport::send(&mut writer, &msg).await {
                log::error!("[bridge-client] send: {e}");
                break;
            }
        }
    });

    (inbound_rx, outbound_tx)
}

// ── Handshake helper (client side) ───────────────────────────────────────────

async fn perform_handshake<R, W>(reader: &mut R, writer: &mut W, auth_token: &str) -> Result<()>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    let deadline = Duration::from_millis(HANDSHAKE_TIMEOUT_MS);

    // Step 1 — wait for Challenge.
    let frame = timeout(deadline, transport::recv::<_, HandshakeMessage>(reader))
        .await
        .context("handshake: timeout waiting for Challenge")?
        .context("handshake: recv Challenge")?;

    match frame {
        Some(HandshakeMessage::Challenge) => {}
        Some(other) => bail!("handshake: expected Challenge, got {other:?}"),
        None => bail!("handshake: server closed connection before Challenge"),
    }

    // Step 2 — send Response.
    transport::send(
        writer,
        &HandshakeMessage::Response {
            token: auth_token.to_owned(),
        },
    )
    .await
    .context("handshake: send Response")?;

    // Step 3 — wait for verdict.
    let frame = timeout(deadline, transport::recv::<_, HandshakeMessage>(reader))
        .await
        .context("handshake: timeout waiting for verdict")?
        .context("handshake: recv verdict")?;

    match frame {
        Some(HandshakeMessage::Accepted) => {
            log::debug!("[bridge-client] handshake accepted");
            Ok(())
        }
        Some(HandshakeMessage::Rejected { reason }) => {
            bail!("handshake rejected by server: {reason}")
        }
        Some(other) => bail!("handshake: unexpected verdict frame: {other:?}"),
        None => bail!("handshake: server closed connection before verdict"),
    }
}
