//! Small OS probes, all executed on the diagnostic worker. Unavailable fields
//! remain explicit; no elevated access or external processes are needed.

use super::Field;
use std::path::PathBuf;

pub(super) struct Collector {
    config_directory: PathBuf,
    previous_cpu: Option<CpuSample>,
}

impl Collector {
    pub(super) fn new(config_directory: PathBuf) -> Self {
        Self {
            config_directory,
            previous_cpu: None,
        }
    }

    pub(super) fn reset_cpu_sample(&mut self) {
        self.previous_cpu = None;
    }

    pub(super) fn collect(&mut self) -> Vec<Field> {
        let mut fields = vec![
            Field::new(
                "version",
                "Application · Build",
                format!(
                    "{} · {} · {}",
                    env!("CARGO_PKG_VERSION"),
                    env!("SPACEWARS_BUILD_REVISION"),
                    if cfg!(debug_assertions) {
                        "debug"
                    } else {
                        "release"
                    }
                ),
            ),
            Field::new(
                "platform",
                "Device · Platform",
                format!("{} / {}", std::env::consts::OS, std::env::consts::ARCH),
            ),
        ];
        #[cfg(target_os = "linux")]
        self.linux_fields(&mut fields);
        #[cfg(not(target_os = "linux"))]
        fields.push(Field::new(
            "system",
            "Device · System metrics",
            "Available on Linux kiosks; not available on this platform.",
        ));
        fields.push(Field::new(
            "config-directory",
            "Application · Configuration directory",
            self.config_directory.display().to_string(),
        ));
        fields
    }

    #[cfg(target_os = "linux")]
    fn linux_fields(&mut self, fields: &mut Vec<Field>) {
        use nix::sys::statvfs::statvfs;
        let hostname = nix::unistd::gethostname()
            .ok()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Unavailable".into());
        fields.insert(0, Field::new("hostname", "Device · Hostname", hostname));
        let model = read_text("/sys/firmware/devicetree/base/model")
            .or_else(|| read_text("/sys/devices/virtual/dmi/id/product_name"))
            .unwrap_or_else(|| "Unavailable".into());
        fields.push(Field::new("model", "Device · Hardware", model));
        fields.push(Field::new(
            "addresses",
            "Network · Local addresses (not an internet connectivity test)",
            network_addresses(),
        ));
        let cpu_info = read_text("/proc/cpuinfo").unwrap_or_default();
        let cpu_model = cpu_info
            .lines()
            .find_map(|line| {
                line.strip_prefix("model name")
                    .or_else(|| line.strip_prefix("Hardware"))
                    .and_then(|line| line.split_once(':'))
                    .map(|(_, value)| value.trim())
            })
            .unwrap_or(std::env::consts::ARCH);
        let cores = cpu_info
            .lines()
            .filter(|line| line.starts_with("processor\t"))
            .count();
        fields.push(Field::new(
            "cpu",
            "System · CPU",
            format!("{cpu_model} · {cores} logical cores"),
        ));
        let current = read_text("/proc/stat").and_then(|text| CpuSample::parse(&text));
        let usage = current
            .zip(self.previous_cpu)
            .and_then(|(current, previous)| current.usage_since(previous));
        self.previous_cpu = current;
        fields.push(Field::new(
            "cpu-usage",
            "System · CPU busy (all cores, 0–100%)",
            match (current, usage) {
                (_, Some(percent)) => format!("{percent:.1}%"),
                (Some(_), None) => "Sampling…".into(),
                _ => "Unavailable".into(),
            },
        ));
        let memory = read_text("/proc/meminfo")
            .and_then(|text| memory(&text))
            .map(|(available, total)| {
                format!("{} available / {} total", size(available), size(total))
            })
            .unwrap_or_else(|| "Unavailable".into());
        fields.push(Field::new("memory", "System · Memory", memory));
        let temperature = read_text("/sys/class/thermal/thermal_zone0/temp")
            .and_then(|text| text.parse::<f64>().ok())
            .map(|temp| format!("{:.1} °C", temp / 1000.0))
            .unwrap_or_else(|| "Unavailable".into());
        fields.push(Field::new(
            "temperature",
            "System · Thermal zone 0",
            temperature,
        ));
        // /data is the Pi's persistent volume. Desktop runs report the actual
        // config filesystem instead, never silently label / free space as data.
        let data_path = if std::path::Path::new("/data/spacewars").is_dir() {
            PathBuf::from("/data/spacewars")
        } else {
            self.config_directory.clone()
        };
        let disk = statvfs(&data_path)
            .ok()
            .map(|disk| {
                format!(
                    "{} available / {} total",
                    size(disk.blocks_available().saturating_mul(disk.fragment_size())),
                    size(disk.blocks().saturating_mul(disk.fragment_size()))
                )
            })
            .unwrap_or_else(|| "Unavailable".into());
        fields.push(Field::new(
            "storage",
            "Storage · Persistent data / config filesystem",
            format!("{}\n{disk}", data_path.display()),
        ));
    }
}

#[cfg(target_os = "linux")]
fn read_text(path: &str) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|text| {
            text.trim_matches(|ch: char| ch == '\0' || ch.is_whitespace())
                .to_owned()
        })
        .filter(|text| !text.is_empty())
}

#[cfg(target_os = "linux")]
fn network_addresses() -> String {
    use nix::net::if_::InterfaceFlags;
    let Ok(addresses) = nix::ifaddrs::getifaddrs() else {
        return "Unavailable".into();
    };
    let mut rows = std::collections::BTreeSet::new();
    for interface in addresses {
        if !interface.flags.contains(InterfaceFlags::IFF_UP)
            || interface.flags.contains(InterfaceFlags::IFF_LOOPBACK)
        {
            continue;
        }
        let Some(address) = interface.address else {
            continue;
        };
        let ip = if let Some(address) = address.as_sockaddr_in() {
            address.ip().to_string()
        } else if let Some(address) = address.as_sockaddr_in6() {
            address.ip().to_string()
        } else {
            continue;
        };
        rows.insert(format!("{}: {ip}", interface.interface_name));
    }
    if rows.is_empty() {
        "No addresses on active network interfaces".into()
    } else {
        rows.into_iter().collect::<Vec<_>>().join("\n")
    }
}

#[derive(Debug, Clone, Copy)]
struct CpuSample {
    busy: u64,
    total: u64,
}

impl CpuSample {
    fn parse(text: &str) -> Option<Self> {
        let mut values = text.lines().next()?.split_whitespace();
        if values.next()? != "cpu" {
            return None;
        }
        // guest/guest_nice are already counted in user/nice, don't count twice.
        let values = values
            .take(8)
            .map(str::parse::<u64>)
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        if values.len() < 4 {
            return None;
        }
        let total = values
            .iter()
            .try_fold(0u64, |total, value| total.checked_add(*value))?;
        let idle = values[3].checked_add(values.get(4).copied().unwrap_or(0))?;
        Some(Self {
            busy: total.checked_sub(idle)?,
            total,
        })
    }

    fn usage_since(self, previous: Self) -> Option<f64> {
        let elapsed = self.total.checked_sub(previous.total)?;
        let busy = self.busy.checked_sub(previous.busy)?;
        (elapsed > 0 && busy <= elapsed).then(|| busy as f64 * 100.0 / elapsed as f64)
    }
}

fn memory(text: &str) -> Option<(u64, u64)> {
    let value = |key: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(key))?
            .split_whitespace()
            .next()?
            .parse::<u64>()
            .ok()?
            .checked_mul(1024)
    };
    let total = value("MemTotal:")?;
    let available = value("MemAvailable:")?;
    (available <= total && total > 0).then_some((available, total))
}

fn size(bytes: u64) -> String {
    format!("{:.2} GiB", bytes as f64 / 1_073_741_824.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_usage_uses_deltas_and_excludes_guest_double_counting() {
        let first = CpuSample::parse("cpu 10 0 10 80 0 0 0 0 999 999\ncpu0 1").unwrap();
        let second = CpuSample::parse("cpu 20 0 20 100 0 0 0 0 999 999").unwrap();
        assert_eq!(first.total, 100);
        assert_eq!(second.usage_since(first), Some(50.0));
        assert_eq!(first.usage_since(first), None);
        assert_eq!(first.usage_since(second), None);
        for text in ["", "cpu0 1 2 3 4", "cpu 1 2", "cpu x 2 3 4"] {
            assert!(CpuSample::parse(text).is_none());
        }
    }

    #[test]
    fn memory_reports_available_instead_of_only_unused_pages() {
        assert_eq!(
            memory("MemTotal: 8192 kB\nMemFree: 10 kB\nMemAvailable: 4096 kB"),
            Some((4194304, 8388608))
        );
        assert_eq!(memory("MemTotal: 10 kB\nMemFree: 5 kB"), None);
        assert_eq!(memory("MemTotal: 10 kB\nMemAvailable: 20 kB"), None);
        assert_eq!(size(1073741824), "1.00 GiB");
    }
}
