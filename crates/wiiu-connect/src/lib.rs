//! Wii U Connect session server.
//!
//! Phones connect over WebSocket (`/ws`); their input is validated, assigned to a DSU slot and served to Cemu
//! through the Cemuhook DSU protocol.

pub mod http;
pub mod session;
pub mod ws;

use anyhow::Context;
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use wiiu_dsu::{DsuServer, SlotTable};

#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// HTTP/WebSocket listen address. Phones must reach it over the LAN.
    pub bind: SocketAddr,
    /// DSU listen address. Defaults to loopback: only Cemu on this PC needs it.
    pub dsu_bind: SocketAddr,
    /// Pairing token; random when `None`.
    pub token: Option<String>,
    pub web_dir: Option<PathBuf>,
    pub allowed_origins: Vec<String>,
    /// Host name or IP phones use to reach this PC; detected when `None`.
    pub public_host: Option<String>,
}

pub struct Running {
    pub http_addr: SocketAddr,
    pub dsu_addr: SocketAddr,
    pub token: String,
    pub phone_url: String,
    pub sessions: Arc<session::SessionManager>,
    pub table: SlotTable,
}

/// 128-bit random token, hex encoded.
pub fn generate_token() -> anyhow::Result<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| anyhow::anyhow!("no OS randomness: {e}"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// The LAN address of the interface used for outbound traffic. No packets are sent.
pub fn lan_ip() -> Option<IpAddr> {
    let s = UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("192.0.2.1:9").ok()?; // TEST-NET-1, never routed
    let ip = s.local_addr().ok()?.ip();
    (!ip.is_unspecified() && !ip.is_loopback()).then_some(ip)
}

/// Binds all sockets and spawns the server tasks. Returns once everything is listening.
pub async fn start(
    config: ServerConfig,
    shutdown: tokio::sync::watch::Receiver<bool>,
) -> anyhow::Result<(Running, tokio::task::JoinHandle<()>)> {
    let token = match config.token {
        Some(t) if t.len() >= 16 => t,
        Some(_) => anyhow::bail!("--token must be at least 16 characters"),
        None => generate_token()?,
    };
    let table = SlotTable::new();
    let sessions = Arc::new(session::SessionManager::new(token.clone(), table.clone()));

    let dsu = DsuServer::bind(config.dsu_bind, table.clone())
        .await
        .with_context(|| format!("cannot bind DSU server on {}", config.dsu_bind))?;
    let dsu_addr = dsu.local_addr()?;

    let listener = tokio::net::TcpListener::bind(config.bind)
        .await
        .with_context(|| format!("cannot listen on {}", config.bind))?;
    let http_addr = listener.local_addr()?;

    let host = config.public_host.clone().unwrap_or_else(|| {
        if http_addr.ip().is_loopback() {
            "127.0.0.1".into()
        } else {
            lan_ip().map(|ip| ip.to_string()).unwrap_or_else(|| "127.0.0.1".into())
        }
    });
    let http_config = http::HttpConfig {
        phone_base_url: format!("http://{host}:{}", http_addr.port()),
        allowed_origins: config.allowed_origins.clone(),
        web_dir: config.web_dir.clone(),
    };
    let phone_url = http_config.phone_url(&token);
    let app = http::router(http::AppState {
        sessions: sessions.clone(),
        config: Arc::new(http_config),
    });

    let wait = |mut rx: tokio::sync::watch::Receiver<bool>| async move {
        let _ = rx.wait_for(|stop| *stop).await;
    };
    let dsu_task = tokio::spawn(dsu.run(wait(shutdown.clone())));
    let watchdog_sessions = sessions.clone();
    let mut watchdog_stop = shutdown.clone();
    let watchdog_task = tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_millis(100));
        loop {
            tokio::select! {
                _ = watchdog_stop.wait_for(|s| *s) => break,
                _ = tick.tick() => {
                    for slot in watchdog_sessions.watchdog(Instant::now()) {
                        tracing::warn!("slot {slot}: no input for 1 s, released all buttons");
                    }
                }
            }
        }
    });
    let http_task = tokio::spawn(async move {
        let serve = axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
            .with_graceful_shutdown(wait(shutdown));
        if let Err(e) = serve.await {
            tracing::error!("http server error: {e}");
        }
    });
    let all = tokio::spawn(async move {
        let _ = tokio::join!(dsu_task, watchdog_task, http_task);
    });

    Ok((
        Running {
            http_addr,
            dsu_addr,
            token,
            phone_url,
            sessions,
            table,
        },
        all,
    ))
}
