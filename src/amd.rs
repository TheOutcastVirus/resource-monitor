//! Unprivileged AMD telemetry from the kernel's amdgpu sysfs and DRM fdinfo APIs.
use crate::metrics::{Gpu, Process};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

pub struct AmdSampler {
    devices: Vec<Device>,
    clients: HashMap<(String, u64), ClientSample>,
}

struct Device {
    path: PathBuf,
    hwmon: Option<PathBuf>,
    slot: String,
    name: String,
}

struct ClientSample {
    at: Instant,
    engines: HashMap<String, u64>,
}

#[derive(Default)]
struct ProcessUsage {
    memory: Option<u64>,
    utilization: Option<f64>,
    graphics: bool,
    compute: bool,
}

#[derive(Default)]
struct Client {
    slot: String,
    id: u64,
    memory: Option<u64>,
    engines: HashMap<String, u64>,
}

fn number(path: impl AsRef<Path>) -> Option<u64> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn scaled(path: impl AsRef<Path>, divisor: u64) -> Option<u32> {
    u32::try_from(number(path)? / divisor).ok()
}

fn hex(path: impl AsRef<Path>) -> Option<u32> {
    let text = fs::read_to_string(path).ok()?;
    u32::from_str_radix(text.trim().trim_start_matches("0x"), 16).ok()
}

fn marketing_name(ids: &str, device: u32, revision: u32) -> Option<String> {
    ids.lines().find_map(|line| {
        let mut fields = line.splitn(3, ',').map(str::trim);
        let id = u32::from_str_radix(fields.next()?, 16).ok()?;
        let rev = u32::from_str_radix(fields.next()?, 16).ok()?;
        let name = fields.next()?;
        (id == device && rev == revision && !name.is_empty()).then(|| name.to_string())
    })
}

fn active_clock(text: &str) -> Option<u32> {
    let line = text.lines().find(|line| line.contains('*'))?;
    let clock = line.split_once(':')?.1.split_whitespace().next()?;
    clock
        .strip_suffix("Mhz")
        .or_else(|| clock.strip_suffix("MHz"))?
        .parse()
        .ok()
}

fn link_generation(text: &str) -> Option<u32> {
    match text.split_whitespace().next()? {
        "2.5" => Some(1),
        "5.0" => Some(2),
        "8.0" => Some(3),
        "16.0" => Some(4),
        "32.0" => Some(5),
        "64.0" => Some(6),
        _ => None,
    }
}

impl Device {
    fn discover(root: &Path, ids: &str) -> Vec<Self> {
        let Ok(entries) = fs::read_dir(root) else {
            return vec![];
        };
        let mut devices = vec![];
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            // DRM cards need not start at card0; skip connectors and render nodes.
            if !name.strip_prefix("card").is_some_and(|suffix| {
                !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit())
            }) {
                continue;
            }
            let path = entry.path().join("device");
            let uevent = fs::read_to_string(path.join("uevent")).unwrap_or_default();
            if !uevent.lines().any(|line| line == "DRIVER=amdgpu") {
                continue;
            }
            let Some(slot) = uevent
                .lines()
                .find_map(|l| l.strip_prefix("PCI_SLOT_NAME="))
            else {
                continue;
            };
            let device = hex(path.join("device"));
            let revision = hex(path.join("revision"));
            let name = fs::read_to_string(path.join("product_name"))
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .or_else(|| marketing_name(ids, device?, revision?))
                .unwrap_or_else(|| match device {
                    Some(id) => format!("AMD GPU ({id:04x})"),
                    None => "AMD GPU".into(),
                });
            let hwmon = fs::read_dir(path.join("hwmon")).ok().and_then(|entries| {
                entries.flatten().map(|e| e.path()).find(|p| {
                    fs::read_to_string(p.join("name")).is_ok_and(|n| n.trim() == "amdgpu")
                })
            });
            devices.push(Self {
                path,
                hwmon,
                slot: slot.to_string(),
                name,
            });
        }
        devices.sort_by(|a, b| a.slot.cmp(&b.slot));
        devices.dedup_by(|a, b| a.slot == b.slot);
        devices
    }

    fn sample(&self) -> Gpu {
        let sensor = |file: &str, divisor| scaled(self.hwmon.as_ref()?.join(file), divisor);
        let clock = |file| active_clock(&fs::read_to_string(self.path.join(file)).ok()?);
        let fan = self.hwmon.as_ref().and_then(|p| {
            let max = number(p.join("pwm1_max")).unwrap_or(255);
            (max > 0).then_some(())?;
            Some((number(p.join("pwm1"))?.saturating_mul(100) / max).min(100) as u32)
        });
        Gpu {
            name: self.name.clone(),
            uuid: format!("amdgpu-{}", self.slot),
            utilization: scaled(self.path.join("gpu_busy_percent"), 1).filter(|v| *v <= 100),
            memory_used: number(self.path.join("mem_info_vram_used")).unwrap_or(0),
            memory_total: number(self.path.join("mem_info_vram_total")).unwrap_or(0),
            clock: sensor("freq1_input", 1_000_000).or_else(|| clock("pp_dpm_sclk")),
            memory_clock: sensor("freq2_input", 1_000_000).or_else(|| clock("pp_dpm_mclk")),
            temperature: sensor("temp1_input", 1000),
            fan,
            // hwmon reports microwatts; the shared GPU model uses milliwatts.
            power: sensor("power1_average", 1000).or_else(|| sensor("power1_input", 1000)),
            power_limit: sensor("power1_cap", 1000),
            generation: fs::read_to_string(self.path.join("current_link_speed"))
                .ok()
                .and_then(|s| link_generation(&s)),
            width: scaled(self.path.join("current_link_width"), 1).filter(|v| *v > 0),
            ..Default::default()
        }
    }
}

fn memory_bytes(text: &str) -> Option<u64> {
    let mut fields = text.split_whitespace();
    let value: u64 = fields.next()?.parse().ok()?;
    value.checked_mul(match fields.next() {
        None | Some("B") => 1,
        Some("KiB") => 1024,
        Some("MiB") => 1024 * 1024,
        _ => return None,
    })
}

fn client(text: &str) -> Option<Client> {
    let fields: HashMap<_, _> = text
        .lines()
        .filter_map(|l| l.split_once(':').map(|(k, v)| (k, v.trim())))
        .collect();
    if *fields.get("drm-driver")? != "amdgpu" {
        return None;
    }
    Some(Client {
        slot: fields.get("drm-pdev")?.to_string(),
        id: fields.get("drm-client-id")?.parse().ok()?,
        memory: fields
            .get("drm-memory-vram")
            .or_else(|| fields.get("drm-total-vram"))
            .and_then(|s| memory_bytes(s)),
        engines: fields
            .iter()
            .filter_map(|(key, value)| {
                let engine = key.strip_prefix("drm-engine-")?;
                if engine != "gfx" && !engine.starts_with("compute") {
                    return None;
                }
                let (value, unit) = value.split_once(' ')?;
                (unit.trim() == "ns").then_some(())?;
                Some((engine.to_string(), value.parse().ok()?))
            })
            .collect(),
    })
}

fn utilization(current: &mut ClientSample, previous: Option<&ClientSample>) -> Option<f64> {
    let previous = previous?;
    let elapsed = current.at.duration_since(previous.at).as_secs_f64() * 1e9;
    if elapsed == 0. {
        return None;
    }
    let mut delta = None;
    for (engine, value) in &mut current.engines {
        if let Some(old) = previous.engines.get(engine) {
            // DRM counters may briefly regress. Keep the high-water mark.
            *value = (*value).max(*old);
            *delta.get_or_insert(0u64) += *value - old;
        }
    }
    delta.map(|ns| ns as f64 * 100. / elapsed)
}

impl AmdSampler {
    pub fn new() -> Self {
        let ids = [
            "/usr/share/libdrm/amdgpu.ids",
            "/usr/share/hwdata/amdgpu.ids",
        ]
        .iter()
        .find_map(|p| fs::read_to_string(p).ok())
        .unwrap_or_default();
        Self {
            devices: Device::discover(Path::new("/sys/class/drm"), &ids),
            clients: HashMap::new(),
        }
    }

    pub fn sample(&mut self, processes: &[Process]) -> Vec<Gpu> {
        self.sample_at(Path::new("/proc"), processes, Instant::now())
    }

    fn sample_at(&mut self, root: &Path, processes: &[Process], now: Instant) -> Vec<Gpu> {
        let mut gpus: Vec<_> = self.devices.iter().map(Device::sample).collect();
        let mut seen = HashSet::new();
        let mut next = HashMap::new();
        // A DRM client shared across processes is attributed to the lowest PID.
        let mut processes: Vec<_> = processes.iter().collect();
        processes.sort_by_key(|p| p.pid);
        for process in processes {
            if gpus.is_empty() {
                break;
            }
            let path = root.join(process.pid.to_string());
            let Ok(fds) = fs::read_dir(path.join("fd")) else {
                continue;
            };
            let mut rows: HashMap<usize, ProcessUsage> = HashMap::new();
            for fd in fds.flatten() {
                // Avoid opening fdinfo for every socket/file in large processes.
                if !fs::read_link(fd.path()).is_ok_and(|p| p.starts_with("/dev/dri")) {
                    continue;
                }
                let Some(client) = fs::read_to_string(path.join("fdinfo").join(fd.file_name()))
                    .ok()
                    .and_then(|s| client(&s))
                else {
                    continue;
                };
                let Some(index) = self.devices.iter().position(|d| d.slot == client.slot) else {
                    continue;
                };
                let key = (client.slot, client.id);
                if !seen.insert(key.clone()) {
                    continue;
                }
                let graphics = client.engines.contains_key("gfx");
                let compute = client.engines.keys().any(|k| k.starts_with("compute"));
                let mut current = ClientSample {
                    at: now,
                    engines: client.engines,
                };
                let usage = utilization(&mut current, self.clients.get(&key));
                next.insert(key, current);
                let row = rows.entry(index).or_default();
                if let Some(memory) = client.memory {
                    *row.memory.get_or_insert(0) += memory;
                }
                if let Some(usage) = usage {
                    *row.utilization.get_or_insert(0.) += usage;
                }
                row.graphics |= graphics;
                row.compute |= compute;
            }
            for (index, usage) in rows {
                let mut row = process.clone();
                row.gpu_memory = usage.memory;
                row.gpu = usage.utilization.map(|v| v.round().clamp(0., 100.) as u32);
                row.kind = match (usage.graphics, usage.compute) {
                    (true, true) => "G + C",
                    (_, true) => "Compute",
                    (true, _) => "Graphics",
                    _ => "DRM",
                }
                .into();
                gpus[index].processes.push(row);
            }
        }
        self.clients = next;
        for gpu in &mut gpus {
            gpu.processes
                .sort_by_key(|p| std::cmp::Reverse(p.gpu_memory));
        }
        gpus
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        os::unix::fs::symlink,
        sync::atomic::{AtomicU64, Ordering},
        time::Duration,
    };

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "strata-amd-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn write(&self, file: &str, text: &str) {
            let path = self.0.join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }

        fn device(&self) -> Device {
            self.write(
                "drm/card1/device/uevent",
                "DRIVER=amdgpu\nPCI_SLOT_NAME=0000:c4:00.0\n",
            );
            self.write("drm/card1/device/device", "0x1114\n");
            self.write("drm/card1/device/revision", "0xd2\n");
            self.write("drm/card1/device/hwmon/hwmon7/name", "amdgpu\n");
            Device::discover(
                &self.0.join("drm"),
                "1114, D3, Radeon 840M\n1114, D2, Radeon 860M\n",
            )
            .remove(0)
        }

        fn fd(&self, pid: u32, fd: u32, text: &str) {
            self.write(&format!("proc/{pid}/fdinfo/{fd}"), text);
            let dir = self.0.join(format!("proc/{pid}/fd"));
            fs::create_dir_all(&dir).unwrap();
            let path = dir.join(fd.to_string());
            if !path.exists() && fs::symlink_metadata(&path).is_err() {
                symlink("/dev/dri/renderD128", path).unwrap();
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn detects_card1_skips_connectors_and_converts_sensor_units() {
        let fixture = Fixture::new();
        fixture.device();
        fixture.write(
            "drm/card1-DP-1/device/uevent",
            "DRIVER=amdgpu\nPCI_SLOT_NAME=0000:c4:00.0\n",
        );
        fixture.write(
            "drm/renderD128/device/uevent",
            "DRIVER=amdgpu\nPCI_SLOT_NAME=0000:c4:00.0\n",
        );
        fixture.write(
            "drm/card0/device/uevent",
            "DRIVER=nvidia\nPCI_SLOT_NAME=0000:01:00.0\n",
        );
        for (file, value) in [
            ("gpu_busy_percent", "58"),
            ("mem_info_vram_used", "4294967296"),
            ("mem_info_vram_total", "8589934592"),
            ("current_link_speed", "16.0 GT/s PCIe"),
            ("current_link_width", "16"),
            ("pp_dpm_mclk", "0: 1000Mhz\n1: 2400Mhz *\n2: 2800Mhz\n"),
            ("hwmon/hwmon7/freq1_input", "2356000000"),
            ("hwmon/hwmon7/temp1_input", "66000"),
            ("hwmon/hwmon7/power1_average", "33681000"),
            ("hwmon/hwmon7/power1_cap", "45000000"),
            ("hwmon/hwmon7/pwm1", "128"),
            ("hwmon/hwmon7/pwm1_max", "255"),
        ] {
            fixture.write(&format!("drm/card1/device/{file}"), value);
        }
        let devices =
            Device::discover(&fixture.0.join("drm"), "1114, D2, AMD Radeon 860M Graphics");
        assert_eq!(devices.len(), 1);
        let gpu = devices[0].sample();
        assert_eq!(gpu.name, "AMD Radeon 860M Graphics");
        assert_eq!(gpu.uuid, "amdgpu-0000:c4:00.0");
        assert_eq!(gpu.utilization, Some(58));
        assert_eq!((gpu.memory_used, gpu.memory_total), (4 << 30, 8 << 30));
        assert_eq!((gpu.clock, gpu.memory_clock), (Some(2356), Some(2400)));
        assert_eq!(gpu.temperature, Some(66));
        assert_eq!((gpu.power, gpu.power_limit), (Some(33681), Some(45000)));
        assert_eq!(gpu.fan, Some(50));
        assert_eq!((gpu.generation, gpu.width), (Some(4), Some(16)));
        assert_eq!(
            (gpu.rx, gpu.tx, gpu.encoder, gpu.decoder),
            (None, None, None, None)
        );
    }

    #[test]
    fn unsupported_sensors_and_unmarked_clocks_remain_unavailable() {
        let fixture = Fixture::new();
        let mut device = fixture.device();
        device.hwmon = None;
        fixture.write("drm/card1/device/pp_dpm_mclk", "0: 1000Mhz\n1: 2400Mhz\n");
        fixture.write("drm/card1/device/pp_dpm_sclk", "0: 600Mhz *\n");
        fixture.write("drm/card1/device/gpu_busy_percent", "invalid");
        let gpu = device.sample();
        assert_eq!(gpu.clock, Some(600));
        assert_eq!(gpu.memory_clock, None);
        assert_eq!(
            (gpu.temperature, gpu.power, gpu.power_limit, gpu.fan),
            (None, None, None, None)
        );
        assert_eq!(gpu.utilization, None);
        assert_eq!(marketing_name("1114, D2, Radeon 860M", 0x1114, 0xd3), None);
    }

    fn fdinfo(id: u64, gfx: u64, compute: u64) -> String {
        format!(
            "drm-driver:\tamdgpu\ndrm-pdev:\t0000:c4:00.0\ndrm-client-id:\t{id}\ndrm-memory-vram:\t1024 KiB\ndrm-total-vram:\t1024 KiB\ndrm-engine-gfx:\t{gfx} ns\ndrm-engine-compute:\t{compute} ns\n"
        )
    }

    #[test]
    fn process_sampling_deduplicates_clients_and_uses_engine_deltas() {
        let fixture = Fixture::new();
        let device = fixture.device();
        let mut sampler = AmdSampler {
            devices: vec![device],
            clients: HashMap::new(),
        };
        let processes = [
            Process {
                pid: 20,
                ..Default::default()
            },
            Process {
                pid: 10,
                ..Default::default()
            },
        ];
        fixture.fd(10, 3, &fdinfo(1, 1_000_000_000, 0));
        fixture.fd(10, 4, &fdinfo(1, 1_000_000_000, 0));
        fixture.fd(20, 5, &fdinfo(1, 1_000_000_000, 0));
        fixture.fd(10, 6, &fdinfo(2, 0, 0));
        let now = Instant::now();
        let first = sampler.sample_at(&fixture.0.join("proc"), &processes, now);
        assert_eq!(first[0].processes.len(), 1);
        assert_eq!(first[0].processes[0].pid, 10);
        assert_eq!(first[0].processes[0].gpu_memory, Some(2 * 1024 * 1024));
        assert_eq!(first[0].processes[0].gpu, None);
        for (pid, fd) in [(10, 3), (10, 4), (20, 5)] {
            fixture.fd(pid, fd, &fdinfo(1, 1_500_000_000, 100_000_000));
        }
        fixture.fd(10, 6, &fdinfo(2, 0, 100_000_000));
        let second = sampler.sample_at(
            &fixture.0.join("proc"),
            &processes,
            now + Duration::from_secs(1),
        );
        assert_eq!(second[0].processes[0].gpu, Some(70));
        assert_eq!(second[0].processes[0].kind, "G + C");
        assert_eq!(sampler.clients.len(), 2);
        sampler.sample_at(&fixture.0.join("proc"), &[], now + Duration::from_secs(2));
        assert!(sampler.clients.is_empty());
    }

    #[test]
    fn counter_regressions_preserve_high_water_mark() {
        let now = Instant::now();
        let sample = |at, value| ClientSample {
            at,
            engines: HashMap::from([("gfx".into(), value)]),
        };
        let previous = sample(now, 1_000_000_000);
        let mut current = sample(now + Duration::from_secs(1), 900_000_000);
        assert_eq!(utilization(&mut current, Some(&previous)), Some(0.));
        assert_eq!(current.engines["gfx"], 1_000_000_000);
        let mut next = sample(now + Duration::from_secs(2), 1_100_000_000);
        assert_eq!(utilization(&mut next, Some(&current)), Some(10.));
    }

    #[test]
    fn parses_fdinfo_without_double_counting_memory_aliases() {
        let parsed = client(&fdinfo(1, 12, 34)).unwrap();
        assert_eq!(parsed.memory, Some(1024 * 1024));
        assert_eq!(parsed.engines.len(), 2);
        assert!(client("drm-driver: i915\ndrm-client-id: 1\ndrm-pdev: 0000:00:02.0").is_none());
        assert!(client("drm-driver: amdgpu\ndrm-client-id: invalid").is_none());
        let parsed = client("drm-driver: amdgpu\ndrm-pdev: 0000:c4:00.0\ndrm-client-id: 2\ndrm-total-vram: 2 MiB\ndrm-engine-gfx: invalid ns\ndrm-engine-capacity-compute: 4\ndrm-engine-dma: 100 ns").unwrap();
        assert_eq!(parsed.memory, Some(2 * 1024 * 1024));
        assert!(parsed.engines.is_empty());
        assert_eq!(memory_bytes("18446744073709551615 KiB"), None);
    }
}
