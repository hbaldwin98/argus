//! `argus web`: what its flags ask for, and handing the web server this
//! machine's daemon to serve.
//!
//! The server itself is `argus-web`. It is handed the same connect-or-start
//! the terminal client uses, so a daemon is started one way whoever asks.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use crate::settings::WebSettings;

/// What `argus web` was asked to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebCommand {
    Serve {
        port: Option<u16>,
        listen: Option<String>,
        url: Option<String>,
    },
    Devices,
    Revoke(String),
}

pub const USAGE: &str =
    "usage: argus web [--port PORT] [--listen ADDR] [--url URL] | argus web devices | argus web revoke NAME";

pub fn parse(args: &[String]) -> anyhow::Result<WebCommand> {
    match args {
        [devices] if devices == "devices" => return Ok(WebCommand::Devices),
        [revoke, name] if revoke == "revoke" && !name.is_empty() => {
            return Ok(WebCommand::Revoke(name.clone()))
        }
        _ => {}
    }
    let (mut port, mut listen, mut url) = (None, None, None);
    let mut rest = args.iter();
    while let Some(flag) = rest.next() {
        let value = rest.next().ok_or_else(|| anyhow::anyhow!("{flag} wants a value\n{USAGE}"))?;
        match flag.as_str() {
            "--port" => {
                port = Some(value.parse().map_err(|_| anyhow::anyhow!("--port wants a number"))?)
            }
            "--listen" => listen = Some(value.clone()),
            "--url" => url = Some(value.clone()),
            _ => anyhow::bail!("{USAGE}"),
        }
    }
    Ok(WebCommand::Serve { port, listen, url })
}

/// The address to bind: `--listen` as an address or a bare IP, else
/// loopback; the port from `--port`, `client.toml`, or the default, unless
/// the address already names one.
pub fn listen_address(
    port: Option<u16>,
    listen: Option<&str>,
    settings: &WebSettings,
) -> anyhow::Result<SocketAddr> {
    let port = port.or(settings.port).unwrap_or(argus_web::DEFAULT_PORT);
    let Some(listen) = listen.or(settings.listen.as_deref()) else {
        return Ok(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port));
    };
    if let Ok(address) = listen.parse::<SocketAddr>() {
        return Ok(address);
    }
    let ip: IpAddr = listen
        .trim_start_matches('[')
        .trim_end_matches(']')
        .parse()
        .map_err(|_| anyhow::anyhow!("--listen wants an IP address, like 100.64.0.1 or 0.0.0.0"))?;
    Ok(SocketAddr::new(ip, port))
}

pub async fn run(command: WebCommand) -> anyhow::Result<()> {
    let config_dir = argus_protocol::config_dir();
    match command {
        WebCommand::Devices => println!("{}", argus_web::devices(&config_dir)),
        WebCommand::Revoke(name) => println!("{}", argus_web::revoke(&config_dir, &name)?),
        WebCommand::Serve { port, listen, url } => {
            let settings = crate::settings::load().web;
            let options = argus_web::Options {
                listen: listen_address(port, listen.as_deref(), &settings)?,
                url: url.or(settings.url),
                config_dir,
            };
            argus_web::run(options, crate::launch::ensure_daemon_and_connect).await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn web_with_no_flags_serves_with_the_defaults() {
        assert_eq!(
            parse(&[]).unwrap(),
            WebCommand::Serve { port: None, listen: None, url: None }
        );
        let bound = listen_address(None, None, &WebSettings::default()).unwrap();
        assert_eq!(bound, "127.0.0.1:7420".parse().unwrap());
    }

    #[test]
    fn flags_outrank_client_toml_which_outranks_the_defaults() {
        let settings = WebSettings {
            port: Some(8000),
            listen: Some("100.64.0.1".into()),
            url: None,
        };
        assert_eq!(
            listen_address(None, None, &settings).unwrap(),
            "100.64.0.1:8000".parse().unwrap()
        );
        assert_eq!(
            listen_address(Some(9000), Some("0.0.0.0"), &settings).unwrap(),
            "0.0.0.0:9000".parse().unwrap()
        );
        assert_eq!(
            listen_address(Some(9000), Some("[::1]:7000"), &settings).unwrap(),
            "[::1]:7000".parse().unwrap()
        );
    }

    #[test]
    fn devices_and_revoke_are_their_own_commands() {
        assert_eq!(parse(&args(&["devices"])).unwrap(), WebCommand::Devices);
        assert_eq!(
            parse(&args(&["revoke", "phone"])).unwrap(),
            WebCommand::Revoke("phone".into())
        );
        assert_eq!(
            parse(&args(&["--url", "https://box.ts.net", "--port", "7500"])).unwrap(),
            WebCommand::Serve {
                port: Some(7500),
                listen: None,
                url: Some("https://box.ts.net".into())
            }
        );
        assert!(parse(&args(&["--port"])).is_err());
        assert!(parse(&args(&["--bogus", "x"])).is_err());
        assert!(listen_address(None, Some("not an ip"), &WebSettings::default()).is_err());
    }
}
