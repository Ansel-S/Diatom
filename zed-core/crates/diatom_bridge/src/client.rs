
use anyhow::{Context, Result};
use tokio::sync::mpsc;

use crate::protocol::{BrowserMessage, DevPanelMessage};
use crate::transport;

const CHAN_CAP: usize = 256;

/// Handle returned by [`BridgeClient::connect`].
pub struct BridgeClient {
    /// Messages arriving from the DevPanel (e.g. EvalJs, SlmRequest).
    pub inbound: mpsc::Receiver<DevPanelMessage>,
    /// Queue a message to be sent to the DevPanel.
    pub outbound: mpsc::Sender<BrowserMessage>,
}

impl BridgeClient {
    /// Connect to a DevPanel listening at `socket_path`.
    /// Retries up to `retries` times with 100 ms back-off (DevPanel may still
    /// be starting its GPUI event loop when Diatom calls connect).
    #[cfg(unix)]
    pub async fn connect(socket_path: &str, retries: u8) -> Result<Self> {
        use tokio::net::UnixStream;
        use tokio::time::{sleep, Duration};

        let mut stream = None;
        for attempt in 0..=retries {
            match UnixStream::connect(socket_path).await {
                Ok(s) => {
                    stream = Some(s);
                    break;
                }
                Err(e) if attempt < retries => {
                    log::debug!("[bridge-client] connect attempt {attempt} failed ({e}), retrying");
                    sleep(Duration::from_millis(100)).await;
                }
                Err(e) => return Err(e).context("connect to DevPanel socket"),
            }
        }
        let stream = stream.expect("loop exited only on success or error");

        let (inbound_tx, inbound_rx) = mpsc::channel::<DevPanelMessage>(CHAN_CAP);
        let (outbound_tx, outbound_rx) = mpsc::channel::<BrowserMessage>(CHAN_CAP);

        let (mut reader, mut writer) = tokio::io::split(stream);

        tokio::spawn(async move {
            loop {
                match transport::recv::<_, DevPanelMessage>(&mut reader).await {
                    Ok(Some(msg)) => {
                        if inbound_tx.send(msg).await.is_err() {
                            break;
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

        tokio::spawn(async move {
            let mut rx = outbound_rx;
            while let Some(msg) = rx.recv().await {
                if let Err(e) = transport::send(&mut writer, &msg).await {
                    log::error!("[bridge-client] send: {e}");
                    break;
                }
            }
        });

        Ok(Self {
            inbound: inbound_rx,
            outbound: outbound_tx,
        })
    }

    #[cfg(windows)]
    pub async fn connect(pipe_name: &str, retries: u8) -> Result<Self> {
        use tokio::net::windows::named_pipe::ClientOptions;
        use tokio::time::{sleep, Duration};

        let mut pipe = None;
        for attempt in 0..=retries {
            match ClientOptions::new().open(pipe_name) {
                Ok(p) => {
                    pipe = Some(p);
                    break;
                }
                Err(e) if attempt < retries => {
                    sleep(Duration::from_millis(100)).await;
                    let _ = e;
                }
                Err(e) => return Err(e).context("connect to DevPanel pipe"),
            }
        }
        let pipe = pipe.expect("loop exits on success or early return");

        use std::sync::Arc;
        use tokio::sync::Mutex;
        let pipe = Arc::new(Mutex::new(pipe));

        let (inbound_tx, inbound_rx) = mpsc::channel::<DevPanelMessage>(CHAN_CAP);
        let (outbound_tx, outbound_rx) = mpsc::channel::<BrowserMessage>(CHAN_CAP);

        let read_pipe = Arc::clone(&pipe);
        tokio::spawn(async move {
            loop {
                let mut guard = read_pipe.lock().await;
                match transport::recv::<_, DevPanelMessage>(&mut *guard).await {
                    Ok(Some(msg)) => {
                        drop(guard);
                        if inbound_tx.send(msg).await.is_err() {
                            break;
                        }
                    }
                    Ok(None) => break,
                    Err(e) => {
                        log::error!("[bridge-client] recv: {e}");
                        break;
                    }
                }
            }
        });

        let write_pipe = Arc::clone(&pipe);
        tokio::spawn(async move {
            let mut rx = outbound_rx;
            while let Some(msg) = rx.recv().await {
                let mut guard = write_pipe.lock().await;
                if let Err(e) = transport::send(&mut *guard, &msg).await {
                    log::error!("[bridge-client] send: {e}");
                    break;
                }
            }
        });

        Ok(Self {
            inbound: inbound_rx,
            outbound: outbound_tx,
        })
    }

    /// Convenience wrapper — fire-and-forget a message to the DevPanel.
    pub async fn send(&self, msg: BrowserMessage) -> Result<()> {
        self.outbound
            .send(msg)
            .await
            .context("DevPanel outbound channel closed")
    }
}

