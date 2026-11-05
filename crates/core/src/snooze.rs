//! When a thread's snooze ends: the times the menus offer, and a day and a time the user writes.

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, NaiveTime, TimeZone};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Serialize, Debug, PartialEq)]
pub struct Preset {
    pub label: &'static str,
    /// When it wakes, in seconds since 1970.
    pub until: f64,
}

/// The times a snooze menu offers, the soonest first, each once: in an hour, in three, this
/// evening while it is still to come, tomorrow morning and next Monday morning.
pub fn presets(now: DateTime<Local>) -> Vec<Preset> {
    let today = now.date_naive();
    let monday = today + Duration::days(i64::from(7 - today.weekday().num_days_from_monday()));
    let candidates = [
        ("In 1 Hour", Some(now + Duration::hours(1))),
        ("In 3 Hours", Some(now + Duration::hours(3))),
        ("This Evening", at(today, 18, 0).filter(|evening| *evening > now)),
        ("Tomorrow", at(today + Duration::days(1), 9, 0)),
        ("Next Week", at(monday, 9, 0)),
    ];
    let mut presets: Vec<Preset> = Vec::new();
    for (label, until) in candidates {
        let Some(until) = until else { continue };
        let until = until.timestamp() as f64;
        if presets.iter().any(|preset| (preset.until / 60.0).floor() == (until / 60.0).floor()) {
            continue;
        }
        presets.push(Preset { label, until });
    }
    presets.sort_by(|one, other| one.until.total_cmp(&other.until));
    presets
}

/// When a snooze the user set ends: a day ("2026-11-06") and a time of it ("9:30", "21:30",
/// "9pm", "9:30 am"), which must be to come.
pub fn custom(day: &str, time: &str, now: DateTime<Local>) -> Result<f64, String> {
    let day = NaiveDate::parse_from_str(day.trim(), "%Y-%m-%d").map_err(|_| "Pick a day.".to_string())?;
    let time = time_of_day(time).ok_or_else(|| "Write a time like 9:30 or 6 pm.".to_string())?;
    let until = Local.from_local_datetime(&day.and_time(time)).earliest().ok_or("That time doesn't exist here.")?;
    if until <= now {
        return Err("Pick a time that is still to come.".to_string());
    }
    Ok(until.timestamp() as f64)
}

fn at(day: NaiveDate, hour: u32, minute: u32) -> Option<DateTime<Local>> {
    Local.from_local_datetime(&day.and_hms_opt(hour, minute, 0)?).earliest()
}

fn time_of_day(text: &str) -> Option<NaiveTime> {
    let text = text.trim().to_lowercase().replace('.', ":");
    let (clock, half) = match text.strip_suffix("am").or_else(|| text.strip_suffix('a')) {
        Some(clock) => (clock.trim(), Some(0)),
        None => match text.strip_suffix("pm").or_else(|| text.strip_suffix('p')) {
            Some(clock) => (clock.trim(), Some(12)),
            None => (text.as_str(), None),
        },
    };
    let (hour, minute) = clock.split_once(':').unwrap_or((clock, "0"));
    let (hour, minute): (u32, u32) = (hour.trim().parse().ok()?, minute.trim().parse().ok()?);
    let hour = match half {
        Some(_) if !(1..=12).contains(&hour) => return None,
        Some(offset) => hour % 12 + offset,
        None => hour,
    };
    NaiveTime::from_hms_opt(hour, minute, 0)
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Ask {
    Presets,
    Custom { day: String, time: String },
}

/// Answers a client's question about a snooze: JSON in, JSON out.
pub fn snooze_json(input: &str) -> String {
    let answer: Value = match serde_json::from_str(input) {
        Ok(Ask::Presets) => json!({ "presets": presets(Local::now()) }),
        Ok(Ask::Custom { day, time }) => match custom(&day, &time, Local::now()) {
            Ok(until) => json!({ "until": until }),
            Err(error) => json!({ "error": error }),
        },
        Err(error) => json!({ "error": error.to_string() }),
    };
    answer.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local(day: &str, time: &str) -> DateTime<Local> {
        let day = NaiveDate::parse_from_str(day, "%Y-%m-%d").unwrap();
        Local.from_local_datetime(&day.and_time(time_of_day(time).unwrap())).earliest().unwrap()
    }

    fn labels(presets: &[Preset]) -> Vec<&str> {
        presets.iter().map(|preset| preset.label).collect()
    }

    #[test]
    fn a_wednesday_afternoon_offers_every_preset_soonest_first() {
        let presets = presets(local("2026-11-04", "14:00"));

        assert_eq!(labels(&presets), ["In 1 Hour", "In 3 Hours", "This Evening", "Tomorrow", "Next Week"]);
        assert_eq!(presets[2].until, local("2026-11-04", "18:00").timestamp() as f64);
        assert_eq!(presets[4].until, local("2026-11-09", "9:00").timestamp() as f64);
    }

    #[test]
    fn the_evening_goes_once_it_has_come_and_a_sunday_has_no_separate_next_week() {
        let presets = presets(local("2026-11-08", "19:00"));

        assert_eq!(labels(&presets), ["In 1 Hour", "In 3 Hours", "Tomorrow"]);
    }

    #[test]
    fn a_custom_time_reads_either_clock_and_must_be_to_come() {
        let now = local("2026-11-04", "14:00");

        assert_eq!(custom("2026-11-05", "9pm", now), Ok(local("2026-11-05", "21:00").timestamp() as f64));
        assert_eq!(custom("2026-11-05", "9:30 AM", now), Ok(local("2026-11-05", "9:30").timestamp() as f64));
        assert_eq!(custom("2026-11-05", "12 am", now), Ok(local("2026-11-05", "0:00").timestamp() as f64));
        assert_eq!(custom("2026-11-04", "13:00", now), Err("Pick a time that is still to come.".to_string()));
        assert_eq!(custom("2026-11-05", "25:00", now), Err("Write a time like 9:30 or 6 pm.".to_string()));
    }
}
