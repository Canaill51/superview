//! Stand-in for `chrono` 0.4, limited to what `i-slint-core` calls (its `date_time.rs`, used by
//! Slint's date picker), on jiff (DEC-74). Same results as chrono 0.4.45 for every year from -9999
//! to 9999; jiff refuses years beyond, which chrono accepted. A Slint update that calls more of
//! chrono fails to compile here, never silently.
use core::fmt;
use jiff::civil::Date;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NaiveDate(Date);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimeDelta(i64);

impl TimeDelta {
    pub fn num_days(&self) -> i64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Weekday(jiff::civil::Weekday);

impl Weekday {
    pub fn number_from_monday(&self) -> u32 {
        self.0.to_monday_one_offset() as u32
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParseError;

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("input does not match the date format")
    }
}

impl std::error::Error for ParseError {}

impl NaiveDate {
    pub fn from_ymd_opt(year: i32, month: u32, day: u32) -> Option<Self> {
        let year = i16::try_from(year).ok()?;
        let month = i8::try_from(month).ok()?;
        let day = i8::try_from(day).ok()?;
        Date::new(year, month, day).ok().map(NaiveDate)
    }

    pub fn signed_duration_since(self, rhs: NaiveDate) -> TimeDelta {
        let days = self
            .0
            .since((jiff::Unit::Day, rhs.0))
            .map(|span| i64::from(span.get_days()))
            .unwrap_or(0);
        TimeDelta(days)
    }

    pub fn format<'a>(&self, format: &'a str) -> DelayedFormat<'a> {
        DelayedFormat {
            date: self.0,
            format,
        }
    }

    pub fn parse_from_str(input: &str, format: &str) -> Result<Self, ParseError> {
        Date::strptime(format, input)
            .map(NaiveDate)
            .map_err(|_| ParseError)
    }
}

/// What `NaiveDate::format` returns; like chrono's, `Display` fails on an invalid format.
pub struct DelayedFormat<'a> {
    date: Date,
    format: &'a str,
}

impl fmt::Display for DelayedFormat<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let format = chrono_locale_date(self.format)?;
        let text = jiff::fmt::strtime::format(&format, self.date).map_err(|_| fmt::Error)?;
        f.write_str(&text)
    }
}

/// chrono writes `%x` as `%m/%d/%y` and refuses a flag on it (`%-x`); jiff writes it differently,
/// so it is spelled out here, and a flagged `%x` fails as it does in chrono.
fn chrono_locale_date(format: &str) -> Result<String, fmt::Error> {
    let mut out = String::with_capacity(format.len());
    let mut chars = format.chars().peekable();
    while let Some(c) = chars.next() {
        out.push(c);
        if c != '%' {
            continue;
        }
        let mut flagged = false;
        while let Some(&flag) = chars.peek().filter(|f| "-_0^#".contains(**f)) {
            out.push(flag);
            chars.next();
            flagged = true;
        }
        match chars.next() {
            Some('x') if flagged => return Err(fmt::Error),
            Some('x') => {
                out.pop();
                out.push_str("%m/%d/%y");
            }
            Some(next) => out.push(next),
            None => {}
        }
    }
    Ok(out)
}

pub trait Datelike {
    fn year(&self) -> i32;
    fn month(&self) -> u32;
    fn day(&self) -> u32;
    fn weekday(&self) -> Weekday;
}

impl Datelike for NaiveDate {
    fn year(&self) -> i32 {
        i32::from(self.0.year())
    }
    fn month(&self) -> u32 {
        self.0.month() as u32
    }
    fn day(&self) -> u32 {
        self.0.day() as u32
    }
    fn weekday(&self) -> Weekday {
        Weekday(self.0.weekday())
    }
}

/// The local time zone; only `Local::now().date_naive()` is offered.
pub struct Local;

pub struct LocalNow(jiff::ZonedDateTime);

impl Local {
    pub fn now() -> LocalNow {
        LocalNow(jiff::ZonedDateTime::now())
    }
}

impl LocalNow {
    pub fn date_naive(&self) -> NaiveDate {
        NaiveDate(self.0.date())
    }
}

#[cfg(test)]
mod tests {
    //! The bodies below are i-slint-core 1.18.1's `date_time.rs`; the expected values were read
    //! from chrono 0.4.45 running those same bodies.
    use super::*;

    fn month_day_count(month: u32, year: i32) -> Option<i32> {
        Some(
            NaiveDate::from_ymd_opt(
                match month {
                    12 => year + 1,
                    _ => year,
                },
                match month {
                    12 => 1,
                    _ => month + 1,
                },
                1,
            )?
            .signed_duration_since(NaiveDate::from_ymd_opt(year, month, 1)?)
            .num_days() as i32,
        )
    }

    fn month_offset(month: u32, year: i32) -> i32 {
        if let Some(date) = NaiveDate::from_ymd_opt(year, month, 1) {
            let offset = date.weekday().number_from_monday() as i32;
            if offset >= 7 {
                return 0;
            }
            return offset;
        }
        0
    }

    /// `None` for an invalid date, `Some(Err)` when chrono's `Display` would fail.
    fn format_date(
        format: &str,
        day: u32,
        month: u32,
        year: i32,
    ) -> Option<Result<String, fmt::Error>> {
        use fmt::Write;
        let date = NaiveDate::from_ymd_opt(year, month, day)?;
        let mut text = String::new();
        Some(write!(text, "{}", date.format(format)).map(|()| text))
    }

    fn parse_date(date: &str, format: &str) -> Option<[i32; 3]> {
        NaiveDate::parse_from_str(date, format)
            .ok()
            .map(|date| [date.day() as i32, date.month() as i32, date.year()])
    }

    #[test]
    fn month_lengths_match_chrono() {
        for (month, year, days) in [
            (2, 2024, Some(29)),
            (2, 2023, Some(28)),
            (2, 1900, Some(28)),
            (2, 2000, Some(29)),
            (12, 2026, Some(31)),
            (4, 2026, Some(30)),
            (0, 2026, None),
            (13, 2026, None),
        ] {
            assert_eq!(month_day_count(month, year), days, "{month}/{year}");
        }
    }

    #[test]
    fn first_weekday_matches_chrono() {
        // Sunday is 0, Monday 1, Saturday 6.
        for (month, year, offset) in [
            (11, 2026, 0),
            (12, 2026, 2),
            (6, 2026, 1),
            (8, 2026, 6),
            (0, 2026, 0),
        ] {
            assert_eq!(month_offset(month, year), offset, "{month}/{year}");
        }
    }

    #[test]
    fn weekdays_are_numbered_from_monday_as_in_chrono() {
        // 2026-11-16 is a Monday; chrono numbers Monday 1 and Sunday 7. Slint's month_offset folds
        // 7 into 0, so the test above cannot see a Sunday counted as 0: this one does.
        for (day, number) in [(16, 1), (17, 2), (21, 6), (22, 7)] {
            let date = NaiveDate::from_ymd_opt(2026, 11, day).unwrap();
            assert_eq!(date.weekday().number_from_monday(), number, "2026-11-{day}");
        }
    }

    #[test]
    fn date_picker_formats_match_chrono() {
        let ok = |text: &str| Some(Ok(text.to_owned()));
        assert_eq!(format_date("%a, %b %d", 17, 11, 2026), ok("Tue, Nov 17"));
        assert_eq!(format_date("%B %Y", 17, 11, 2026), ok("November 2026"));
        assert_eq!(format_date("%m/%d/%Y", 17, 11, 2026), ok("11/17/2026"));
        assert_eq!(format_date("%-d.%-m.%y", 5, 3, 2009), ok("5.3.09"));
        assert_eq!(format_date("%m/%d/%Y", 31, 2, 2026), None);
    }

    #[test]
    fn locale_date_is_written_as_chrono_writes_it() {
        assert_eq!(
            format_date("%x", 5, 3, 2009),
            Some(Ok("03/05/09".to_owned()))
        );
        assert_eq!(format_date("%%x", 5, 3, 2009), Some(Ok("%x".to_owned())));
        assert_eq!(format_date("%-x", 5, 3, 2009), Some(Err(fmt::Error)));
        assert_eq!(format_date("%Q", 5, 3, 2009), Some(Err(fmt::Error)));
    }

    #[test]
    fn parsing_matches_chrono() {
        for (input, expected) in [
            ("11/17/2026", Some([17, 11, 2026])),
            ("1/2/2026", Some([2, 1, 2026])),
            ("02/29/2024", Some([29, 2, 2024])),
            ("11/17/26", Some([17, 11, 26])),
            (" 11/17/2026", Some([17, 11, 2026])),
            ("02/29/2023", None),
            ("13/01/2026", None),
            ("11/17/2026x", None),
            ("", None),
        ] {
            assert_eq!(parse_date(input, "%m/%d/%Y"), expected, "{input:?}");
        }
    }

    #[test]
    fn today_is_a_real_date() {
        let today = Local::now().date_naive();
        assert!(NaiveDate::from_ymd_opt(today.year(), today.month(), today.day()).is_some());
        assert!(today.year() >= 2026);
    }
}
