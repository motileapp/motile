//! A server's machine as its dropdown shows it: how long it has been up, and a row for
//! its CPU, memory, traffic and disk, each with the last minutes as bars or how full it is.

use motile_protocol::wire::{MachineStats, StatsSample};
use serde::Serialize;

use crate::limits::Tone;

/// The bars a row has room for: five minutes of samples three seconds apart.
pub const SLOTS: usize = 100;
/// The traffic that fills a bar when nothing went faster, so that an idle line stays low.
const LEAST_TRAFFIC_PEAK: f64 = 1000.0;
const STEP: f64 = 1000.0;
const MEMORY_STEP: f64 = 1024.0;
/// Where CPU, memory and disk turn pending, then warning, as a share of all of it. Traffic has no limit to
/// near, so it stays calm.
const TONES: Switches = Some([0.6, 0.8]);
const CALM: Switches = None;

type Switches = Option<[f64; 2]>;

#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct StatsView {
    /// How long the machine has been up: "2d 10h".
    pub uptime: String,
    pub rows: Vec<StatsRow>,
}

#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct StatsRow {
    pub kind: StatsKind,
    pub label: String,
    /// After the label and a " · ": "2 cores", or the total, "7.7 GB".
    pub detail: Option<String>,
    /// What is used of the total, before the value: "2.2 GB".
    pub used: Option<String>,
    pub value: String,
    /// `SLOTS` bars, the newest last; a row without them shows `fill`.
    pub bars: Vec<Bar>,
    pub fill: Option<Bar>,
}

/// How tall a bar is, or how full a meter, from 0 to 1, and its colour.
#[derive(Serialize, Clone, Copy, PartialEq, Debug)]
pub struct Bar {
    pub height: f32,
    pub tone: Tone,
}

impl Bar {
    fn of(share: f64, switches: Switches) -> Bar {
        let share = share.clamp(0.0, 1.0);
        let tone = match switches {
            Some([_, warning]) if share >= warning => Tone::Warning,
            Some([pending, _]) if share >= pending => Tone::Pending,
            _ => Tone::Success,
        };
        Bar { height: share as f32, tone }
    }
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum StatsKind {
    Cpu,
    Memory,
    Download,
    Upload,
    Disk,
}

/// Adds what a server sent to what it sent before: all it keeps at first, then one sample each.
pub fn merge(kept: &mut MachineStats, sent: MachineStats) {
    let mut samples = std::mem::take(&mut kept.samples);
    samples.extend(sent.samples.iter().copied());
    let first = samples.len().saturating_sub(SLOTS);
    *kept = MachineStats { samples: samples.split_off(first), ..sent };
}

pub fn view(stats: &MachineStats) -> StatsView {
    let latest = stats.samples.last().copied().unwrap_or_default();
    let memory_total = stats.memory_total.max(1) as f64;
    let disk_share = stats.disk_used as f64 / stats.disk_total.max(1) as f64;
    let traffic_peak = |of: fn(&StatsSample) -> u64| {
        stats.samples.iter().map(of).max().unwrap_or_default().max(LEAST_TRAFFIC_PEAK as u64) as f64
    };
    let (received_peak, sent_peak) = (traffic_peak(|sample| sample.received), traffic_peak(|sample| sample.sent));
    let cores = match stats.cores {
        1 => "1 core".to_string(),
        cores => format!("{cores} cores"),
    };
    let rows = vec![
        StatsRow {
            kind: StatsKind::Cpu,
            label: "CPU".into(),
            detail: Some(cores),
            used: None,
            value: percent(f64::from(latest.cpu)),
            bars: bars(&stats.samples, TONES, |sample| f64::from(sample.cpu) / 100.0),
            fill: None,
        },
        StatsRow {
            kind: StatsKind::Memory,
            label: "RAM".into(),
            detail: Some(bytes(stats.memory_total, MEMORY_STEP)),
            used: Some(bytes(latest.memory_used, MEMORY_STEP)),
            value: percent(latest.memory_used as f64 / memory_total * 100.0),
            bars: bars(&stats.samples, TONES, |sample| sample.memory_used as f64 / memory_total),
            fill: None,
        },
        StatsRow {
            kind: StatsKind::Download,
            label: "Download".into(),
            detail: None,
            used: None,
            value: rate(latest.received),
            bars: bars(&stats.samples, CALM, |sample| sample.received as f64 / received_peak),
            fill: None,
        },
        StatsRow {
            kind: StatsKind::Upload,
            label: "Upload".into(),
            detail: None,
            used: None,
            value: rate(latest.sent),
            bars: bars(&stats.samples, CALM, |sample| sample.sent as f64 / sent_peak),
            fill: None,
        },
        StatsRow {
            kind: StatsKind::Disk,
            label: "Disk".into(),
            detail: Some(bytes(stats.disk_total, STEP)),
            used: Some(bytes(stats.disk_used, STEP)),
            value: percent(disk_share * 100.0),
            bars: Vec::new(),
            fill: Some(Bar::of(disk_share, TONES)),
        },
    ];
    StatsView { uptime: uptime(stats.uptime_secs), rows }
}

/// A bar for each slot, the empty ones first while fewer samples have come.
fn bars(samples: &[StatsSample], switches: Switches, share: impl Fn(&StatsSample) -> f64) -> Vec<Bar> {
    let shown = &samples[samples.len().saturating_sub(SLOTS)..];
    let empty = std::iter::repeat_n(Bar::of(0.0, switches), SLOTS - shown.len());
    empty.chain(shown.iter().map(|sample| Bar::of(share(sample), switches))).collect()
}

fn percent(value: f64) -> String {
    if value > 0.0 && value < 10.0 { format!("{value:.1}%") } else { format!("{value:.0}%") }
}

fn rate(bytes_per_second: u64) -> String {
    format!("{}/s", bytes(bytes_per_second, STEP))
}

/// As the Mac counts them: memory in 1024s, "16.0 GB", and disks and traffic in 1000s, "1.0 TB",
/// "1.4 KB".
fn bytes(bytes: u64, step: f64) -> String {
    const UNITS: [&str; 5] = ["KB", "MB", "GB", "TB", "PB"];
    if (bytes as f64) < step {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / step;
    let mut unit = 0;
    // What would round to a whole step is shown in the next unit: "1.0 TB", not "1000 GB".
    while value >= step - 0.5 && unit < UNITS.len() - 1 {
        value /= step;
        unit += 1;
    }
    let unit = UNITS[unit];
    if value >= 100.0 { format!("{value:.0} {unit}") } else { format!("{value:.1} {unit}") }
}

fn uptime(seconds: u64) -> String {
    let (days, hours, minutes) = (seconds / 86_400, seconds % 86_400 / 3600, seconds % 3600 / 60);
    match (days, hours) {
        (0, 0) => format!("{}m", minutes.max(1)),
        (0, hours) => format!("{hours}h {minutes}m"),
        (days, hours) => format!("{days}d {hours}h"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(cpu: f32, received: u64) -> StatsSample {
        StatsSample { at: 0.0, cpu, memory_used: 2_362_232_012, received, sent: 285 }
    }

    fn stats(samples: Vec<StatsSample>) -> MachineStats {
        MachineStats {
            cores: 2,
            memory_total: 8_267_812_454,
            disk_used: 9_663_676_416,
            disk_total: 26_306_674_688,
            uptime_secs: 2 * 86_400 + 10 * 3600 + 120,
            every_secs: 3.0,
            samples,
        }
    }

    #[test]
    fn a_machine_reads_as_its_monitor_would() {
        let view = view(&stats(vec![sample(12.0, 0), sample(0.5, 1434)]));
        assert_eq!(view.uptime, "2d 10h");
        let lines: Vec<_> = view
            .rows
            .iter()
            .map(|row| (row.label.as_str(), row.detail.as_deref(), row.used.as_deref(), row.value.as_str()))
            .collect();
        assert_eq!(
            lines,
            [
                ("CPU", Some("2 cores"), None, "0.5%"),
                ("RAM", Some("7.7 GB"), Some("2.2 GB"), "29%"),
                ("Download", None, None, "1.4 KB/s"),
                ("Upload", None, None, "285 B/s"),
                ("Disk", Some("26.3 GB"), Some("9.7 GB"), "37%"),
            ]
        );
        assert_eq!(view.rows[4].fill, Some(Bar { height: 0.36734694, tone: Tone::Success }));
    }

    #[test]
    fn the_bars_end_with_the_newest_sample_and_traffic_fills_to_its_peak() {
        let view = view(&stats(vec![sample(50.0, 4096), sample(100.0, 2048)]));
        let heights = |row: usize| view.rows[row].bars.iter().map(|bar| bar.height).collect::<Vec<_>>();
        let cpu = heights(0);
        assert_eq!(cpu.len(), SLOTS);
        assert!(cpu[..SLOTS - 2].iter().all(|height| *height == 0.0));
        assert_eq!(&cpu[SLOTS - 2..], [0.5, 1.0]);
        assert_eq!(&heights(2)[SLOTS - 2..], [1.0, 0.5]);
        assert_eq!(heights(3)[SLOTS - 1], 285.0 / 1000.0);
    }

    #[test]
    fn a_part_turns_pending_then_warning_as_it_fills_and_traffic_never_does() {
        let view = view(&stats(vec![sample(59.0, 0), sample(60.0, 0), sample(80.0, 9000)]));
        let tones = |row: usize| view.rows[row].bars[SLOTS - 3..].iter().map(|bar| bar.tone).collect::<Vec<_>>();
        assert_eq!(tones(0), [Tone::Success, Tone::Pending, Tone::Warning]);
        assert_eq!(tones(2), [Tone::Success; 3]);
    }

    #[test]
    fn later_samples_add_to_the_first_and_the_oldest_go() {
        let mut kept = MachineStats::default();
        merge(&mut kept, stats((0..SLOTS).map(|cpu| sample(cpu as f32, 0)).collect()));
        merge(&mut kept, stats(vec![sample(1000.0, 0)]));
        assert_eq!(kept.samples.len(), SLOTS);
        assert_eq!(kept.samples[0].cpu, 1.0);
        assert_eq!(kept.samples[SLOTS - 1].cpu, 1000.0);
    }

    #[test]
    fn an_amount_that_rounds_to_the_next_unit_is_shown_in_it() {
        assert_eq!(bytes(999_653_638_144, STEP), "1.0 TB");
        assert_eq!(bytes(999, STEP), "999 B");
        assert_eq!(bytes(17_179_869_184, MEMORY_STEP), "16.0 GB");
    }

    #[test]
    fn uptime_names_its_two_largest_units() {
        assert_eq!(uptime(30), "1m");
        assert_eq!(uptime(3 * 3600 + 12 * 60), "3h 12m");
        assert_eq!(uptime(86_400 + 59), "1d 0h");
    }
}
