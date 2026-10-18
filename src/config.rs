use std::net::SocketAddr;
use std::path::PathBuf;

pub const DEFAULT_LISTEN: &str = "0.0.0.0:5000";
pub const DEFAULT_INCUS: &str = "unix:///var/lib/incus/unix.socket";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IncusEndpoint {
    Unix(PathBuf),
    Tcp(SocketAddr),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub listen: SocketAddr,
    pub incus: IncusEndpoint,
}

impl IncusEndpoint {
    pub fn parse(s: &str) -> Result<IncusEndpoint, String> {
        if let Some(path) = s.strip_prefix("unix://") {
            if path.is_empty() {
                return Err("unix:// endpoint needs a socket path".into());
            }
            Ok(IncusEndpoint::Unix(PathBuf::from(path)))
        } else if let Some(addr) = s.strip_prefix("tcp://") {
            addr.parse()
                .map(IncusEndpoint::Tcp)
                .map_err(|e| format!("invalid tcp endpoint {addr:?}: {e}"))
        } else {
            Err(format!(
                "unsupported Incus endpoint {s:?} (expected unix:///path or tcp://host:port)"
            ))
        }
    }
}

impl Config {
    pub fn from_env() -> anyhow::Result<Config> {
        Self::from_lookup(|k| std::env::var(k).ok())
    }

    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> anyhow::Result<Config> {
        let listen = get("SI_LISTEN").unwrap_or_else(|| DEFAULT_LISTEN.into());
        let listen = listen
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid SI_LISTEN {listen:?}: {e}"))?;
        let incus = get("SI_INCUS").unwrap_or_else(|| DEFAULT_INCUS.into());
        let incus = IncusEndpoint::parse(&incus).map_err(|e| anyhow::anyhow!("SI_INCUS: {e}"))?;
        Ok(Config { listen, incus })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_endpoints() {
        assert_eq!(
            IncusEndpoint::parse("unix:///var/lib/incus/unix.socket").unwrap(),
            IncusEndpoint::Unix("/var/lib/incus/unix.socket".into())
        );
        assert_eq!(
            IncusEndpoint::parse("tcp://127.0.0.1:8444").unwrap(),
            IncusEndpoint::Tcp("127.0.0.1:8444".parse().unwrap())
        );
        assert!(IncusEndpoint::parse("http://x").is_err());
        assert!(IncusEndpoint::parse("unix://").is_err());
        assert!(IncusEndpoint::parse("tcp://nonsense").is_err());
    }

    #[test]
    fn defaults() {
        let c = Config::from_lookup(|_| None).unwrap();
        assert_eq!(c.listen, "0.0.0.0:5000".parse().unwrap());
        assert_eq!(
            c.incus,
            IncusEndpoint::Unix("/var/lib/incus/unix.socket".into())
        );
    }

    #[test]
    fn overrides_and_errors() {
        let c = Config::from_lookup(|k| match k {
            "SI_LISTEN" => Some("127.0.0.1:8080".into()),
            "SI_INCUS" => Some("tcp://10.0.0.1:8443".into()),
            _ => None,
        })
        .unwrap();
        assert_eq!(c.listen, "127.0.0.1:8080".parse().unwrap());
        assert_eq!(c.incus, IncusEndpoint::Tcp("10.0.0.1:8443".parse().unwrap()));
        assert!(Config::from_lookup(|k| (k == "SI_LISTEN").then(|| "bad".into())).is_err());
    }
}
