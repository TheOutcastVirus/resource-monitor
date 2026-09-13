//! Linux sampling lives off the UI thread. No subprocesses in the sampling path.
use nvml_wrapper::{
    Nvml,
    enum_wrappers::device::{Clock, PcieUtilCounter, TemperatureSensor},
    enums::device::UsedGpuMemory,
};
use std::{
    collections::HashMap,
    fs,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Default, Debug)]
pub struct Process {
    pub pid: u32,
    pub start_time: u64,
    pub tcp: Option<u32>,
    pub udp: Option<u32>,
    pub name: String,
    pub command: String,
    pub user: String,
    pub state: String,
    pub cpu: f32,
    pub memory: u64,
    pub threads: u64,
    pub gpu_memory: Option<u64>,
    pub gpu: Option<u32>,
    pub kind: String,
}
#[derive(Clone, Default, Debug)]
pub struct Gpu {
    pub name: String,
    pub uuid: String,
    pub utilization: Option<u32>,
    pub memory_used: u64,
    pub memory_total: u64,
    pub clock: Option<u32>,
    pub memory_clock: Option<u32>,
    pub temperature: Option<u32>,
    pub fan: Option<u32>,
    pub power: Option<u32>,
    pub power_limit: Option<u32>,
    pub rx: Option<u32>,
    pub tx: Option<u32>,
    pub generation: Option<u32>,
    pub width: Option<u32>,
    pub encoder: Option<u32>,
    pub decoder: Option<u32>,
    pub processes: Vec<Process>,
    pub process_error: Option<String>,
}
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub at: Instant,
    pub cpu_name: String,
    pub cores: Vec<f32>,
    pub cpu: f32,
    pub temperature: Option<f32>,
    pub memory_active: u64,
    pub memory_cached: u64,
    pub memory_buffers: u64,
    pub memory_free: u64,
    pub memory_used: u64,
    pub memory_total: u64,
    pub swap_used: u64,
    pub swap_total: u64,
    pub processes: Vec<Process>,
    pub gpus: Vec<Gpu>,
    pub gpu_error: Option<String>,
    pub sample_ms: f32,
    pub network: crate::network::Network,
}
impl Default for Snapshot {
    fn default() -> Self {
        Self {
            at: Instant::now(),
            cpu_name: String::new(),
            cores: vec![],
            cpu: 0.,
            temperature: None,
            memory_active: 0,
            memory_cached: 0,
            memory_buffers: 0,
            memory_free: 0,
            memory_used: 0,
            memory_total: 0,
            swap_used: 0,
            swap_total: 0,
            processes: vec![],
            gpus: vec![],
            gpu_error: None,
            sample_ms: 0.,
            network: Default::default(),
        }
    }
}
pub struct Feed {
    pub latest: Arc<Mutex<Option<Arc<Snapshot>>>>,
    pub interval: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
}
impl Feed {
    pub fn start(interval: u64) -> Self {
        let latest = Arc::new(Mutex::new(None));
        let rate = Arc::new(AtomicU64::new(interval));
        let stop = Arc::new(AtomicBool::new(false));
        let (out, poll, quit) = (latest.clone(), rate.clone(), stop.clone());
        thread::Builder::new()
            .name("strata-sampler".into())
            .spawn(move || {
                let mut sampler = Sampler::new();
                while !quit.load(Ordering::Relaxed) {
                    let start = Instant::now();
                    let sample = sampler.sample();
                    if let Ok(mut slot) = out.lock() {
                        *slot = Some(Arc::new(sample));
                    }
                    let deadline = start + Duration::from_millis(poll.load(Ordering::Relaxed));
                    while Instant::now() < deadline && !quit.load(Ordering::Relaxed) {
                        thread::sleep(Duration::from_millis(50));
                    }
                }
            })
            .expect("start sampler");
        Self {
            latest,
            interval: rate,
            stop,
        }
    }
}
impl Drop for Feed {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[derive(Clone, Copy, Default)]
struct CpuTime {
    total: u64,
    idle: u64,
}
fn cpu_times(text: &str) -> Vec<CpuTime> {
    text.lines()
        .take_while(|l| l.starts_with("cpu"))
        .map(|l| {
            // guest and guest_nice are already included in user and nice.
            let v: Vec<u64> = l
                .split_whitespace()
                .skip(1)
                .take(8)
                .map(|s| s.parse().unwrap_or(0))
                .collect();
            CpuTime {
                total: v.iter().sum(),
                idle: v.get(3).unwrap_or(&0) + v.get(4).unwrap_or(&0),
            }
        })
        .collect()
}
fn usage(now: CpuTime, old: CpuTime) -> f32 {
    let total = now.total.saturating_sub(old.total);
    if total == 0 {
        return 0.;
    }
    (100. * (total.saturating_sub(now.idle.saturating_sub(old.idle))) as f32 / total as f32)
        .clamp(0., 100.)
}
fn meminfo(text: &str) -> (u64, u64, u64, u64) {
    let m: HashMap<&str, u64> = text
        .lines()
        .filter_map(|l| {
            let mut s = l.split_whitespace();
            Some((
                s.next()?.trim_end_matches(':'),
                s.next()?.parse::<u64>().ok()? * 1024,
            ))
        })
        .collect();
    let total = *m.get("MemTotal").unwrap_or(&0);
    let swap = *m.get("SwapTotal").unwrap_or(&0);
    (
        total.saturating_sub(*m.get("MemAvailable").unwrap_or(&0)),
        total,
        swap.saturating_sub(*m.get("SwapFree").unwrap_or(&0)),
        swap,
    )
}
// Cache excludes shared memory (which cannot simply be reclaimed) and includes
// reclaimable slab. The four disjoint segments sum to physical memory.
fn memory_breakdown(text: &str) -> [u64; 4] {
    let m: HashMap<_, _> = text
        .lines()
        .filter_map(|l| {
            let mut s = l.split_whitespace();
            Some((
                s.next()?.trim_end_matches(':'),
                s.next()?.parse::<u64>().ok()? * 1024,
            ))
        })
        .collect();
    let get = |k| *m.get(k).unwrap_or(&0);
    let total = get("MemTotal");
    let free = get("MemFree").min(total);
    let buffers = get("Buffers").min(total - free);
    let cache = (get("Cached")
        .saturating_add(get("SReclaimable"))
        .saturating_sub(get("Shmem")))
    .min(total - free - buffers);
    [total - free - buffers - cache, cache, buffers, free]
}
// Return start time as well as CPU ticks so recycled PIDs never inherit old usage.
fn parse_stat(text: &str) -> Option<(String, String, u64, u64, u64)> {
    let left = text.find('(')?;
    let right = text.rfind(')')?;
    let v: Vec<&str> = text.get(right + 2..)?.split_whitespace().collect();
    Some((
        text[left + 1..right].into(),
        v.first()?.to_string(),
        v.get(11)?.parse::<u64>().ok()? + v.get(12)?.parse::<u64>().ok()?,
        v.get(19)?.parse().ok()?,
        v.get(17)?.parse().ok()?,
    ))
}
pub struct Sampler {
    previous: Vec<CpuTime>,
    network: crate::network::NetworkSampler,
    pids: HashMap<u32, (u64, u64)>,
    users: HashMap<u32, String>,
    name: String,
    thermal: Vec<String>,
    nvml: Option<Nvml>,
    nvml_error: Option<String>,
    last_gpu_stamp: HashMap<String, u64>,
}
impl Sampler {
    pub fn new() -> Self {
        let info = fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
        let name = info
            .lines()
            .find_map(|l| {
                l.strip_prefix("model name")
                    .and_then(|s| s.split_once(':'))
                    .map(|(_, s)| s.trim().to_string())
            })
            .unwrap_or("CPU".into());
        let users = fs::read_to_string("/etc/passwd")
            .unwrap_or_default()
            .lines()
            .filter_map(|l| {
                let s: Vec<_> = l.split(':').collect();
                Some((s.get(2)?.parse().ok()?, s.first()?.to_string()))
            })
            .collect();
        let mut thermal = vec![];
        if let Ok(entries) = fs::read_dir("/sys/class/hwmon") {
            for e in entries.flatten() {
                let path = e.path();
                let name = fs::read_to_string(path.join("name")).unwrap_or_default();
                if ["coretemp", "k10temp", "zenpower", "cpu_thermal"].contains(&name.trim())
                    && let Ok(files) = fs::read_dir(path)
                {
                    for f in files.flatten() {
                        let n = f.file_name().to_string_lossy().to_string();
                        if n.starts_with("temp") && n.ends_with("_input") {
                            thermal.push(f.path().to_string_lossy().into_owned());
                        }
                    }
                }
            }
        }
        let (nvml, nvml_error) = match Nvml::init() {
            Ok(n) => (Some(n), None),
            Err(e) => (None, Some(format!("NVIDIA unavailable: {e}"))),
        };
        Self {
            previous: vec![],
            network: crate::network::NetworkSampler::new(),
            pids: HashMap::new(),
            users,
            name,
            thermal,
            nvml,
            nvml_error,
            last_gpu_stamp: HashMap::new(),
        }
    }
    pub fn sample(&mut self) -> Snapshot {
        let start = Instant::now();
        let times = cpu_times(&fs::read_to_string("/proc/stat").unwrap_or_default());
        let delta = times
            .first()
            .zip(self.previous.first())
            .map(|(a, b)| a.total.saturating_sub(b.total))
            .unwrap_or(0);
        let util: Vec<_> = times
            .iter()
            .enumerate()
            .map(|(i, t)| self.previous.get(i).map(|p| usage(*t, *p)).unwrap_or(0.))
            .collect();
        let count = times.len().saturating_sub(1);
        self.previous = times;
        let memory_text = fs::read_to_string("/proc/meminfo").unwrap_or_default();
        let (memory_used, memory_total, swap_used, swap_total) = meminfo(&memory_text);
        let [memory_active, memory_cached, memory_buffers, memory_free] =
            memory_breakdown(&memory_text);
        let temperature = self
            .thermal
            .iter()
            .filter_map(|p| fs::read_to_string(p).ok()?.trim().parse::<f32>().ok())
            .map(|t| t / 1000.)
            .filter(|t| (0. ..150.).contains(t))
            .reduce(f32::max);
        let mut processes = vec![];
        let mut next_pids = HashMap::new();
        if let Ok(entries) = fs::read_dir("/proc") {
            for e in entries.flatten() {
                let Ok(pid) = e.file_name().to_string_lossy().parse::<u32>() else {
                    continue;
                };
                let path = e.path();
                let Ok(stat) = fs::read_to_string(path.join("stat")) else {
                    continue;
                };
                let Some((name, state, ticks, born, threads)) = parse_stat(&stat) else {
                    continue;
                };
                let cpu = self
                    .pids
                    .get(&pid)
                    .filter(|(_, b)| *b == born)
                    .map(|(old, _)| {
                        if delta > 0 {
                            ticks.saturating_sub(*old) as f32 * count as f32 * 100. / delta as f32
                        } else {
                            0.
                        }
                    })
                    .unwrap_or(0.);
                next_pids.insert(pid, (ticks, born));
                let status = fs::read_to_string(path.join("status")).unwrap_or_default();
                let uid = status.lines().find_map(|l| {
                    l.strip_prefix("Uid:")?
                        .split_whitespace()
                        .next()?
                        .parse::<u32>()
                        .ok()
                });
                let memory = status
                    .lines()
                    .find_map(|l| {
                        l.strip_prefix("VmRSS:")?
                            .split_whitespace()
                            .next()?
                            .parse::<u64>()
                            .ok()
                    })
                    .unwrap_or(0)
                    * 1024;
                let command = fs::read(path.join("cmdline"))
                    .map(|b| {
                        String::from_utf8_lossy(&b)
                            .replace('\0', " ")
                            .trim()
                            .to_string()
                    })
                    .unwrap_or_default();
                let user = uid
                    .map(|u| self.users.get(&u).cloned().unwrap_or(u.to_string()))
                    .unwrap_or("—".into());
                processes.push(Process {
                    pid,
                    start_time: born,
                    name,
                    command,
                    user,
                    state,
                    cpu,
                    memory,
                    threads,
                    ..Default::default()
                });
            }
        }
        self.pids = next_pids;
        let network = self.network.sample(&mut processes);
        let mut gpus = vec![];
        let mut gpu_error = self.nvml_error.clone();
        if let Some(nvml) = &self.nvml {
            match nvml.device_count() {
                Err(e) => gpu_error = Some(e.to_string()),
                Ok(count) => {
                    for i in 0..count {
                        let device = match nvml.device_by_index(i) {
                            Ok(d) => d,
                            Err(e) => {
                                gpu_error = Some(e.to_string());
                                continue;
                            }
                        };
                        let memory = device.memory_info().ok();
                        let uuid = device.uuid().unwrap_or_else(|_| format!("gpu-{i}"));
                        let mut gpu = Gpu {
                            name: device.name().unwrap_or_else(|_| format!("GPU {i}")),
                            uuid: uuid.clone(),
                            utilization: device.utilization_rates().ok().map(|u| u.gpu),
                            memory_used: memory.as_ref().map(|m| m.used).unwrap_or(0),
                            memory_total: memory.map(|m| m.total).unwrap_or(0),
                            clock: device.clock_info(Clock::Graphics).ok(),
                            memory_clock: device.clock_info(Clock::Memory).ok(),
                            temperature: device.temperature(TemperatureSensor::Gpu).ok(),
                            fan: device.fan_speed(0).ok(),
                            power: device.power_usage().ok(),
                            power_limit: device.enforced_power_limit().ok(),
                            rx: device.pcie_throughput(PcieUtilCounter::Receive).ok(),
                            tx: device.pcie_throughput(PcieUtilCounter::Send).ok(),
                            generation: device.current_pcie_link_gen().ok(),
                            width: device.current_pcie_link_width().ok(),
                            encoder: device.encoder_utilization().ok().map(|u| u.utilization),
                            decoder: device.decoder_utilization().ok().map(|u| u.utilization),
                            ..Default::default()
                        };
                        let stamp = self.last_gpu_stamp.entry(uuid).or_default();
                        let samples = device.process_utilization_stats(*stamp).ok();
                        let mut usages = HashMap::new();
                        for s in samples.unwrap_or_default() {
                            *stamp = (*stamp).max(s.timestamp);
                            usages.insert(s.pid, s.sm_util);
                        }
                        let mut merged: HashMap<u32, Process> = HashMap::new();
                        for (kind, result) in [
                            ("Graphics", device.running_graphics_processes()),
                            ("Compute", device.running_compute_processes()),
                        ] {
                            match result {
                                Err(e) => {
                                    gpu.process_error =
                                        Some(format!("Some process data unavailable: {e}"))
                                }
                                Ok(ps) => {
                                    for p in ps {
                                        let memory = match p.used_gpu_memory {
                                            UsedGpuMemory::Used(v) => Some(v),
                                            _ => None,
                                        };
                                        let row = merged.entry(p.pid).or_insert_with(|| {
                                            processes
                                                .iter()
                                                .find(|r| r.pid == p.pid)
                                                .cloned()
                                                .unwrap_or(Process {
                                                    pid: p.pid,
                                                    name: "Exited / inaccessible".into(),
                                                    ..Default::default()
                                                })
                                        });
                                        row.kind = if row.kind.is_empty() {
                                            kind.into()
                                        } else {
                                            "G + C".into()
                                        };
                                        row.gpu_memory = row.gpu_memory.max(memory);
                                        row.gpu = usages.get(&p.pid).copied();
                                    }
                                }
                            }
                        }
                        gpu.processes = merged.into_values().collect();
                        gpu.processes
                            .sort_by_key(|p| std::cmp::Reverse(p.gpu_memory));
                        gpus.push(gpu);
                    }
                }
            }
        }
        Snapshot {
            at: Instant::now(),
            cpu_name: self.name.clone(),
            cores: util.iter().skip(1).copied().collect(),
            cpu: util.first().copied().unwrap_or(0.),
            temperature,
            memory_active,
            memory_cached,
            memory_buffers,
            memory_free,
            memory_used,
            memory_total,
            swap_used,
            swap_total,
            processes,
            gpus,
            gpu_error,
            sample_ms: start.elapsed().as_secs_f32() * 1000.,
            network,
        }
    }
}
pub fn bytes(n: u64) -> String {
    if n >= 1024 * 1024 * 1024 {
        format!("{:.1} GiB", n as f64 / (1024. * 1024. * 1024.))
    } else {
        format!("{:.0} MiB", n as f64 / (1024. * 1024.))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cpu_excludes_guest_and_counts_iowait_idle() {
        let a = cpu_times(
            "cpu 100 20 30 400 50 5 5 0 80 10\ncpu0 100 20 30 400 50 5 5 0 80 10\nintr 5",
        );
        assert_eq!(a[0].total, 610);
        assert_eq!(a[0].idle, 450);
        assert!((usage(a[0], CpuTime::default()) - 26.2295).abs() < 0.001);
        assert_eq!(usage(a[0], a[0]), 0.);
    }
    #[test]
    fn process_names_can_contain_parentheses_and_spaces() {
        let s = "123 (a (worker) job) S 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21";
        let (name, state, ticks, born, threads) = parse_stat(s).unwrap();
        assert_eq!(name, "a (worker) job");
        assert_eq!(state, "S");
        assert_eq!(ticks, 23);
        assert_eq!(born, 19);
        assert_eq!(threads, 17);
    }
    #[test]
    fn memory_segments_do_not_double_count_shared_memory() {
        let b = memory_breakdown(
            "MemTotal: 100 kB\nMemFree: 10 kB\nBuffers: 5 kB\nCached: 50 kB\nSReclaimable: 10 kB\nShmem: 15 kB",
        );
        assert_eq!(b, [40 * 1024, 45 * 1024, 5 * 1024, 10 * 1024]);
        assert_eq!(b.iter().sum::<u64>(), 100 * 1024);
    }
    #[test]
    fn memory_uses_available_and_handles_no_swap() {
        assert_eq!(
            meminfo("MemTotal: 100 kB\nMemAvailable: 40 kB\nSwapTotal: 0 kB\nSwapFree: 0 kB"),
            (60 * 1024, 100 * 1024, 0, 0)
        );
    }
}
