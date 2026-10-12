//! Wii U Connect session server.
//!
//! Phones connect over WebSocket (`/ws`); their input is validated, assigned to a DSU slot and served to Cemu
//! through the Cemuhook DSU protocol.

pub mod embedded;
pub mod http;
pub mod session;
pub mod setup;
mod setup_http;
pub mod tls;
pub mod tracking;
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
    /// Serve the phone app compiled into the executable when `web_dir` is `None`.
    pub embedded_web: bool,
    /// The `/setup` assistant (Cemu download and configuration, game folders); `None` disables it.
    pub setup: Option<setup::SetupConfig>,
    pub allowed_origins: Vec<String>,
    /// Host name or IP phones use to reach this PC; detected when `None`.
    pub public_host: Option<String>,
    /// Optional HTTPS listener (self-signed). Phones need it for camera and motion sensors.
    pub https: Option<HttpsConfig>,
}

#[derive(Debug, Clone)]
pub struct HttpsConfig {
    pub bind: SocketAddr,
    /// Where the certificate and key are stored.
    pub data_dir: PathBuf,
}

pub struct Running {
    pub http_addr: SocketAddr,
    pub https_addr: Option<SocketAddr>,
    /// Certificate file, when HTTPS is enabled.
    pub cert_path: Option<PathBuf>,
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

/// A fixed token must fit the protocol's `hello.token` limit and survive the pairing URL unencoded.
fn valid_fixed_token(t: &str) -> bool {
    (16..=128).contains(&t.len()) && t.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Binds all sockets and spawns the server tasks. Returns once everything is listening.
pub async fn start(
    config: ServerConfig,
    shutdown: tokio::sync::watch::Receiver<bool>,
) -> anyhow::Result<(Running, tokio::task::JoinHandle<()>)> {
    let token = match config.token {
        Some(t) if valid_fixed_token(&t) => t,
        Some(_) => anyhow::bail!("--token must be 16-128 characters of A-Z, a-z, 0-9, '-' or '_'"),
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
    let https = match &config.https {
        Some(h) => {
            tls::install_crypto_provider();
            let hosts = vec!["localhost".to_string(), "127.0.0.1".to_string(), host.clone()];
            let certs = tls::load_or_create(&h.data_dir, &hosts)?;
            let rustls_config = axum_server::tls_rustls::RustlsConfig::from_pem_file(&certs.cert, &certs.key)
                .await
                .context("loading HTTPS certificate")?;
            let listener =
                std::net::TcpListener::bind(h.bind).with_context(|| format!("cannot listen on {} (HTTPS)", h.bind))?;
            listener.set_nonblocking(true)?;
            let addr = listener.local_addr()?;
            Some((listener, rustls_config, addr, certs.cert))
        }
        None => None,
    };
    let phone_base_url = match &https {
        Some((_, _, addr, _)) => format!("https://{host}:{}", addr.port()),
        None => format!("http://{host}:{}", http_addr.port()),
    };
    let http_config = http::HttpConfig {
        phone_base_url,
        allowed_origins: config.allowed_origins.clone(),
        web_dir: config.web_dir.clone(),
        embedded_web: config.embedded_web,
    };
    let phone_url = http_config.phone_url(&token);
    let app = http::router(http::AppState {
        sessions: sessions.clone(),
        config: Arc::new(http_config),
        setup: config.setup.clone().map(|cfg| setup::Setup::new(cfg, dsu_addr)),
    });
    let https_app = app.clone();

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
    let http_stop = wait(shutdown.clone());
    let http_task = tokio::spawn(async move {
        let serve = axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
            .with_graceful_shutdown(http_stop);
        if let Err(e) = serve.await {
            tracing::error!("http server error: {e}");
        }
    });
    let (https_addr, cert_path, https_task) = match https {
        Some((listener, rustls_config, addr, cert)) => {
            let handle = axum_server::Handle::new();
            let shutdown_handle = handle.clone();
            let stop = wait(shutdown.clone());
            tokio::spawn(async move {
                stop.await;
                shutdown_handle.graceful_shutdown(Some(Duration::from_secs(1)));
            });
            let server = axum_server::from_tcp_rustls(listener, rustls_config)?.handle(handle);
            let app = https_app;
            let task = tokio::spawn(async move {
                if let Err(e) = server
                    .serve(app.into_make_service_with_connect_info::<SocketAddr>())
                    .await
                {
                    tracing::error!("https server error: {e}");
                }
            });
            (Some(addr), Some(cert), Some(task))
        }
        None => (None, None, None),
    };
    let all = tokio::spawn(async move {
        let _ = tokio::join!(dsu_task, watchdog_task, http_task);
        if let Some(t) = https_task {
            let _ = t.await;
        }
    });

    Ok((
        Running {
            http_addr,
            https_addr,
            cert_path,
            dsu_addr,
            token,
            phone_url,
            sessions,
            table,
        },
        all,
    ))
}

#[cfg(test)]
mod tests {
    use super::valid_fixed_token;

    #[test]
    fn fixed_tokens_must_be_url_safe_and_fit_the_protocol() {
        assert!(valid_fixed_token("test-pairing-token-not-secret"));
        assert!(valid_fixed_token(&"a".repeat(128)));
        assert!(!valid_fixed_token("short"));
        assert!(!valid_fixed_token(&"a".repeat(129)));
        assert!(!valid_fixed_token("has space in the token"));
        assert!(!valid_fixed_token("amp&ersand-token-value"));
    }
}
