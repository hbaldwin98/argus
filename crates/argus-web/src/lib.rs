//! `argus web`: the phone client, served by a foreground client of the
//! local daemon.
//!
//! The daemon never listens on a network; this process does, for exactly
//! as long as it runs. It binds loopback unless told otherwise and does no
//! TLS — a phone reaches it through something that does, such as Tailscale
//! Serve. How it gets to the daemon is the caller's business: `argus`
//! hands it the same connect-or-start it uses itself, so there is one way
//! a daemon gets started. See TARGET.md, "Mobile and web client".

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite};

mod daemon;
mod markdown;
mod pairing;
mod phone;
mod routes;

pub use pairing::Devices;

/// The port `argus web` binds unless told otherwise. Fixed rather than
/// chosen per run, because an installed page and its push subscription
/// belong to one origin, and a new port every run is a new origin.
pub const DEFAULT_PORT: u16 = 7420;

/// How `argus web` was asked to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub listen: SocketAddr,
    /// The address a phone should open, when it is not the one bound —
    /// the name Tailscale Serve or a proxy puts in front of it.
    pub url: Option<String>,
    /// Where paired devices are kept.
    pub config_dir: PathBuf,
}

/// Serves the phone client until interrupted, or until the daemon says it
/// is stopping.
pub async fn run<C, F, S>(options: Options, connect: C) -> anyhow::Result<()>
where
    C: Fn() -> F + Send + Sync + 'static,
    F: std::future::Future<Output = anyhow::Result<S>> + Send + 'static,
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let listener = tokio::net::TcpListener::bind(options.listen)
        .await
        .map_err(|e| {
            anyhow::anyhow!(
                "argus web could not listen on {}: {e} (another argus web, or `--port` to pick another)",
                options.listen
            )
        })?;
    let (feeds, asks, link) = daemon::start(connect);
    let app = Arc::new(routes::App {
        feeds,
        asks,
        devices: Devices::at(&options.config_dir),
        code: Mutex::new(pairing::Code::new()?),
    });

    let url = options
        .url
        .clone()
        .unwrap_or_else(|| format!("http://{}", options.listen));
    println!("{}", banner(&options, &url, app.code.lock().unwrap().digits()));
    tokio::spawn(new_codes_on_enter(app.clone(), url));

    let shutdown = async move {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = link => println!("argusd is stopping, so argus web is too."),
        }
    };
    axum::serve(listener, routes::router(app))
        .with_graceful_shutdown(shutdown)
        .await?;
    Ok(())
}

/// What `argus web` prints on starting: where to go, how to pair, and a
/// warning when what it binds is reachable without encryption.
fn banner(options: &Options, url: &str, code: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("argus web is serving {url}\n"));
    if !options.listen.ip().is_loopback() && !url.starts_with("https://") {
        out.push_str(&format!(
            "\nWarning: {} is reachable from the network over plain HTTP. A paired \
phone's cookie crosses it unencrypted; put HTTPS in front (Tailscale Serve) and pass --url.\n",
            options.listen
        ));
    }
    out.push('\n');
    out.push_str(&pairing_text(url, code));
    out
}

/// The pairing code, and a QR code that opens the page with it filled in.
fn pairing_text(url: &str, code: &str) -> String {
    let link = format!("{}/#pair={code}", url.trim_end_matches('/'));
    let qr = qrcode::QrCode::new(link.as_bytes())
        .map(|qr| {
            qr.render::<qrcode::render::unicode::Dense1x2>()
                .quiet_zone(true)
                .build()
        })
        .unwrap_or_default();
    format!(
        "{qr}\nScan to pair, or open {url} and enter {code}.\n\
The code works once, for {} minutes. Press Enter for a new one.\n",
        pairing::CODE_LIFETIME.as_secs() / 60
    )
}

async fn new_codes_on_enter(app: Arc<routes::App>, url: String) {
    let mut lines = tokio::io::BufReader::new(tokio::io::stdin()).lines();
    while let Ok(Some(_)) = lines.next_line().await {
        let Ok(code) = pairing::Code::new() else { continue };
        let digits = code.digits().to_string();
        *app.code.lock().unwrap() = code;
        println!("{}", pairing_text(&url, &digits));
    }
}

/// `argus web devices`: every paired device, one a line.
pub fn devices(config_dir: &Path) -> String {
    let devices = Devices::at(config_dir).list();
    if devices.is_empty() {
        return "No devices are paired with argus web.".to_string();
    }
    devices
        .iter()
        .map(|d| format!("{}  (paired {})", d.name, date(d.paired)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A Unix time as its UTC calendar date: all a list of paired devices
/// needs, without a calendar crate for it.
fn date(secs: u64) -> String {
    // Howard Hinnant's days-to-civil, for days since 1970-01-01.
    let days = (secs / 86_400) as i64 + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 { month_index + 3 } else { month_index - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

/// `argus web revoke <name>`: unpairs a device. A running `argus web` drops
/// its sessions within seconds.
pub fn revoke(config_dir: &Path, name: &str) -> anyhow::Result<String> {
    if Devices::at(config_dir).revoke(name)? {
        Ok(format!("{name} is no longer paired."))
    } else {
        anyhow::bail!("no paired device is called {name}; `argus web devices` lists them")
    }
}

#[cfg(test)]
mod tests;
