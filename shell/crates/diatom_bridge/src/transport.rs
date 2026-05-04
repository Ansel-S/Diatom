use anyhow::{Context, Result, bail};
use serde::{Serialize, de::DeserializeOwned};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Maximum allowed message body length (64 KiB).
const MAX_FRAME: usize = 64 * 1024;

/// Write one JSON-encoded message to `writer`.
pub async fn send<W, T>(writer: &mut W, msg: &T) -> Result<()>
where
    W: AsyncWrite + Unpin,
    T: Serialize,
{
    let mut buf = Vec::with_capacity(1024);
    buf.extend_from_slice(&[0, 0, 0, 0]);
    serde_json::to_writer(&mut buf, msg).context("serialize message")?;

    let len = buf.len() - 4;
    if len > MAX_FRAME {
        bail!(
            "message too large: {} bytes (max {})",
            len,
            MAX_FRAME
        );
    }

    buf[..4].copy_from_slice(&(len as u32).to_be_bytes());

    writer.write_all(&buf).await.context("write frame")?;
    Ok(())
}

/// Read one JSON-encoded message from `reader`.
/// Returns `None` on clean EOF (peer closed the connection).
pub async fn recv<R, T>(reader: &mut R) -> Result<Option<T>>
where
    R: AsyncRead + Unpin,
    T: DeserializeOwned,
{
    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e).context("read length prefix"),
    }
    
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_FRAME {
        bail!("incoming frame too large: {} bytes", len);
    }
    
    let mut body = vec![0u8; len];
    reader
        .read_exact(&mut body)
        .await
        .context("read frame body")?;
        
    let msg = serde_json::from_slice(&body).context("deserialize message")?;
    Ok(Some(msg))
}

/// Returns the platform-appropriate socket path for the given Diatom instance.
pub fn socket_path(pid: u32) -> String {
    #[cfg(unix)]
    {
        let tmp = std::env::temp_dir();
        format!("{}/diatom-devpanel-{}.sock", tmp.display(), pid)
    }
    #[cfg(windows)]
    {
        format!(r"\\.\pipe\diatom-devpanel-{}", pid)
    }
}
