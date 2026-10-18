//! Map raw Incus instances to the shared UI model, and derive CPU percentages.
use super::raw;
use crate::model::*;
use std::collections::HashMap;
use std::time::Instant;

fn kind_of(s: &str) -> InstanceKind {
    if s == "virtual-machine" {
        InstanceKind::Vm
    } else {
        InstanceKind::Container
    }
}

fn status_of(s: &str) -> InstanceStatus {
    match s {
        "Running" => InstanceStatus::Running,
        "Stopped" => InstanceStatus::Stopped,
        "Frozen" => InstanceStatus::Frozen,
        "Error" => InstanceStatus::Error,
        o => InstanceStatus::Other(o.to_string()),
    }
}

fn image_desc(r: &raw::Instance) -> String {
    let get = |k: &str| r.config.get(k).map(|s| s.trim()).filter(|s| !s.is_empty());
    if let Some(d) = get("image.description") {
        return d.to_string();
    }
    match (get("image.os"), get("image.release")) {
        (Some(o), Some(rel)) => format!("{o} {rel}"),
        (Some(o), None) => o.to_string(),
        (None, Some(rel)) => rel.to_string(),
        (None, None) => String::new(),
    }
}

/// Parse an Incus byte-size value such as "2GiB", "512MB" or "1024".
/// Returns `None` for percentages or anything unparseable.
fn parse_size(s: &str) -> Option<u64> {
    let s = s.trim();
    let split = s.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(s.len());
    let (num, unit) = s.split_at(split);
    let n: f64 = num.parse().ok()?;
    let mult: f64 = match unit.trim().to_ascii_lowercase().as_str() {
        "" | "b" => 1.0,
        "kb" | "k" => 1e3,
        "mb" | "m" => 1e6,
        "gb" | "g" => 1e9,
        "tb" | "t" => 1e12,
        "pb" | "p" => 1e15,
        "kib" => 1024.0,
        "mib" => 1024f64.powi(2),
        "gib" => 1024f64.powi(3),
        "tib" => 1024f64.powi(4),
        "pib" => 1024f64.powi(5),
        _ => return None,
    };
    Some((n * mult) as u64)
}

fn mem_limit(r: &raw::Instance) -> Option<u64> {
    parse_size(r.expanded_config.get("limits.memory")?).filter(|n| *n > 0)
}

/// `limits.cpu` when it is a plain integer (not a pinning range like "0-3").
pub fn cpu_limit(r: &raw::Instance) -> Option<u32> {
    r.expanded_config
        .get("limits.cpu")?
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|n| *n > 0)
}

/// Cumulative CPU time in nanoseconds, if the instance reports it.
pub fn cpu_usage_ns(r: &raw::Instance) -> Option<u64> {
    r.state.as_ref()?.cpu.as_ref().map(|c| c.usage)
}

fn disk_used(state: &raw::InstanceState) -> u64 {
    match &state.disk {
        Some(d) => match d.get("root") {
            Some(root) => root.usage,
            None => d.values().map(|x| x.usage).sum(),
        },
        None => 0,
    }
}

fn keep_address(iface: &str, a: &raw::Address) -> bool {
    if iface == "lo" || a.scope == "local" || a.scope == "link" {
        return false;
    }
    match a.address.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(ip)) => !ip.is_loopback() && !ip.is_link_local(),
        // fe80::/10
        Ok(std::net::IpAddr::V6(ip)) => !ip.is_loopback() && (ip.segments()[0] & 0xffc0) != 0xfe80,
        Err(_) => false,
    }
}

/// Convert a raw instance to the UI summary. `cpu_pct` is left `None`; fill it
/// from a [`CpuTracker`] for running instances.
pub fn to_summary(r: &raw::Instance) -> InstanceSummary {
    let mut ipv4 = Vec::new();
    let mut ipv6 = Vec::new();
    let (mut mem_used, mut disk, mut rx, mut tx) = (0, 0, 0, 0);
    if let Some(st) = &r.state {
        mem_used = st.memory.as_ref().map_or(0, |m| m.usage);
        disk = disk_used(st);
        if let Some(net) = &st.network {
            let mut ifaces: Vec<_> = net.iter().collect();
            ifaces.sort_by(|a, b| a.0.cmp(b.0));
            for (name, n) in ifaces {
                if name == "lo" {
                    continue;
                }
                if let Some(c) = &n.counters {
                    rx += c.bytes_received;
                    tx += c.bytes_sent;
                }
                for a in n.addresses.iter().filter(|a| keep_address(name, a)) {
                    match a.family.as_str() {
                        "inet" => ipv4.push(a.address.clone()),
                        "inet6" => ipv6.push(a.address.clone()),
                        _ => {}
                    }
                }
            }
        }
    }
    InstanceSummary {
        name: r.name.clone(),
        kind: kind_of(&r.kind),
        status: status_of(&r.status),
        image_desc: image_desc(r),
        ipv4,
        ipv6,
        cpu_pct: None,
        mem_used,
        mem_limit: mem_limit(r),
        disk_used: disk,
        net_rx: rx,
        net_tx: tx,
        created_at: r.created_at.clone(),
        profiles: r.profiles.clone(),
    }
}

/// Derives CPU percentage from successive cumulative-usage samples.
pub struct CpuTracker {
    host_threads: u32,
    last: HashMap<String, (u64, Instant)>,
}

impl CpuTracker {
    pub fn new(host_threads: u32) -> Self {
        Self { host_threads: host_threads.max(1), last: HashMap::new() }
    }

    /// Percent of the instance's CPU capacity (host threads).
    /// `None` on the first sample, on counter reset, or zero elapsed time.
    pub fn sample(&mut self, name: &str, usage_ns: u64, at: Instant) -> Option<f32> {
        self.sample_with_limit(name, usage_ns, at, None)
    }

    /// Like [`sample`](Self::sample) but with the instance's `limits.cpu`.
    pub fn sample_with_limit(
        &mut self,
        name: &str,
        usage_ns: u64,
        at: Instant,
        cpus: Option<u32>,
    ) -> Option<f32> {
        let prev = self.last.insert(name.to_string(), (usage_ns, at))?;
        let elapsed = at.checked_duration_since(prev.1)?.as_nanos() as f64;
        if elapsed <= 0.0 || usage_ns < prev.0 {
            return None;
        }
        let cpus = cpus.filter(|c| *c > 0).unwrap_or(self.host_threads) as f64;
        let pct = (usage_ns - prev.0) as f64 / (elapsed * cpus) * 100.0;
        Some(pct.clamp(0.0, 100.0) as f32)
    }

    /// Drop state for instances no longer present or not running.
    pub fn retain(&mut self, mut keep: impl FnMut(&str) -> bool) {
        self.last.retain(|k, _| keep(k));
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn load() -> Vec<raw::Instance> {
        let s = std::fs::read_to_string("tests/fixtures/instances_recursion2.json").unwrap();
        let v: Value = serde_json::from_str(&s).unwrap();
        serde_json::from_value(v["metadata"].clone()).unwrap()
    }

    #[test]
    fn ips_exclude_loopback_and_link_local() {
        let s = to_summary(&load()[0]);
        assert_eq!(s.kind, InstanceKind::Container);
        assert_eq!(s.ipv4, vec!["10.178.204.23"]);
        assert_eq!(s.ipv6, vec!["fd42:1:2:3::23"]);
        assert_eq!(s.image_desc, "Ubuntu noble amd64 (20261001_07:42)");
        assert_eq!(s.mem_limit, Some(2 << 30));
        assert_eq!(s.disk_used, 104857600);
        assert_eq!((s.net_rx, s.net_tx), (1000, 2000));
    }

    #[test]
    fn stopped_vm_maps() {
        let s = to_summary(&load()[1]);
        assert_eq!(s.kind, InstanceKind::Vm);
        assert_eq!(s.status, InstanceStatus::Stopped);
        assert_eq!(s.cpu_pct, None);
        assert_eq!(s.image_desc, "Debian 12");
        assert_eq!(s.mem_limit, Some(4096 * 1024 * 1024));
        assert!(s.ipv4.is_empty());
    }

    #[test]
    fn cpu_pct_from_deltas() {
        let mut t = CpuTracker::new(1);
        let t0 = Instant::now();
        assert_eq!(t.sample("a", 0, t0), None);
        let p = t.sample("a", 500_000_000, t0 + std::time::Duration::from_secs(1));
        assert_eq!(p, Some(50.0));
    }

    #[test]
    fn size_parsing() {
        assert_eq!(parse_size("512MB"), Some(512_000_000));
        assert_eq!(parse_size("2GiB"), Some(2 << 30));
        assert_eq!(parse_size("50%"), None);
        assert_eq!(cpu_limit(&load()[1]), Some(2));
    }
}
