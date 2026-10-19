//! Numbers and dates as text, as a JavaScript runtime writes them in en-US: `Intl.NumberFormat`
//! (grouping, fraction digits, currencies, compact notation), `toLocaleString` with the date styles
//! open-intelligent-ui uses, `toISOString`, and `Date.parse` of ISO strings. The functions use it,
//! and core formats the resolved tree with it.

use crate::js;

pub const DAY_MS: f64 = 86_400_000.0;
/// The largest time value a `Date` holds, either side of 1970.
pub const MAX_TIME: f64 = 8.64e15;

const NBSP: char = '\u{a0}';

/// What `Intl.NumberFormat("en-US", options)` is given: the style, the notation and the
/// fraction digits.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NumberFormat {
    /// `style: "currency"` with this code.
    pub currency: Option<String>,
    /// `notation: "compact"`: 1.2K, $3.4M.
    pub compact: bool,
    pub min_fraction_digits: u32,
    pub max_fraction_digits: u32,
}

impl NumberFormat {
    pub fn decimal(min_fraction_digits: u32, max_fraction_digits: u32) -> NumberFormat {
        NumberFormat { currency: None, compact: false, min_fraction_digits, max_fraction_digits }
    }

    pub fn currency(code: impl Into<String>, min_fraction_digits: u32, max_fraction_digits: u32) -> NumberFormat {
        NumberFormat { currency: Some(code.into()), compact: false, min_fraction_digits, max_fraction_digits }
    }

    pub fn compact(mut self) -> NumberFormat {
        self.compact = true;
        self
    }

    pub fn format(&self, value: f64) -> String {
        let sign = if value.is_sign_negative() && !value.is_nan() { "-" } else { "" };
        let body = if value.is_nan() {
            "NaN".to_owned()
        } else if value.is_infinite() {
            "∞".to_owned()
        } else {
            self.digits(value.abs())
        };
        let Some(code) = &self.currency else { return format!("{sign}{body}") };
        let symbol = currency_symbol(code);
        if symbol.chars().last().is_some_and(char::is_alphabetic) {
            return format!("{sign}{symbol}{NBSP}{body}");
        }
        format!("{sign}{symbol}{body}")
    }

    fn digits(&self, value: f64) -> String {
        let max = self.max_fraction_digits.max(self.min_fraction_digits) as i32;
        let mut decimal = Decimal::from_f64(value);
        let mut suffix = "";
        if self.compact && !decimal.is_zero() {
            let magnitude = decimal.point - 1;
            let mut multiplier = compact_multiplier(magnitude);
            decimal.point += multiplier;
            decimal.round(max);
            if !decimal.is_zero() && decimal.point - 1 != magnitude + multiplier {
                let next = compact_multiplier(magnitude + 1);
                if next != multiplier {
                    decimal.point += next - multiplier;
                    decimal.round(max);
                    multiplier = next;
                }
            }
            suffix = compact_suffix(multiplier);
        } else {
            decimal.round(max);
        }
        let grouping = if self.compact { 2 } else { 1 };
        decimal.render(self.min_fraction_digits as usize, grouping) + suffix
    }
}

/// `Intl`'s compact power for a magnitude: thousands, millions, billions, trillions.
fn compact_multiplier(magnitude: i32) -> i32 {
    match magnitude {
        ..3 => 0,
        3..6 => -3,
        6..9 => -6,
        9..12 => -9,
        _ => -12,
    }
}

fn compact_suffix(multiplier: i32) -> &'static str {
    match multiplier {
        -3 => "K",
        -6 => "M",
        -9 => "B",
        -12 => "T",
        _ => "",
    }
}

/// The symbol en-US writes for a currency; a code without one is written as itself.
pub fn currency_symbol(code: &str) -> &str {
    match code {
        "USD" => "$",
        "EUR" => "€",
        "GBP" => "£",
        "JPY" => "¥",
        "INR" => "₹",
        "KRW" => "₩",
        "ILS" => "₪",
        "VND" => "₫",
        "PHP" => "₱",
        "AUD" => "A$",
        "BRL" => "R$",
        "CAD" => "CA$",
        "CNY" => "CN¥",
        "HKD" => "HK$",
        "MXN" => "MX$",
        "NZD" => "NZ$",
        "TWD" => "NT$",
        "XCD" => "EC$",
        "XAF" => "FCFA",
        "XOF" => "F\u{202f}CFA",
        "XPF" => "CFPF",
        code => code,
    }
}

/// A non-negative number's shortest round-trip digits, as `Intl` reads a double: the value is
/// `0.d₁d₂… × 10^point`.
#[derive(Debug, Clone, PartialEq)]
struct Decimal {
    digits: Vec<u8>,
    point: i32,
}

impl Decimal {
    fn from_f64(value: f64) -> Decimal {
        let text = js::number_to_string(value);
        let (mantissa, exponent) = match text.split_once('e') {
            Some((mantissa, exponent)) => (mantissa, exponent.parse::<i32>().unwrap_or(0)),
            None => (text.as_str(), 0),
        };
        let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
        let mut digits: Vec<u8> = whole.bytes().chain(fraction.bytes()).map(|b| b - b'0').collect();
        let mut point = whole.len() as i32 + exponent;
        let leading = digits.iter().take_while(|&&digit| digit == 0).count();
        digits.drain(..leading);
        point -= leading as i32;
        while digits.last() == Some(&0) {
            digits.pop();
        }
        if digits.is_empty() {
            point = 0;
        }
        Decimal { digits, point }
    }

    fn is_zero(&self) -> bool {
        self.digits.is_empty()
    }

    /// Keeps `fraction` digits after the point, a half rounding away from zero.
    fn round(&mut self, fraction: i32) {
        let keep = self.point + fraction;
        if keep >= self.digits.len() as i32 {
            return;
        }
        if keep < 0 {
            self.digits.clear();
            self.point = 0;
            return;
        }
        let keep = keep as usize;
        let up = self.digits[keep] >= 5;
        self.digits.truncate(keep);
        if up {
            let mut index = keep;
            loop {
                if index == 0 {
                    self.digits.insert(0, 1);
                    self.point += 1;
                    break;
                }
                index -= 1;
                if self.digits[index] < 9 {
                    self.digits[index] += 1;
                    break;
                }
                self.digits[index] = 0;
            }
        }
        while self.digits.last() == Some(&0) {
            self.digits.pop();
        }
        if self.digits.is_empty() {
            self.point = 0;
        }
    }

    /// The digits with a point and groups of three; `min_grouping` is how many digits the
    /// highest group needs before a comma is written.
    fn render(&self, min_fraction: usize, min_grouping: usize) -> String {
        let digit = |index: i32| -> char {
            let Ok(index) = usize::try_from(index) else { return '0' };
            self.digits.get(index).map_or('0', |digit| (b'0' + digit) as char)
        };
        let whole: String =
            if self.point <= 0 { "0".to_owned() } else { (0..self.point).map(digit).collect::<String>() };
        let mut out = String::new();
        let grouped = whole.len() >= 3 + min_grouping;
        for (index, c) in whole.chars().enumerate() {
            if grouped && index > 0 && (whole.len() - index).is_multiple_of(3) {
                out.push(',');
            }
            out.push(c);
        }
        let shown = (self.digits.len() as i32 - self.point).max(0) as usize;
        let fraction: String = (0..shown.max(min_fraction)).map(|index| digit(self.point + index as i32)).collect();
        if !fraction.is_empty() {
            out.push('.');
            out.push_str(&fraction);
        }
        out
    }
}

/// A time value's calendar fields, read in UTC: shift the value by an offset first to read
/// local time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateTime {
    pub year: i64,
    /// 1 to 12.
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    pub millisecond: u32,
    /// 0 for Sunday.
    pub weekday: u32,
}

impl DateTime {
    pub fn from_ms(ms: f64) -> DateTime {
        let days = (ms / DAY_MS).floor();
        let within = (ms - days * DAY_MS) as i64;
        let (year, month, day) = civil_from_days(days as i64);
        DateTime {
            year,
            month,
            day,
            hour: (within / 3_600_000) as u32,
            minute: (within / 60_000 % 60) as u32,
            second: (within / 1000 % 60) as u32,
            millisecond: (within % 1000) as u32,
            weekday: (days as i64 + 4).rem_euclid(7) as u32,
        }
    }
}

/// Days since 1970-01-01 of a proleptic Gregorian date.
pub fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let of_era = year.rem_euclid(400);
    let month = month as i64;
    let of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day as i64 - 1;
    let of_era_days = of_era * 365 + of_era / 4 - of_era / 100 + of_year;
    era * 146_097 + of_era_days - 719_468
}

/// The year, month (1 to 12) and day of a count of days since 1970-01-01.
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let of_era = days.rem_euclid(146_097);
    let year_of_era = (of_era - of_era / 1460 + of_era / 36_524 - of_era / 146_096) / 365;
    let of_year = of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted = (5 * of_year + 2) / 153;
    let day = (of_year - (153 * shifted + 2) / 5 + 1) as u32;
    let month = if shifted < 10 { shifted + 3 } else { shifted - 9 } as u32;
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

pub fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// `Date.UTC(year, month, day)` with a 0-based month that may run over into other years, and a
/// year from 0 to 99 read as 1900 to 1999, as JavaScript does. In days, not milliseconds.
pub fn date_utc_days(year: f64, month: f64, day: f64) -> f64 {
    let year = if (0.0..=99.0).contains(&year.trunc()) { 1900.0 + year.trunc() } else { year.trunc() };
    let year = year + (month / 12.0).floor();
    let month = month.rem_euclid(12.0);
    days_from_civil(year as i64, month as u32 + 1, 1) as f64 + day.trunc() - 1.0
}

/// The `toLocaleString("en-US", …)` styles `@FormatDate` offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateStyle {
    /// Oct 9, 2026
    Date,
    /// Oct 9
    Short,
    /// Friday, October 9, 2026
    Long,
    /// Fri
    Weekday,
    /// Oct 2026
    Month,
    /// 9:30 PM
    Time,
    /// Oct 9, 9:30 PM
    DateTime,
}

impl DateStyle {
    pub fn from_name(name: &str) -> Option<DateStyle> {
        Some(match name {
            "date" => DateStyle::Date,
            "short" => DateStyle::Short,
            "long" => DateStyle::Long,
            "weekday" => DateStyle::Weekday,
            "month" => DateStyle::Month,
            "time" => DateStyle::Time,
            "datetime" => DateStyle::DateTime,
            _ => return None,
        })
    }
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const WEEKDAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

pub fn format_date(at: &DateTime, style: DateStyle) -> String {
    let month = MONTHS[at.month as usize - 1];
    let short_month = &month[..3];
    let weekday = WEEKDAYS[at.weekday as usize];
    let year = if at.year <= 0 { 1 - at.year } else { at.year };
    let time = || {
        let hour = match at.hour % 12 {
            0 => 12,
            hour => hour,
        };
        let half = if at.hour < 12 { "AM" } else { "PM" };
        format!("{hour}:{:02} {half}", at.minute)
    };
    match style {
        DateStyle::Date => format!("{short_month} {}, {year}", at.day),
        DateStyle::Short => format!("{short_month} {}", at.day),
        DateStyle::Long => format!("{weekday}, {month} {}, {year}", at.day),
        DateStyle::Weekday => weekday[..3].to_owned(),
        DateStyle::Month => format!("{short_month} {year}"),
        DateStyle::Time => time(),
        DateStyle::DateTime => format!("{short_month} {}, {}", at.day, time()),
    }
}

/// `Date.prototype.toISOString`; None outside the range a `Date` holds.
pub fn iso_string(ms: f64) -> Option<String> {
    if !ms.is_finite() || ms.abs() > MAX_TIME {
        return None;
    }
    let at = DateTime::from_ms(ms);
    let year = match at.year {
        0..=9999 => format!("{:04}", at.year),
        year if year < 0 => format!("-{:06}", -year),
        year => format!("+{year:06}"),
    };
    Some(format!(
        "{year}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        at.month, at.day, at.hour, at.minute, at.second, at.millisecond
    ))
}

/// `Date.parse` of an ISO date or date-time (ECMA-262's Date Time String Format, with a space
/// allowed for the `T`). A date alone is UTC; a date-time without an offset is local time,
/// `offset` giving the minutes local time is ahead of UTC at a UTC instant.
pub fn parse_iso(text: &str, offset: impl Fn(f64) -> f64) -> Option<f64> {
    let mut cursor = Cursor { text: text.as_bytes(), at: 0 };
    let year = match cursor.peek() {
        Some(sign @ (b'+' | b'-')) => {
            cursor.at += 1;
            let year = cursor.number(6)?;
            if sign == b'-' && year == 0 {
                return None;
            }
            if sign == b'-' { -year } else { year }
        }
        _ => cursor.number(4)?,
    };
    let mut month = 1;
    let mut day = 1;
    if cursor.eat(b'-') {
        month = cursor.number(2)?;
        if cursor.eat(b'-') {
            day = cursor.number(2)?;
        }
    }
    if !(1..=12).contains(&month) || day < 1 || day > i64::from(days_in_month(year, month as u32)) {
        return None;
    }
    let date = days_from_civil(year, month as u32, day as u32) as f64 * DAY_MS;
    if cursor.done() {
        return within_range(date);
    }
    if !(cursor.eat(b'T') || cursor.eat(b't') || cursor.eat(b' ')) {
        return None;
    }
    let hour = cursor.number(2)?;
    if !cursor.eat(b':') {
        return None;
    }
    let minute = cursor.number(2)?;
    let mut second = 0;
    let mut millisecond = 0.0;
    if cursor.eat(b':') {
        second = cursor.number(2)?;
        if cursor.eat(b'.') || cursor.eat(b',') {
            let start = cursor.at;
            while cursor.peek().is_some_and(|b| b.is_ascii_digit()) {
                cursor.at += 1;
            }
            let digits = &text[start..cursor.at];
            if digits.is_empty() {
                return None;
            }
            millisecond = format!("0.{digits}").parse::<f64>().ok().map(|fraction| (fraction * 1000.0).floor())?;
        }
    }
    let midnight = hour == 24 && minute == 0 && second == 0 && millisecond == 0.0;
    if (hour > 23 && !midnight) || minute > 59 || second > 59 {
        return None;
    }
    let time = date + ((hour * 60 + minute) * 60 + second) as f64 * 1000.0 + millisecond;
    let zone = match cursor.peek() {
        None => None,
        Some(b'Z' | b'z') => {
            cursor.at += 1;
            Some(0.0)
        }
        Some(sign @ (b'+' | b'-')) => {
            cursor.at += 1;
            let hours = cursor.number(2)?;
            cursor.eat(b':');
            let minutes = cursor.number(2)?;
            if hours > 23 || minutes > 59 {
                return None;
            }
            let minutes = (hours * 60 + minutes) as f64;
            Some(if sign == b'+' { minutes } else { -minutes })
        }
        Some(_) => return None,
    };
    if !cursor.done() {
        return None;
    }
    let utc = match zone {
        Some(minutes) => time - minutes * 60_000.0,
        None => {
            let guess = time - offset(time) * 60_000.0;
            time - offset(guess) * 60_000.0
        }
    };
    within_range(utc)
}

fn within_range(ms: f64) -> Option<f64> {
    (ms.abs() <= MAX_TIME).then_some(ms)
}

struct Cursor<'a> {
    text: &'a [u8],
    at: usize,
}

impl Cursor<'_> {
    fn peek(&self) -> Option<u8> {
        self.text.get(self.at).copied()
    }

    fn eat(&mut self, byte: u8) -> bool {
        if self.peek() != Some(byte) {
            return false;
        }
        self.at += 1;
        true
    }

    fn done(&self) -> bool {
        self.at == self.text.len()
    }

    /// Exactly `width` digits.
    fn number(&mut self, width: usize) -> Option<i64> {
        let digits = self.text.get(self.at..self.at + width)?;
        if !digits.iter().all(u8::is_ascii_digit) {
            return None;
        }
        self.at += width;
        std::str::from_utf8(digits).ok()?.parse().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_numbers_as_intl() {
        let cases = [
            (NumberFormat::decimal(0, 2), 12345.678, "12,345.68"),
            (NumberFormat::decimal(0, 2), 12345.6, "12,345.6"),
            (NumberFormat::decimal(0, 0), 2.5, "3"),
            (NumberFormat::decimal(0, 2), 1.005, "1.01"),
            (NumberFormat::decimal(0, 1), 600.0, "600"),
            (NumberFormat::decimal(0, 2), 0.004, "0"),
            (NumberFormat::decimal(0, 2), -0.004, "-0"),
            (NumberFormat::decimal(0, 2), 999.999, "1,000"),
            (NumberFormat::decimal(0, 2), 1e21, "1,000,000,000,000,000,000,000"),
            (NumberFormat::decimal(0, 3), 1.5e-7, "0"),
            (NumberFormat::decimal(2, 2), 0.1, "0.10"),
            (NumberFormat::decimal(0, 2), f64::INFINITY, "∞"),
            (NumberFormat::currency("USD", 2, 2), 1234.5, "$1,234.50"),
            (NumberFormat::currency("EUR", 0, 0), 1234.5, "€1,235"),
            (NumberFormat::currency("USD", 2, 2), -5.0, "-$5.00"),
            (NumberFormat::currency("CHF", 2, 2), 1.0, "CHF\u{a0}1.00"),
            (NumberFormat::currency("JPY", 2, 2), 1.0, "¥1.00"),
            (NumberFormat::decimal(0, 1).compact(), 1234567.0, "1.2M"),
            (NumberFormat::decimal(0, 1).compact(), 999.0, "999"),
            (NumberFormat::decimal(0, 1).compact(), 1000.0, "1K"),
            (NumberFormat::decimal(0, 1).compact(), 999_999.0, "1M"),
            (NumberFormat::decimal(0, 1).compact(), 999.96, "1K"),
            (NumberFormat::decimal(0, 1).compact(), -1500.0, "-1.5K"),
            (NumberFormat::decimal(0, 1).compact(), 2.5e9, "2.5B"),
            (NumberFormat::decimal(0, 1).compact(), 1.5e15, "1500T"),
            (NumberFormat::decimal(0, 1).compact(), 1.5e16, "15,000T"),
            (NumberFormat::currency("USD", 0, 1).compact(), 1234567.0, "$1.2M"),
            (NumberFormat::currency("USD", 0, 1).compact(), -1234.0, "-$1.2K"),
        ];
        for (format, value, expected) in cases {
            assert_eq!(format.format(value), expected, "{value} with {format:?}");
        }
    }

    #[test]
    fn walks_the_calendar() {
        for days in [-719_468, -1, 0, 1, 365, 20_735, 2_932_896] {
            let (year, month, day) = civil_from_days(days);
            assert_eq!(days_from_civil(year, month, day), days);
        }
        assert_eq!(civil_from_days(20_735), (2026, 10, 9));
        assert_eq!(date_utc_days(2026.0, 13.0, 1.0), days_from_civil(2027, 2, 1) as f64);
        assert_eq!(date_utc_days(2026.0, 2.0, 0.0), days_from_civil(2026, 2, 28) as f64);
        assert_eq!(date_utc_days(50.0, 0.0, 1.0), days_from_civil(1950, 1, 1) as f64);
        assert_eq!(DateTime::from_ms(-1.0).weekday, 3);
    }

    #[test]
    fn formats_dates_as_en_us() {
        let at = DateTime::from_ms(parse_iso("2026-10-09T21:30:00Z", |_| 0.0).unwrap());
        let cases = [
            (DateStyle::Date, "Oct 9, 2026"),
            (DateStyle::Short, "Oct 9"),
            (DateStyle::Long, "Friday, October 9, 2026"),
            (DateStyle::Weekday, "Fri"),
            (DateStyle::Month, "Oct 2026"),
            (DateStyle::Time, "9:30 PM"),
            (DateStyle::DateTime, "Oct 9, 9:30 PM"),
        ];
        for (style, expected) in cases {
            assert_eq!(format_date(&at, style), expected);
        }
        let midnight = DateTime::from_ms(0.0);
        assert_eq!(format_date(&midnight, DateStyle::Time), "12:00 AM");
    }

    #[test]
    fn writes_and_reads_iso_strings() {
        assert_eq!(iso_string(0.0).as_deref(), Some("1970-01-01T00:00:00.000Z"));
        assert_eq!(iso_string(-1.0).as_deref(), Some("1969-12-31T23:59:59.999Z"));
        assert_eq!(iso_string(MAX_TIME).as_deref(), Some("+275760-09-13T00:00:00.000Z"));
        assert_eq!(iso_string(-62_198_755_200_000.0).as_deref(), Some("-000001-01-01T00:00:00.000Z"));
        assert_eq!(iso_string(MAX_TIME + 1.0), None);

        let utc = |_| 0.0;
        assert_eq!(parse_iso("2026-10-09", utc), Some(20_735.0 * DAY_MS));
        assert_eq!(parse_iso("2026-10-09T21:30:00Z", utc), Some(20_735.0 * DAY_MS + 77_400_000.0));
        assert_eq!(parse_iso("2026-10-09T23:30:00.5+02:00", utc), Some(20_735.0 * DAY_MS + 77_400_500.0));
        assert_eq!(parse_iso("2026-10-09T24:00", utc), Some(20_736.0 * DAY_MS));
        assert_eq!(parse_iso("2026-10", utc), Some(days_from_civil(2026, 10, 1) as f64 * DAY_MS));
        assert_eq!(parse_iso("+002026-10-09", utc), Some(20_735.0 * DAY_MS));
        let new_york = |_| -240.0;
        assert_eq!(parse_iso("2026-10-09 20:00", new_york), Some(20_736.0 * DAY_MS));
        assert_eq!(parse_iso("2026-10-09", new_york), Some(20_735.0 * DAY_MS));
        for text in ["", "soon", "2026-02-30", "2026-13-01", "2026-10-09T25:00", "-000000-01-01", "2026-10-09Tx"] {
            assert_eq!(parse_iso(text, utc), None, "{text}");
        }
    }
}
