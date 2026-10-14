//! Amounts, sizes and dates in the user's language and region, written by the system's
//! formatters as the Mac app's are: Foundation's, and ICU's for compact counts, which is what
//! Foundation's own compact style runs on. Elsewhere, and should a formatter fail, they are
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

/// A count in up to three significant figures, its size in a word or a letter: "12.3K", "1M".
pub fn compact(value: u64) -> String {
    system::compact(value).unwrap_or_else(|| plain_compact(value))
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

fn plain_compact(value: u64) -> String {
    let value = value as f64;
    if value < 1e3 {
        return format!("{value}");
    }
    let exponent = value.log10().floor() as i32;
    let rounded = {
        let scale = 10f64.powi(exponent - 2);
        (value / scale).round() * scale
    };
    let units = [(1e12, "T"), (1e9, "B"), (1e6, "M"), (1e3, "K")];
    let Some((size, unit)) = units.into_iter().find(|(size, _)| rounded >= *size) else {
        return format!("{rounded}");
    };
    let shown = rounded / size;
    let text = format!("{shown:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    format!("{text}{unit}")
}

#[cfg(target_os = "macos")]
mod system {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::ffi::{CString, c_char, c_void};

    use objc2::rc::Retained;
    use objc2_foundation::{
        NSByteCountFormatter, NSByteCountFormatterCountStyle, NSCalendar, NSDate, NSDateFormatter,
        NSDateFormatterStyle, NSLocale, NSNumber, NSNumberFormatter, NSNumberFormatterStyle, NSString,
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
        static COMPACT: RefCell<Option<(String, Compact)>> = const { RefCell::new(None) };
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

    pub fn compact(value: u64) -> Option<String> {
        let locale = NSLocale::currentLocale().localeIdentifier().to_string();
        COMPACT.with(|kept| {
            let mut kept = kept.borrow_mut();
            if kept.as_ref().is_none_or(|(made_for, _)| *made_for != locale) {
                *kept = Some((locale.clone(), Compact::new(&locale)?));
            }
            kept.as_ref()?.1.format(value.min(i64::MAX as u64) as i64)
        })
    }

    /// ICU's number formatter on Foundation's `.compactName` with 1 to 3 significant digits.
    struct Compact {
        formatter: *mut c_void,
        result: *mut c_void,
    }

    #[link(name = "icucore", kind = "dylib")]
    unsafe extern "C" {
        fn unumf_openForSkeletonAndLocale(
            skeleton: *const u16,
            length: i32,
            locale: *const c_char,
            status: *mut i32,
        ) -> *mut c_void;
        fn unumf_openResult(status: *mut i32) -> *mut c_void;
        fn unumf_formatInt(formatter: *const c_void, value: i64, result: *mut c_void, status: *mut i32);
        fn unumf_resultToString(result: *const c_void, buffer: *mut u16, capacity: i32, status: *mut i32) -> i32;
        fn unumf_close(formatter: *mut c_void);
        fn unumf_closeResult(result: *mut c_void);
    }

    /// ICU says an error with a status above zero; below zero is a warning.
    fn failed(status: i32) -> bool {
        status > 0
    }

    impl Compact {
        fn new(locale: &str) -> Option<Self> {
            let skeleton: Vec<u16> = "compact-short @##".encode_utf16().collect();
            let locale = CString::new(locale).ok()?;
            let mut status = 0;
            // SAFETY: the skeleton and the locale outlive the call, which copies them.
            let formatter = unsafe {
                unumf_openForSkeletonAndLocale(skeleton.as_ptr(), skeleton.len() as i32, locale.as_ptr(), &mut status)
            };
            if failed(status) || formatter.is_null() {
                return None;
            }
            // SAFETY: ICU makes an empty result to format into.
            let result = unsafe { unumf_openResult(&mut status) };
            let compact = Self { formatter, result };
            (!failed(status) && !result.is_null()).then_some(compact)
        }

        fn format(&self, value: i64) -> Option<String> {
            let mut status = 0;
            let mut buffer = [0u16; 64];
            // SAFETY: both were opened by ICU and are only used on this thread; the buffer's
            // length is its capacity.
            let length = unsafe {
                unumf_formatInt(self.formatter, value, self.result, &mut status);
                unumf_resultToString(self.result, buffer.as_mut_ptr(), buffer.len() as i32, &mut status)
            };
            if failed(status) {
                return None;
            }
            String::from_utf16(buffer.get(..usize::try_from(length).ok()?)?).ok()
        }
    }

    impl Drop for Compact {
        fn drop(&mut self) {
            // SAFETY: opened by ICU, closed once.
            unsafe {
                if !self.result.is_null() {
                    unumf_closeResult(self.result);
                }
                unumf_close(self.formatter);
            }
        }
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

    pub fn compact(_: u64) -> Option<String> {
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
        assert_eq!(plain_compact(999), "999");
        assert_eq!(plain_compact(12_345), "12.3K");
        assert_eq!(plain_compact(999_999), "1M");
        assert_eq!(plain_compact(1_234_567), "1.23M");
        assert_eq!(plain_bytes(1), "1 byte");
        assert_eq!(plain_bytes(5_432), "5 KB");
        assert_eq!(plain_bytes(123_456_789), "123.5 MB");
        assert_eq!(plain_bytes(1_234_567_890), "1.23 GB");
    }

    #[test]
    fn the_system_writes_every_amount() {
        assert!(!bytes(123_456_789).is_empty());
        assert!(!file_bytes(0).is_empty());
        assert!(!stamp(1_760_000_000.).is_empty());
        assert!(!usd(1234.5).is_empty());
        assert!(usd(0.004).starts_with('<'));
        assert!(!percent(0.42).is_empty());
        assert!(!compact(999_999).is_empty());
        assert!(!date(1_760_000_000., "EEEj", "%a %-I %p").is_empty());
        assert!((1..=7).contains(&first_weekday()));
    }
}
