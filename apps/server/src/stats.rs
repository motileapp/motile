//! What the machine is doing: its CPU, memory, network and disk, sampled every few seconds over
//! the last five minutes, for the clients' look at a server.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use motile_protocol::now;
use motile_protocol::wire::{MachineStats, StatsSample};
use sysinfo::{DiskRefreshKind, Disks, Networks, System};
use tokio::sync::watch;

const EVERY: Duration = Duration::from_secs(3);
const KEPT: usize = 100;
/// Interfaces that loop back, or carry what a real one carries again: a VPN, a bridge, a container's.
const NOT_COUNTED: [&str; 15] = [
    "lo",
    "utun",
    "tun",
    "tap",
    "wg",
    "tailscale",
    "zt",
    "veth",
    "docker",
    "br-",
    "virbr",
    "bridge",
    "awdl",
    "llw",
    "anpi",
];

static STATS: LazyLock<watch::Sender<MachineStats>> = LazyLock::new(|| {
    let (sender, _) = watch::channel(MachineStats::default());
    let sampled = sender.clone();
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"));
    std::thread::Builder::new()
        .name("stats".into())
        .spawn(move || sample(&sampled, &home))
        .expect("the stats thread starts");
    sender
});

/// Starts sampling, so that the first client to look sees the last minutes.
pub fn start() {
    LazyLock::force(&STATS);
}

pub fn watch() -> watch::Receiver<MachineStats> {
    STATS.subscribe()
}

fn sample(stats: &watch::Sender<MachineStats>, home: &Path) {
    let Ok(mut system) = System::new() else {
        return tracing::warn!("couldn't read the machine's stats");
    };
    let mut networks = Networks::new_with_refreshed_list().ok();
    let mut disks = Disks::new_with_refreshed_list().ok();
    system.refresh_cpu_usage();
    let mut samples = VecDeque::with_capacity(KEPT);
    let mut last = Instant::now();
    loop {
        std::thread::sleep(EVERY);
        let elapsed = last.elapsed().as_secs_f64().max(f64::EPSILON);
        last = Instant::now();
        system.refresh_cpu_usage();
        system.refresh_memory();
        let (received, sent) = networks.as_mut().map(|networks| traffic(networks, elapsed)).unwrap_or_default();
        let (disk_used, disk_total) = disks.as_mut().and_then(|disks| disk_of(disks, home)).unwrap_or_default();
        let (memory_used, memory_total) = memory(&system);
        if samples.len() == KEPT {
            samples.pop_front();
        }
        samples.push_back(StatsSample { at: now(), cpu: system.global_cpu_usage(), memory_used, received, sent });
        stats.send_replace(MachineStats {
            cores: system.cpus().len() as u32,
            memory_total,
            disk_used,
            disk_total,
            uptime_secs: System::uptime().unwrap_or_default(),
            every_secs: EVERY.as_secs_f64(),
            samples: samples.iter().copied().collect(),
        });
    }
}

/// In a container with a memory limit, the container's memory rather than the host's.
fn memory(system: &System) -> (u64, u64) {
    let host = (system.used_memory(), system.total_memory());
    match system.cgroup_limits() {
        Some(limits) if limits.total_memory > 0 && limits.total_memory < host.1 => {
            (limits.total_memory.saturating_sub(limits.free_memory), limits.total_memory)
        }
        _ => host,
    }
}

/// Bytes per second received and sent since the last refresh, on the interfaces that count.
fn traffic(networks: &mut Networks, elapsed: f64) -> (u64, u64) {
    networks.refresh(true);
    let counted = networks.iter().filter(|(name, _)| counts(&name.to_string_lossy()));
    let (received, sent) =
        counted.fold((0, 0), |(received, sent), (_, data)| (received + data.received(), sent + data.transmitted()));
    ((received as f64 / elapsed) as u64, (sent as f64 / elapsed) as u64)
}

fn counts(interface: &str) -> bool {
    !NOT_COUNTED.iter().any(|prefix| interface.starts_with(prefix))
}

/// Used and total bytes of the volume `home` is on. What is used is what isn't free, which on
/// APFS counts the volumes that share its container, as Finder does. The whole list is refreshed:
/// on macOS a disk refreshed alone keeps reading the space it first read.
fn disk_of(disks: &mut Disks, home: &Path) -> Option<(u64, u64)> {
    disks.refresh_specifics(true, DiskRefreshKind::nothing().with_storage());
    let disk = disks
        .iter()
        .filter(|disk| home.starts_with(disk.mount_point()))
        .max_by_key(|disk| disk.mount_point().as_os_str().len())?;
    Some((disk.total_space().saturating_sub(disk.available_space()), disk.total_space()))
}

#[cfg(test)]
mod tests {
    use super::counts;

    #[test]
    fn only_the_interfaces_that_carry_traffic_once_count() {
        for counted in ["en0", "eth0", "enp3s0", "wlan0"] {
            assert!(counts(counted), "{counted}");
        }
        for skipped in ["lo", "lo0", "utun3", "tailscale0", "veth1a2b", "docker0", "br-91ab", "bridge100", "awdl0"] {
            assert!(!counts(skipped), "{skipped}");
        }
    }
}
