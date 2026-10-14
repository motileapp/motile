//! Amounts, sizes and dates in the user's language and region, written by the system's
//! formatters as the Mac app's are: Foundation's. Elsewhere, and should a formatter fail, they are
//! written as in US English.

use chrono::{Local, TimeZone};

/// A size as the Finder says it: "123.5 MB", "1 byte".
pub fn bytes(count: u64) -> String {
    system::bytes(count).unwrap_or_else(|| plain_bytes(count))
}

/// A file's size as the Mac's transcript says it: "Zero KB", "1 byte", "12.3 MB".
pub fn file_bytes(count: u64) -> String {
    system::file_bytes(count).unwrap_or_else(|| if count == 0 { "Zero KB".into() } else { plain_bytes(count) })
}

/// The time of day, with the date before it when that isn't today: "2:03 PM",
/// "Oct 14, 2026 at 2:03 PM".
pub fn stamp(seconds: f64) -> String {
    let Some(time) = Local.timestamp_opt(seconds as i64, 0).single() else { return String::new() };
    let today = time.date_naive() == Local::now().date_naive();
    system::stamp(seconds, !today)
        .unwrap_or_else(|| time.format(if today { "%-I:%M %p" } else { "%b %-d, %Y at %-I:%M %p" }).to_string())
}

/// Dollars and cents; less than a cent is said so.
pub fn usd(value: f64) -> String {
    if value > 0. && value < 0.01 {
        return format!("<{}", usd(0.01));
    }
    system::usd(value).unwrap_or_else(|| plain_usd(value))
}

/// A share as a percentage with at most one decimal: "42%", "42.5%".
pub fn percent(share: f64) -> String {
    system::percent(share).unwrap_or_else(|| plain_percent(share))
}

/// "820", "1.5K", "45.7M", "123B", "1.2T": at most three significant digits.
pub fn abbreviated(count: u64) -> String {
    const UNITS: [&str; 5] = ["", "K", "M", "B", "T"];
    let mut value = count as f64;
    let mut unit = 0;
    while value >= 999.5 && unit < UNITS.len() - 1 {
        value /= 1000.;
        unit += 1;
    }
    let decimals = if unit == 0 || value >= 100. {
        0
    } else if value >= 10. {
        1
    } else {
        2
    };
    let digits = format!("{value:.decimals$}");
    let digits = if decimals > 0 { digits.trim_end_matches('0').trim_end_matches('.') } else { &digits };
    format!("{digits}{}", UNITS[unit])
}

/// The time, in seconds since 1970, written with the parts of a date `template` names, in the
/// order and words of the user's region: "EEEj" is "Thu 10 AM" or "Do, 10 Uhr". `fallback` is
/// the chrono pattern written where the system can't.
pub fn date(seconds: f64, template: &'static str, fallback: &str) -> String {
    system::date(seconds, template).unwrap_or_else(|| {
        Local
            .timestamp_opt(seconds as i64, 0)
            .single()
            .map(|time| time.format(fallback).to_string())
            .unwrap_or_default()
    })
}

/// The day weeks start on in the user's region, 1 for Sunday to 7 for Saturday.
pub fn first_weekday() -> u32 {
    system::first_weekday().unwrap_or(1)
}

fn plain_bytes(count: u64) -> String {
    if count == 1 {
        return "1 byte".into();
    }
    let value = count as f64;
    let Some((size, unit, decimals)) =
        [(1e12, "TB", 2), (1e9, "GB", 2), (1e6, "MB", 1), (1e3, "KB", 0)].into_iter().find(|(size, ..)| value >= *size)
    else {
        return format!("{count} bytes");
    };
    let text = format!("{:.decimals$}", value / size);
    let text = if text.contains('.') { text.trim_end_matches('0').trim_end_matches('.').to_string() } else { text };
    format!("{text} {unit}")
}

fn plain_usd(value: f64) -> String {
    let cents = (value * 100.).round() as u64;
    let (dollars, cents) = (cents / 100, cents % 100);
    let digits = dollars.to_string();
    let mut grouped = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    format!("${grouped}.{cents:02}")
}

fn plain_percent(share: f64) -> String {
    let tenths = (share * 1000.).round() / 10.;
    if tenths.fract() == 0. { format!("{tenths:.0}%") } else { format!("{tenths:.1}%") }
}

#[cfg(target_os = "macos")]
mod system {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use objc2::rc::Retained;
    use objc2_foundation::{
        NSByteCountFormatter, NSByteCountFormatterCountStyle, NSCalendar, NSDate, NSDateFormatter,
        NSDateFormatterStyle, NSNumber, NSNumberFormatter, NSNumberFormatterStyle, NSString,
    };

    thread_local! {
        static BYTES: Retained<NSByteCountFormatter> = {
            let formatter = NSByteCountFormatter::new();
            formatter.setCountStyle(NSByteCountFormatterCountStyle::File);
            formatter.setAllowsNonnumericFormatting(false);
            formatter
        };
        static FILE_BYTES: Retained<NSByteCountFormatter> = {
            let formatter = NSByteCountFormatter::new();
            formatter.setCountStyle(NSByteCountFormatterCountStyle::File);
            formatter
        };
        static STAMPS: [Retained<NSDateFormatter>; 2] = [NSDateFormatterStyle::NoStyle, NSDateFormatterStyle::MediumStyle]
            .map(|date| {
                let formatter = NSDateFormatter::new();
                formatter.setDateStyle(date);
                formatter.setTimeStyle(NSDateFormatterStyle::ShortStyle);
                formatter
            });
        static USD: Retained<NSNumberFormatter> = {
            let formatter = NSNumberFormatter::new();
            formatter.setNumberStyle(NSNumberFormatterStyle::CurrencyStyle);
            formatter.setCurrencyCode(Some(&NSString::from_str("USD")));
            formatter
        };
        static PERCENT: Retained<NSNumberFormatter> = {
            let formatter = NSNumberFormatter::new();
            formatter.setNumberStyle(NSNumberFormatterStyle::PercentStyle);
            formatter.setMinimumFractionDigits(0);
            formatter.setMaximumFractionDigits(1);
            formatter
        };
        static DATES: RefCell<HashMap<&'static str, Retained<NSDateFormatter>>> = RefCell::default();
    }

    pub fn bytes(count: u64) -> Option<String> {
        Some(BYTES.with(|formatter| formatter.stringFromByteCount(count.min(i64::MAX as u64) as i64)).to_string())
    }

    pub fn file_bytes(count: u64) -> Option<String> {
        Some(FILE_BYTES.with(|formatter| formatter.stringFromByteCount(count.min(i64::MAX as u64) as i64)).to_string())
    }

    pub fn stamp(seconds: f64, with_date: bool) -> Option<String> {
        let date = NSDate::dateWithTimeIntervalSince1970(seconds);
        Some(STAMPS.with(|formatters| formatters[usize::from(with_date)].stringFromDate(&date)).to_string())
    }

    pub fn usd(value: f64) -> Option<String> {
        USD.with(|formatter| formatter.stringFromNumber(&NSNumber::new_f64(value))).map(|text| text.to_string())
    }

    pub fn percent(share: f64) -> Option<String> {
        PERCENT.with(|formatter| formatter.stringFromNumber(&NSNumber::new_f64(share))).map(|text| text.to_string())
    }

    pub fn date(seconds: f64, template: &'static str) -> Option<String> {
        DATES.with(|formatters| {
            let mut formatters = formatters.borrow_mut();
            let formatter = formatters.entry(template).or_insert_with(|| {
                let formatter = NSDateFormatter::new();
                formatter.setLocalizedDateFormatFromTemplate(&NSString::from_str(template));
                formatter
            });
            Some(formatter.stringFromDate(&NSDate::dateWithTimeIntervalSince1970(seconds)).to_string())
        })
    }

    pub fn first_weekday() -> Option<u32> {
        u32::try_from(NSCalendar::currentCalendar().firstWeekday()).ok().filter(|day| (1..=7).contains(day))
    }
}

#[cfg(not(target_os = "macos"))]
mod system {
    pub fn bytes(_: u64) -> Option<String> {
        None
    }

    pub fn file_bytes(_: u64) -> Option<String> {
        None
    }

    pub fn stamp(_: f64, _: bool) -> Option<String> {
        None
    }

    pub fn usd(_: f64) -> Option<String> {
        None
    }

    pub fn percent(_: f64) -> Option<String> {
        None
    }

    pub fn date(_: f64, _: &'static str) -> Option<String> {
        None
    }

    pub fn first_weekday() -> Option<u32> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_amounts_read_as_us_english_does() {
        assert_eq!(plain_usd(1234.5), "$1,234.50");
        assert_eq!(plain_percent(0.42), "42%");
        assert_eq!(plain_percent(0.4251), "42.5%");
        assert_eq!(plain_bytes(1), "1 byte");
        assert_eq!(plain_bytes(5_432), "5 KB");
        assert_eq!(plain_bytes(123_456_789), "123.5 MB");
        assert_eq!(plain_bytes(1_234_567_890), "1.23 GB");
    }

    #[test]
    fn counts_are_abbreviated_to_three_digits() {
        assert_eq!(abbreviated(820), "820");
        assert_eq!(abbreviated(1_500), "1.5K");
        assert_eq!(abbreviated(21_437), "21.4K");
        assert_eq!(abbreviated(999_999), "1M");
        assert_eq!(abbreviated(4_571_000), "4.57M");
        assert_eq!(abbreviated(45_700_000), "45.7M");
        assert_eq!(abbreviated(123_000_000_000), "123B");
        assert_eq!(abbreviated(1_200_000_000_000), "1.2T");
        assert_eq!(abbreviated(1_200_000_000_000_000), "1200T");
    }

    #[test]
    fn the_system_writes_every_amount() {
        assert!(!bytes(123_456_789).is_empty());
        assert!(!file_bytes(0).is_empty());
        assert!(!stamp(1_760_000_000.).is_empty());
        assert!(!usd(1234.5).is_empty());
        assert!(usd(0.004).starts_with('<'));
        assert!(!percent(0.42).is_empty());
        assert!(!date(1_760_000_000., "EEEj", "%a %-I %p").is_empty());
        assert!((1..=7).contains(&first_weekday()));
    }
}
