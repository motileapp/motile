//! Dates: "YYYY-MM-DD" calendar days, ISO strings and timestamps, read and written as
//! open-intelligent-ui does. A calendar date has no time zone; a timestamp or an ISO date-time is
//! read in the viewer's, which the [`Env`] gives.

use super::{Required, date};
use crate::format::{self, DAY_MS, DateStyle, DateTime};
use crate::js;
use crate::library::{Args, Env, FnError, FunctionDef};
use crate::schema::Schema;
use crate::value::Value;

pub(super) fn functions() -> Vec<FunctionDef> {
    vec![
        FunctionDef::new("Now", "The current date and time as an ISO string", [], now).returns(Schema::String),
        FunctionDef::new("Today", "Today's date in the viewer's time zone, as \"YYYY-MM-DD\"", [], today)
            .returns(Schema::String),
        FunctionDef::new(
            "FormatDate",
            "A date as text: \"date\" Oct 9, 2026 (default), \"short\" Oct 9, \"long\" Friday, October 9, 2026, \
             \"weekday\" Fri, \"month\" Oct 2026, \"time\" 9:30 PM, \"datetime\", \"iso\" 2026-10-09. Dates are \
             \"YYYY-MM-DD\", ISO or a timestamp",
            [
                date().field("date"),
                Schema::enumeration(["date", "short", "long", "weekday", "month", "time", "datetime", "iso"])
                    .field("style")
                    .optional(),
            ],
            format_date,
        )
        .returns(Schema::String),
        FunctionDef::new(
            "AddDays",
            "The date days later (negative: earlier), as \"YYYY-MM-DD\"",
            [date().field("date"), Schema::Number.field("days")],
            add_days,
        )
        .returns(Schema::String),
        FunctionDef::new(
            "AddMonths",
            "The date months later (negative: earlier), kept within the month (Jan 31 + 1 is Feb 28), as \
             \"YYYY-MM-DD\"; 12 months is a year",
            [date().field("date"), Schema::Number.field("months")],
            add_months,
        )
        .returns(Schema::String),
        FunctionDef::new(
            "DaysBetween",
            "Calendar days from one date to another (negative if to is earlier)",
            [date().field("from"), date().field("to")],
            days_between,
        )
        .returns(Schema::Number),
        FunctionDef::new(
            "MonthsBetween",
            "Whole months from one date to another (negative if to is earlier); @Floor(@MonthsBetween(birthday, \
             @Today()) / 12) is an age",
            [date().field("from"), date().field("to")],
            months_between,
        )
        .returns(Schema::Number),
        FunctionDef::new(
            "StartOf",
            "First day of the date's \"week\" (Monday), \"month\" or \"year\", as \"YYYY-MM-DD\"",
            [date().field("date"), Schema::enumeration(["week", "month", "year"]).field("unit")],
            start_of,
        )
        .returns(Schema::String),
    ]
}

/// A date as a calendar day, in days since 1970-01-01, and for a timestamp or a date-time the
/// instant, in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParsedDate {
    pub day: f64,
    pub instant: Option<f64>,
}

/// Reads "YYYY-MM-DD", an ISO date-time or a timestamp.
pub fn parse_date(value: &Value, env: &dyn Env) -> Result<ParsedDate, FnError> {
    let invalid = || FnError::new(format!("invalid date \"{}\"", js::to_string(value)));
    let instant = match value {
        Value::String(text) => {
            if let Some((year, month, day)) = calendar_date(js::trim(text)) {
                let days = format::date_utc_days(year, month - 1.0, day);
                let (_, read_month, read_day) = format::civil_from_days(days as i64);
                if f64::from(read_month) != month || f64::from(read_day) != day {
                    return Err(invalid());
                }
                return Ok(ParsedDate { day: days, instant: None });
            }
            format::parse_iso(js::trim(text), |at| env.utc_offset_minutes(at)).ok_or_else(invalid)?
        }
        Value::Number(ms) if ms.is_finite() && ms.abs() <= format::MAX_TIME => ms.trunc() + 0.0,
        _ => return Err(invalid()),
    };
    let local = instant + env.utc_offset_minutes(instant) * 60_000.0;
    Ok(ParsedDate { day: (local / DAY_MS).floor(), instant: Some(instant) })
}

/// The year, month and day of exactly "YYYY-MM-DD".
fn calendar_date(text: &str) -> Option<(f64, f64, f64)> {
    let bytes = text.as_bytes();
    let shape = bytes.len() == 10 && bytes[4] == b'-' && bytes[7] == b'-';
    let digits = [0..4, 5..7, 8..10]
        .iter()
        .all(|range| bytes.get(range.clone()).is_some_and(|d| d.iter().all(u8::is_ascii_digit)));
    if !shape || !digits {
        return None;
    }
    let number = |range: std::ops::Range<usize>| text[range].parse::<f64>().ok();
    Some((number(0..4)?, number(5..7)?, number(8..10)?))
}

/// "YYYY-MM-DD" of a calendar day.
pub fn iso_day(day: f64) -> Result<String, FnError> {
    let text = format::iso_string(day * DAY_MS).ok_or_else(|| FnError::new("Invalid time value"))?;
    Ok(text.chars().take(10).collect())
}

fn ymd(day: f64) -> (f64, f64, f64) {
    let (year, month, day) = format::civil_from_days(day as i64);
    (year as f64, f64::from(month) - 1.0, f64::from(day))
}

/// Monday of the day's week.
fn monday(day: f64) -> f64 {
    day - (day + 3.0).rem_euclid(7.0)
}

fn now(args: &Args<'_>) -> Result<Value, FnError> {
    let text = format::iso_string(args.env().now()).ok_or_else(|| FnError::new("Invalid time value"))?;
    Ok(Value::String(text))
}

fn today(args: &Args<'_>) -> Result<Value, FnError> {
    let parsed = parse_date(&Value::Number(args.env().now()), args.env())?;
    Ok(Value::String(iso_day(parsed.day)?))
}

/// A calendar date formats in UTC, so its day never shifts with the viewer's zone.
fn format_date(args: &Args<'_>) -> Result<Value, FnError> {
    let parsed = parse_date(args.need("date")?, args.env())?;
    let style = args.str("style").unwrap_or("date");
    if style == "iso" {
        return Ok(Value::String(iso_day(parsed.day)?));
    }
    let style = DateStyle::from_name(style).unwrap_or(DateStyle::Date);
    let at = match parsed.instant {
        Some(instant) => instant + args.env().utc_offset_minutes(instant) * 60_000.0,
        None => parsed.day * DAY_MS,
    };
    Ok(Value::String(format::format_date(&DateTime::from_ms(at), style)))
}

fn add_days(args: &Args<'_>) -> Result<Value, FnError> {
    let parsed = parse_date(args.need("date")?, args.env())?;
    let days = args.need_number("days")?;
    Ok(Value::String(iso_day(parsed.day + days.trunc())?))
}

fn add_months(args: &Args<'_>) -> Result<Value, FnError> {
    let parsed = parse_date(args.need("date")?, args.env())?;
    let months = args.need_number("months")?;
    let (year, month, day) = ymd(parsed.day);
    let target = month + months.trunc();
    let (_, _, last) = ymd(format::date_utc_days(year, target + 1.0, 0.0));
    Ok(Value::String(iso_day(format::date_utc_days(year, target, day.min(last)))?))
}

fn days_between(args: &Args<'_>) -> Result<Value, FnError> {
    let from = parse_date(args.need("from")?, args.env())?;
    let to = parse_date(args.need("to")?, args.env())?;
    Ok(Value::Number(to.day - from.day))
}

fn months_between(args: &Args<'_>) -> Result<Value, FnError> {
    let (from_year, from_month, from_day) = ymd(parse_date(args.need("from")?, args.env())?.day);
    let (to_year, to_month, to_day) = ymd(parse_date(args.need("to")?, args.env())?.day);
    let mut months = (to_year - from_year) * 12.0 + (to_month - from_month);
    if months > 0.0 && to_day < from_day {
        months -= 1.0;
    }
    if months < 0.0 && to_day > from_day {
        months += 1.0;
    }
    Ok(Value::Number(months))
}

fn start_of(args: &Args<'_>) -> Result<Value, FnError> {
    let day = parse_date(args.need("date")?, args.env())?.day;
    let (year, month, _) = ymd(day);
    let start = match args.need_str("unit")? {
        "week" => monday(day),
        "month" => format::date_utc_days(year, month, 1.0),
        "year" => format::date_utc_days(year, 0.0, 1.0),
        _ => return Err(super::expects("unit", "\"week\" | \"month\" | \"year\"")),
    };
    Ok(Value::String(iso_day(start)?))
}

/// The first day of the period a day is in.
pub(super) fn period_start(day: f64, unit: Period) -> f64 {
    match unit {
        Period::Day => day,
        Period::Week => monday(day),
        Period::Month => {
            let (year, month, _) = ymd(day);
            format::date_utc_days(year, month, 1.0)
        }
    }
}

pub(super) fn next_period(start: f64, unit: Period) -> f64 {
    match unit {
        Period::Day => start + 1.0,
        Period::Week => start + 7.0,
        Period::Month => {
            let (year, month, _) = ymd(start);
            format::date_utc_days(year, month + 1.0, 1.0)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Period {
    Day,
    Week,
    Month,
}
