//! TCP reverse-tunnel **endpoint** with HTTP control plane (no UDP).
//! Protocol-compatible with `tcptotcpgohttp-server` / `tcptotcphttp-server`.

use clap::Parser;
use serde::{Deserialize, Serialize};
use std::{
    net::{Shutdown, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};
use tracing::{error, info, warn};

const CONNECT_BUFFER_SEC: u8 = 5;
const CONNECT_IDLE_SEC: u64 = 7200;
const RECONNECT_DELAY: Duration = Duration::from_secs(20);
const HEALTH_INTERVAL: Duration = Duration::from_secs(120);

#[derive(Parser, Debug)]
#[command(name = "tcptotcphttp-endpoint", about = "HTTP-control TCP reverse-tunnel endpoint (Rust)")]
struct Cli {
    #[arg(long = "server-host", env = "RUST_APP_SERVER_HOST")]
    server_host: String,

    #[arg(long = "server-port", env = "RUST_APP_SERVER_PORT", default_value = "3000")]
    server_port: u16,

    /// Local TCP service to expose, e.g. 127.0.0.1:22
    #[arg(long = "endpoint", env = "RUST_APP_ADDR_ENDPOINT")]
    endpoint: String,

    /// preferred_client|preferred_app|token
    #[arg(long = "register", env = "RUST_APP_REGISTER_VALUE", default_value = "")]
    register: String,

    #[arg(long = "poll-mode", env = "RUST_APP_POLL_MODE", default_value = "long")]
    poll_mode: String,

    #[arg(long = "poll-wait", env = "RUST_APP_POLL_WAIT", default_value_t = 20)]
    poll_wait: u64,

    /// Optional local dummy HTTP port; only enabled when > 0
    #[arg(long = "local-http", env = "RUST_APP_LOCAL_HTTP_PORT", default_value_t = 0)]
    local_http: u16,

    #[arg(long = "env-file", env = "RUST_APP_FILE_PATH_ENV_APP")]
    env_file: Option<String>,
}

#[derive(Serialize)]
struct RegisterReq {
    register_value: String,
}

#[derive(Deserialize)]
struct RegisterResp {
    ok: bool,
    client_port: Option<u16>,
    app_port: Option<u16>,
    key: Option<String>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct PendingResp {
    ok: bool,
    count: Option<usize>,
    error: Option<String>,
}

fn main() {
    let env_file_early = std::env::args()
        .position(|a| a == "--env-file")
        .and_then(|i| std::env::args().nth(i + 1))
        .or_else(|| {
            std::env::args()
                .find(|a| a.starts_with("--env-file="))
                .map(|a| a.trim_start_matches("--env-file=").to_string())
        });
    if let Some(p) = &env_file_early {
        let _ = dotenvy::from_filename(p);
    } else {
        let _ = dotenvy::from_filename("env/app.env");
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli = Cli::parse();
    if cli.server_host.trim().is_empty() || cli.endpoint.trim().is_empty() {
        error!("required: --server-host and --endpoint (or RUST_APP_SERVER_HOST / RUST_APP_ADDR_ENDPOINT)");
        std::process::exit(1);
    }

    if cli.local_http > 0 {
        let p = cli.local_http;
        thread::spawn(move || {
            if let Ok(ln) = TcpListener::bind(format!("0.0.0.0:{}", p)) {
                info!(port = p, "local dummy HTTP on");
                for s in ln.incoming().flatten() {
                    let _ = s;
                    // accept and drop — just keep port open for health
                }
            } else {
                warn!(port = p, "failed to bind local-http");
            }
        });
    }

    let base = format!("http://{}:{}", cli.server_host.trim(), cli.server_port);
    let cfg = SessionCfg {
        server_host: cli.server_host.trim().to_string(),
        base_url: base,
        endpoint: cli.endpoint.trim().to_string(),
        register: cli.register.trim().to_string(),
        poll_mode: cli.poll_mode.trim().to_lowercase(),
        poll_wait: cli.poll_wait,
    };

    loop {
        match run_session(&cfg) {
            Ok(()) => info!("session ended; reconnect in {:?}", RECONNECT_DELAY),
            Err(e) => warn!(error = %e, "session ended; reconnect in {:?}", RECONNECT_DELAY),
        }
        thread::sleep(RECONNECT_DELAY);
    }
}

struct SessionCfg {
    server_host: String,
    base_url: String,
    endpoint: String,
    register: String,
    poll_mode: String,
    poll_wait: u64,
}

struct Reg {
    key: String,
    client_port: u16,
    app_port: u16,
}

fn run_session(cfg: &SessionCfg) -> Result<(), String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(cfg.poll_wait + 15))
        .build()
        .map_err(|e| e.to_string())?;

    let reg = do_register(&client, cfg)?;
    info!(
        key = %reg.key,
        client_port = reg.client_port,
        app_port = reg.app_port,
        public = %format!("{}:{}", cfg.server_host, reg.client_port),
        "registered"
    );

    let closed = Arc::new(AtomicBool::new(false));
    let closed2 = closed.clone();
    let host = cfg.server_host.clone();
    let app_port = reg.app_port;
    let endpoint = cfg.endpoint.clone();
    thread::spawn(move || {
        loop {
            thread::sleep(HEALTH_INTERVAL);
            if closed2.load(Ordering::Relaxed) {
                break;
            }
            if !tcp_ok(&format!("{}:{}", host, app_port)) || !tcp_ok(&endpoint) {
                warn!("health check failed; ending session");
                closed2.store(true, Ordering::Release);
                break;
            }
        }
    });

    let result = (|| -> Result<(), String> {
        while !closed.load(Ordering::Relaxed) {
            let count = do_pending(&client, cfg, &reg.key)?;
            if closed.load(Ordering::Relaxed) {
                break;
            }
            if count == 0 {
                if cfg.poll_mode == "short" {
                    thread::sleep(Duration::from_secs(1));
                }
                continue;
            }
            for _ in 0..count {
                if closed.load(Ordering::Relaxed) {
                    break;
                }
                let host = cfg.server_host.clone();
                let app_port = reg.app_port;
                let endpoint = cfg.endpoint.clone();
                thread::spawn(move || open_pair(&host, app_port, &endpoint));
            }
        }
        Ok(())
    })();

    closed.store(true, Ordering::Release);
    let _ = do_quit(&client, cfg, &reg.key);
    result
}

fn do_register(client: &reqwest::blocking::Client, cfg: &SessionCfg) -> Result<Reg, String> {
    let url = format!("{}/v1/register", cfg.base_url);
    let resp = client
        .post(&url)
        .json(&RegisterReq {
            register_value: cfg.register.clone(),
        })
        .send()
        .map_err(|e| e.to_string())?;
    let body: RegisterResp = resp.json().map_err(|e| e.to_string())?;
    if !body.ok || body.key.as_ref().map(|s| s.is_empty()).unwrap_or(true) {
        return Err(body.error.unwrap_or_else(|| "register failed".into()));
    }
    Ok(Reg {
        key: body.key.unwrap(),
        client_port: body.client_port.unwrap_or(0),
        app_port: body.app_port.unwrap_or(0),
    })
}

fn do_pending(
    client: &reqwest::blocking::Client,
    cfg: &SessionCfg,
    key: &str,
) -> Result<usize, String> {
    let mode = if cfg.poll_mode == "short" {
        "short"
    } else {
        "long"
    };
    let wait = if mode == "short" { 0 } else { cfg.poll_wait };
    let url = format!(
        "{}/v1/pending?key={}&mode={}&wait={}",
        cfg.base_url,
        urlencoding(key),
        mode,
        wait
    );
    let resp = client.get(&url).send().map_err(|e| e.to_string())?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err("tunnel not found".into());
    }
    let body: PendingResp = resp.json().map_err(|e| e.to_string())?;
    if !body.ok {
        return Err(body.error.unwrap_or_else(|| "pending failed".into()));
    }
    Ok(body.count.unwrap_or(0))
}

fn do_quit(client: &reqwest::blocking::Client, cfg: &SessionCfg, key: &str) -> Result<(), String> {
    let url = format!("{}/v1/quit", cfg.base_url);
    let _ = client
        .post(&url)
        .json(&serde_json::json!({ "key": key }))
        .timeout(Duration::from_secs(10))
        .send();
    Ok(())
}

fn open_pair(server_host: &str, app_port: u16, endpoint: &str) {
    let server_addr = format!("{}:{}", server_host, app_port);
    let stream_server = match resolve_connect(&server_addr, Duration::from_secs(10)) {
        Ok(s) => s,
        Err(e) => {
            warn!(%server_addr, error = %e, "dial server");
            return;
        }
    };
    let stream_local = match resolve_connect(endpoint, Duration::from_secs(10)) {
        Ok(s) => s,
        Err(e) => {
            warn!(endpoint, error = %e, "dial local");
            let _ = stream_server.shutdown(Shutdown::Both);
            return;
        }
    };
    let _ = tcptotcp::connect(stream_server, stream_local, CONNECT_BUFFER_SEC, CONNECT_IDLE_SEC);
}

fn resolve_connect(addr: &str, timeout: Duration) -> Result<TcpStream, String> {
    use std::net::ToSocketAddrs;
    let mut last = "no address".to_string();
    for sa in addr
        .to_socket_addrs()
        .map_err(|e| e.to_string())?
    {
        match TcpStream::connect_timeout(&sa, timeout) {
            Ok(s) => return Ok(s),
            Err(e) => last = e.to_string(),
        }
    }
    Err(last)
}

fn tcp_ok(addr: &str) -> bool {
    resolve_connect(addr, Duration::from_secs(5)).is_ok()
}

fn urlencoding(s: &str) -> String {
    // minimal encode for key with | 
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            _ => format!("%{:02X}", c as u8),
        })
        .collect()
}
