
use anyhow::{Context, Result};
use std::path::Path;
use tokio::sync::mpsc;

use crate::protocol::{BrowserMessage, DevPanelMessage};
use crate::transport;

/// Capacity of the inbound message channel.
const CHAN_CAP: usize = 256;

/// Handle returned by [`BridgeServer::start`].
pub struct BridgeServer {
    /// Receive BrowserMessages arriving from the Diatom shell.
    pub inbound: mpsc::Receiver<BrowserMessage>,
    /// Send DevPanelMessages back to the Diatom shell.
    pub outbound: mpsc::Sender<DevPanelMessage>,
}

impl BridgeServer {
    /// Bind to `socket_path` and begin accepting exactly one connection
    /// (Diatom and DevPanel are always 1:1).
    ///
    /// Returns immediately; the actual I/O runs on background tokio tasks.
    #[cfg(unix)]
    pub async fn start(socket_path: impl AsRef<Path>) -> Result<Self> {
        use tokio::net::UnixListener;

        let path = socket_path.as_ref();

        if path.exists() {
            std::fs::remove_file(path).context("remove stale socket")?;
        }

        let listener = UnixListener::bind(path).context("bind Unix socket")?;
        let (inbound_tx, inbound_rx) = mpsc::channel::<BrowserMessage>(CHAN_CAP);
        let (outbound_tx, outbound_rx) = mpsc::channel::<DevPanelMessage>(CHAN_CAP);

        tokio::spawn(accept_unix(listener, inbound_tx, outbound_rx));

        Ok(Self {
            inbound: inbound_rx,
            outbound: outbound_tx,
        })
    }

    #[cfg(windows)]
    pub async fn start(pipe_name: impl AsRef<str>) -> Result<Self> {
        use tokio::net::windows::named_pipe::ServerOptions;

        let name = pipe_name.as_ref();
        let server = ServerOptions::new()
            .first_pipe_instance(true)
            .create(name)
            .context("create named pipe")?;

        let (inbound_tx, inbound_rx) = mpsc::channel::<BrowserMessage>(CHAN_CAP);
        let (outbound_tx, outbound_rx) = mpsc::channel::<DevPanelMessage>(CHAN_CAP);

        tokio::spawn(accept_windows(server, inbound_tx, outbound_rx));

        Ok(Self {
            inbound: inbound_rx,
            outbound: outbound_tx,
        })
    }
}


#[cfg(unix)]
async fn accept_unix(
    listener: tokio::net::UnixListener,
    inbound_tx: mpsc::Sender<BrowserMessage>,
    mut outbound_rx: mpsc::Receiver<DevPanelMessage>,
) {
    let (stream, _addr) = match listener.accept().await {
        Ok(x) => x,
        Err(e) => {
            log::error!("[bridge-server] accept failed: {e}");
            return;
        }
    };

    let (mut reader, mut writer) = tokio::io::split(stream);

    let read_task = {
        let tx = inbound_tx.clone();
        tokio::spawn(async move {
            loop {
                match transport::recv::<_, BrowserMessage>(&mut reader).await {
                    Ok(Some(msg)) => {
                        if tx.send(msg).await.is_err() {
                            break; // DevPanel dropped the receiver — exit.
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
        })
    };

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


#[cfg(windows)]
async fn accept_windows(
    mut server: tokio::net::windows::named_pipe::NamedPipeServer,
    inbound_tx: mpsc::Sender<BrowserMessage>,
    mut outbound_rx: mpsc::Receiver<DevPanelMessage>,
) {
    if let Err(e) = server.connect().await {
        log::error!("[bridge-server] pipe connect failed: {e}");
        return;
    }

    use std::sync::Arc;
    use tokio::sync::Mutex;
    let pipe = Arc::new(Mutex::new(server));

    let read_pipe = Arc::clone(&pipe);
    let read_task = tokio::spawn(async move {
        loop {
            let mut guard = read_pipe.lock().await;
            match transport::recv::<_, BrowserMessage>(&mut *guard).await {
                Ok(Some(msg)) => {
                    drop(guard);
                    if inbound_tx.send(msg).await.is_err() {
                        break;
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    log::error!("[bridge-server] recv: {e}");
                    break;
                }
            }
        }
    });

    let write_pipe = Arc::clone(&pipe);
    let write_task = tokio::spawn(async move {
        while let Some(msg) = outbound_rx.recv().await {
            let mut guard = write_pipe.lock().await;
            if let Err(e) = transport::send(&mut *guard, &msg).await {
                log::error!("[bridge-server] send: {e}");
                break;
            }
        }
    });

    tokio::select! {
        _ = read_task  => {},
        _ = write_task => {},
    }
}

