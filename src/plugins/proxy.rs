use std::{
  net::{Ipv4Addr, Ipv6Addr},
  sync::Arc,
};

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use tokio::{
  io::{self, AsyncReadExt, AsyncWriteExt},
  net::{TcpListener, TcpStream},
};
use tracing::{info, trace};

use crate::state::AppState;

pub struct Plugin;

#[async_trait]
impl super::Plugin for Plugin {
  async fn start(&self, app: Arc<AppState>) -> Result<()> {
    let port = app.config.proxy_port;
    let listener =
      TcpListener::bind(("0.0.0.0", port)).await.with_context(|| {
        format!("Failed to bind SOCKS5 proxy on port {}", port)
      })?;

    info!("SOCKS5 Proxy listening on 0.0.0.0:{}", port);

    loop {
      let (client, peer_addr) = listener.accept().await?;
      let app_clone = Arc::clone(&app);

      tokio::spawn(async move {
        if let Err(e) = handle_session(client, app_clone).await {
          trace!("Proxy session with {peer_addr} ended: {e}");
        }
      });
    }
  }
}

async fn handle_session(
  mut client: TcpStream,
  app: Arc<AppState>,
) -> Result<()> {
  let (username, _hwid) = socks5_auth_handshake(&mut client).await?;

  if app.sv().license.validate(&username).await.is_err() {
    client.write_all(&[0x01, 0x01]).await?; // Auth Failed
    bail!("Invalid license for user: {}", username);
  }
  client.write_all(&[0x01, 0x00]).await?; // Auth Success

  let (host, port) = socks5_read_target(&mut client).await?;
  trace!("Target requested: {}:{}", host, port);

  let mut upstream =
    TcpStream::connect((host.as_str(), port)).await.with_context(|| {
      format!("Upstream connection failed: {}:{}", host, port)
    })?;
  client.write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).await?;

  let _guard = SessionGuard::new(app, username);

  io::copy_bidirectional(&mut client, &mut upstream).await?;

  Ok(())
}

async fn socks5_auth_handshake(
  client: &mut TcpStream,
) -> Result<(String, String)> {
  let mut header = [0u8; 2];
  client.read_exact(&mut header).await?;
  if header[0] != 0x05 {
    bail!("Unsupported SOCKS version: {}", header[0]);
  }

  let n_methods = header[1] as usize;
  let mut methods = vec![0u8; n_methods];
  client.read_exact(&mut methods).await?;

  if !methods.contains(&0x02) {
    client.write_all(&[0x05, 0xFF]).await?;
    bail!("Client does not support User/Pass auth");
  }
  client.write_all(&[0x05, 0x02]).await?;

  let mut auth_hdr = [0u8; 2];
  client.read_exact(&mut auth_hdr).await?;
  if auth_hdr[0] != 0x01 {
    bail!("Unsupported auth version");
  }

  let ulen = auth_hdr[1] as usize;
  let mut user_buf = vec![0u8; ulen];
  client.read_exact(&mut user_buf).await?;

  let plen = client.read_u8().await? as usize;
  let mut pass_buf = vec![0u8; plen];
  client.read_exact(&mut pass_buf).await?;

  let user = String::from_utf8_lossy(&user_buf).into_owned();
  let pass = String::from_utf8_lossy(&pass_buf).into_owned();

  Ok((user, pass))
}

async fn socks5_read_target(client: &mut TcpStream) -> Result<(String, u16)> {
  let mut hdr = [0u8; 4];
  client.read_exact(&mut hdr).await?;

  if hdr[0] != 0x05 || hdr[1] != 0x01 {
    client.write_all(&[0x05, 0x07, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).await?;
    bail!("Only CONNECT method is supported");
  }

  let host = match hdr[3] {
    0x01 => {
      // IPv4
      let mut ip = [0u8; 4];
      client.read_exact(&mut ip).await?;
      Ipv4Addr::from(ip).to_string()
    }
    0x03 => {
      // Domain
      let len = client.read_u8().await? as usize;
      let mut domain = vec![0u8; len];
      client.read_exact(&mut domain).await?;
      String::from_utf8_lossy(&domain).into_owned()
    }
    0x04 => {
      // IPv6
      let mut ip = [0u8; 16];
      client.read_exact(&mut ip).await?;
      Ipv6Addr::from(ip).to_string()
    }
    _ => bail!("Unsupported address type"),
  };

  let port = client.read_u16().await?;
  Ok((host, port))
}

struct SessionGuard {
  app: Arc<AppState>,
  key: String,
}

impl SessionGuard {
  fn new(app: Arc<AppState>, key: String) -> Self {
    app
      .active_proxy_sessions
      .entry(key.clone())
      .and_modify(|c| *c += 1)
      .or_insert(1);
    Self { app, key }
  }
}

impl Drop for SessionGuard {
  fn drop(&mut self) {
    self.app.active_proxy_sessions.entry(self.key.clone()).and_modify(|c| {
      if *c > 1 {
        *c -= 1;
      } else {
        self.app.active_proxy_sessions.remove(&self.key);
      }
    });
  }
}
