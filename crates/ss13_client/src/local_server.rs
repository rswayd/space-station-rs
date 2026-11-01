//! Single-player launch of the release package (`docs/RUNNING.md`, "Windows demo / releases"):
//! with no `SS13_SERVER_ADDR` and a `data/` directory next to the executable, the client starts
//! the bundled `ss13_server` as a child on loopback and connects to it.

use std::io::{BufRead, BufReader};
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use ss13_net::client::NetClientConfig;

/// MetaStation loads in seconds in a release build; a debug build on a slow disk takes longer.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(300);

pub const SOURCE_URL: &str = "https://github.com/rswayd/space-station-rs";

/// The package directory when this run is a packaged single-player launch.
pub fn package_dir() -> Option<PathBuf> {
    if std::env::var_os("SS13_SERVER_ADDR").is_some() {
        return None;
    }
    let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    dir.join("data").is_dir().then_some(dir)
}

/// The child server; killed when dropped (the client exits or panics). If the client is killed
/// outright, the child sees its stdin close and exits (`SS13_EXIT_WITH_PARENT`).
pub struct LocalServer {
    child: Child,
    pub config: NetClientConfig,
}

impl Drop for LocalServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub fn start(dir: &Path) -> Result<LocalServer, String> {
    // shortcut: the port is probed, then bound by the child; another process can take it in
    // between (a failed start, run again). Pass the bound port back if that ever shows up.
    let port = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))
        .and_then(|s| s.local_addr())
        .map_err(|e| format!("no free loopback port: {e}"))?
        .port();
    let addr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), port);
    let exe = dir.join(format!("ss13_server{}", std::env::consts::EXE_SUFFIX));
    let mut command = Command::new(&exe);
    command
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        // Loopback only: the trust below holds because no other machine can reach the port.
        .env("SS13_BIND_ADDR", addr.to_string())
        // Every client is this machine's player, so the claimed ckey (`config/admins.txt`) is
        // trusted; never set this for a server that binds a non-loopback address.
        .env("SS13_INSECURE_ADMINS", "1")
        .env("SS13_EXIT_WITH_PARENT", "1");
    let defaults = [
        ("SS13_MAP", "MetaStation".into()),
        ("SS13_ASSETS", dir.join("assets")),
        ("SS13_SOURCE", dir.join("data/BandaStation")),
        ("SS13_CONFIG_DIR", dir.join("config")),
        // The round starts at once; the player joins it from the lobby (latejoin).
        ("SS13_LOBBY_SECONDS", "0".into()),
        ("SS13_SOURCE_URL", SOURCE_URL.into()),
    ];
    for (key, value) in defaults {
        if std::env::var_os(key).is_none() {
            command.env(key, value);
        }
    }
    #[cfg(windows)]
    {
        // CREATE_NO_WINDOW: no console window of its own; its output goes through the pipe.
        std::os::windows::process::CommandExt::creation_flags(&mut command, 0x0800_0000);
    }
    let mut child = command
        .spawn()
        .map_err(|e| format!("cannot start {}: {e}", exe.display()))?;
    let stdout = child.stdout.take().expect("piped");
    let (digest_tx, digest_rx) = mpsc::channel();
    // Drains the child's stdout for its whole life (a full pipe would block it) and passes the
    // digest line on.
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            match line.strip_prefix(ss13_game::LOCAL_DIGEST_LINE) {
                Some(digest) => {
                    let _ = digest_tx.send(digest.trim().to_owned());
                }
                None => println!("[server] {line}"),
            }
        }
    });
    let mut server = LocalServer {
        child,
        config: NetClientConfig {
            server_addr: addr,
            certificate_digest: String::new(),
            ..NetClientConfig::default()
        },
    };
    server.config.certificate_digest = match digest_rx.recv_timeout(STARTUP_TIMEOUT) {
        Ok(digest) => digest,
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            let status = server.child.wait().map(|s| s.to_string());
            return Err(format!(
                "the local server exited during startup: {status:?}"
            ));
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            return Err(format!(
                "the local server did not start in {STARTUP_TIMEOUT:?}"
            ));
        }
    };
    Ok(server)
}
