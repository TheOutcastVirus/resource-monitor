//! Aggregate counters and socket ownership. Socket counts are not bandwidth.
use crate::metrics::Process;
use std::{
    collections::{HashMap, HashSet},
    fs,
    time::{Duration, Instant},
};
#[derive(Clone, Default, Debug)]
pub struct Interface {
    pub name: String,
    pub rx: u64,
    pub tx: u64,
    pub rx_rate: f32,
    pub tx_rate: f32,
}
#[derive(Clone, Default, Debug)]
pub struct Network {
    pub interfaces: Vec<Interface>,
    pub rx_rate: f32,
    pub tx_rate: f32,
    pub rx: u64,
    pub tx: u64,
}
pub struct NetworkSampler {
    previous: HashMap<String, (u64, u64)>,
    at: Instant,
    sockets_at: Option<Instant>,
    sockets: HashMap<(u32, u64), Option<(u32, u32)>>,
}
fn counters(text: &str) -> Vec<(String, u64, u64)> {
    text.lines()
        .filter_map(|l| {
            let (name, data) = l.split_once(':')?;
            let v: Vec<_> = data.split_whitespace().collect();
            Some((
                name.trim().into(),
                v.first()?.parse().ok()?,
                v.get(8)?.parse().ok()?,
            ))
        })
        .collect()
}
impl NetworkSampler {
    pub fn new() -> Self {
        Self {
            previous: HashMap::new(),
            at: Instant::now(),
            sockets_at: None,
            sockets: HashMap::new(),
        }
    }
    pub fn sample(&mut self, processes: &mut [Process]) -> Network {
        let now = Instant::now();
        let seconds = now.duration_since(self.at).as_secs_f32().max(0.001);
        self.at = now;
        let mut network = Network::default();
        let mut next = HashMap::new();
        for (name, rx, tx) in counters(&fs::read_to_string("/proc/net/dev").unwrap_or_default()) {
            let (rx_rate, tx_rate) = self
                .previous
                .get(&name)
                .map(|(a, b)| {
                    (
                        rx.saturating_sub(*a) as f32 / seconds,
                        tx.saturating_sub(*b) as f32 / seconds,
                    )
                })
                .unwrap_or_default();
            next.insert(name.clone(), (rx, tx));
            if name != "lo" {
                network.rx_rate += rx_rate;
                network.tx_rate += tx_rate;
                network.rx += rx;
                network.tx += tx;
                network.interfaces.push(Interface {
                    name,
                    rx,
                    tx,
                    rx_rate,
                    tx_rate,
                });
            }
        }
        self.previous = next;
        network.interfaces.sort_by(|a, b| a.name.cmp(&b.name));
        if self
            .sockets_at
            .is_none_or(|t| now.duration_since(t) >= Duration::from_secs(3))
        {
            self.sockets_at = Some(now);
            self.sockets.clear();
            let mut tcp = HashSet::new();
            let mut udp = HashSet::new();
            for (file, is_tcp) in [
                ("tcp", true),
                ("tcp6", true),
                ("udp", false),
                ("udp6", false),
            ] {
                let set = if is_tcp { &mut tcp } else { &mut udp };
                if let Ok(text) = fs::read_to_string(format!("/proc/net/{file}")) {
                    for l in text.lines().skip(1) {
                        if let Some(inode) = l
                            .split_whitespace()
                            .nth(9)
                            .and_then(|s| s.parse::<u64>().ok())
                        {
                            set.insert(inode);
                        }
                    }
                }
            }
            for p in processes.iter() {
                let counts = fs::read_dir(format!("/proc/{}/fd", p.pid)).ok().map(|fds| {
                    let mut seen = HashSet::new();
                    for fd in fds.flatten() {
                        if let Ok(link) = fs::read_link(fd.path()) {
                            let link = link.to_string_lossy();
                            if let Some(inode) = link
                                .strip_prefix("socket:[")
                                .and_then(|s| s.strip_suffix(']'))
                                .and_then(|s| s.parse::<u64>().ok())
                            {
                                seen.insert(inode);
                            }
                        }
                    }
                    (
                        seen.intersection(&tcp).count() as u32,
                        seen.intersection(&udp).count() as u32,
                    )
                });
                self.sockets.insert((p.pid, p.start_time), counts);
            }
        }
        for p in processes {
            if let Some(Some((tcp, udp))) = self.sockets.get(&(p.pid, p.start_time)) {
                p.tcp = Some(*tcp);
                p.udp = Some(*udp);
            }
        }
        network
    }
}
pub fn rate(v: f32) -> String {
    if v >= 1024. * 1024. {
        format!("{:.1} MiB/s", v / (1024. * 1024.))
    } else {
        format!("{:.1} KiB/s", v / 1024.)
    }
}
#[cfg(test)]
mod tests {
    use super::counters;
    #[test]
    fn parses_receive_and_transmit_without_mixing_packets() {
        let n = counters("Inter-| header\n eth0: 1024 2 0 0 0 0 0 0 2048 3 0 0 0 0 0 0");
        assert_eq!(n, vec![("eth0".into(), 1024, 2048)]);
    }
}
