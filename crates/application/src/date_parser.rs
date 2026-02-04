//! Natural language date parsing utilities
//!
//! Provides fuzzy date parsing for German and English natural language dates.

use chrono::{Datelike, Duration, Local, NaiveDate, Weekday};
use tracing::debug;

/// Parse a natural language date string into a NaiveDate
///
/// Supports formats like:
/// - "heute", "today"
/// - "morgen", "tomorrow"
/// - "übermorgen", "day after tomorrow"
/// - "nächsten Montag", "next Monday"
/// - "15. Januar", "January 15"
/// - "15.01.2025", "2025-01-15"
pub fn parse_date(input: &str) -> Option<NaiveDate> {
    let input = input.trim().to_lowercase();
    let today = Local::now().date_naive();

    // Try simple German patterns first
    if let Some(date) = parse_german_relative(&input, today) {
        debug!(input = %input, date = %date, "Parsed German relative date");
        return Some(date);
    }

    // Try simple English patterns
    if let Some(date) = parse_english_relative(&input, today) {
        debug!(input = %input, date = %date, "Parsed English relative date");
        return Some(date);
    }

    // Try weekday patterns
    if let Some(date) = parse_weekday(&input, today) {
        debug!(input = %input, date = %date, "Parsed weekday");
        return Some(date);
    }

    // Try common date formats
    if let Some(date) = parse_date_format(&input, today) {
        debug!(input = %input, date = %date, "Parsed date format");
        return Some(date);
    }

    // Fall back to fuzzydate library
    match fuzzydate::parse(&input) {
        Ok(datetime) => {
            let date = datetime.date();
            debug!(input = %input, date = %date, "Parsed with fuzzydate");
            Some(date)
        }
        Err(_) => {
            debug!(input = %input, "Failed to parse date");
            None
        }
    }
}

/// Parse German relative date expressions
fn parse_german_relative(input: &str, today: NaiveDate) -> Option<NaiveDate> {
    match input {
        "heute" => Some(today),
        "morgen" => Some(today + Duration::days(1)),
        "übermorgen" => Some(today + Duration::days(2)),
        "gestern" => Some(today - Duration::days(1)),
        "vorgestern" => Some(today - Duration::days(2)),
        _ if input.contains("nächste woche") => Some(today + Duration::weeks(1)),
        _ if input.contains("diese woche") => Some(today),
        _ => None,
    }
}

/// Parse English relative date expressions
fn parse_english_relative(input: &str, today: NaiveDate) -> Option<NaiveDate> {
    match input {
        "today" => Some(today),
        "tomorrow" => Some(today + Duration::days(1)),
        "yesterday" => Some(today - Duration::days(1)),
        _ if input.contains("day after tomorrow") => Some(today + Duration::days(2)),
        _ if input.contains("next week") => Some(today + Duration::weeks(1)),
        _ if input.contains("this week") => Some(today),
        _ => None,
    }
}

/// Parse weekday expressions like "nächsten Montag" or "next Monday"
fn parse_weekday(input: &str, today: NaiveDate) -> Option<NaiveDate> {
    // German weekdays
    let weekday = if input.contains("montag") || input.contains("monday") {
        Some(Weekday::Mon)
    } else if input.contains("dienstag") || input.contains("tuesday") {
        Some(Weekday::Tue)
    } else if input.contains("mittwoch") || input.contains("wednesday") {
        Some(Weekday::Wed)
    } else if input.contains("donnerstag") || input.contains("thursday") {
        Some(Weekday::Thu)
    } else if input.contains("freitag") || input.contains("friday") {
        Some(Weekday::Fri)
    } else if input.contains("samstag") || input.contains("saturday") {
        Some(Weekday::Sat)
    } else if input.contains("sonntag") || input.contains("sunday") {
        Some(Weekday::Sun)
    } else {
        None
    }?;

    let is_next = input.contains("nächst")
        || input.contains("next")
        || input.contains("kommend");

    Some(next_weekday(today, weekday, is_next))
}

/// Find the next occurrence of a weekday
fn next_weekday(from: NaiveDate, target: Weekday, force_next: bool) -> NaiveDate {
    let current_weekday = from.weekday();
    let target_num = target.num_days_from_monday();
    let current_num = current_weekday.num_days_from_monday();

    let mut days_until = if target_num > current_num {
        target_num - current_num
    } else if target_num < current_num {
        7 - (current_num - target_num)
    } else if force_next {
        7 // Same day, but force next week
    } else {
        0 // Same day, return today
    };

    // If "next" is specified and the day is still this week, add a week
    if force_next && days_until < 7 && target_num <= current_num {
        days_until += 7;
    }

    from + Duration::days(i64::from(days_until))
}

/// Parse common date formats
fn parse_date_format(input: &str, _today: NaiveDate) -> Option<NaiveDate> {
    // Try ISO format: 2025-01-15
    if let Ok(date) = NaiveDate::parse_from_str(input, "%Y-%m-%d") {
        return Some(date);
    }

    // Try German format: 15.01.2025
    if let Ok(date) = NaiveDate::parse_from_str(input, "%d.%m.%Y") {
        return Some(date);
    }

    // Try short German format: 15.01.
    if let Ok(date) = NaiveDate::parse_from_str(
        &format!("{}{}", input.trim_end_matches('.'), ".2025"),
        "%d.%m.%Y",
    ) {
        return Some(date);
    }

    // Try US format: 01/15/2025
    if let Ok(date) = NaiveDate::parse_from_str(input, "%m/%d/%Y") {
        return Some(date);
    }

    None
}

/// Extract a date from a longer text string
///
/// Useful for parsing dates from command text like "briefing für morgen"
pub fn extract_date_from_text(input: &str) -> Option<NaiveDate> {
    let input = input.to_lowercase();

    // Check for common German patterns in text
    if input.contains("für heute") || input.contains("for today") {
        return parse_date("heute");
    }
    if input.contains("für morgen") || input.contains("for tomorrow") {
        return parse_date("morgen");
    }
    if input.contains("für übermorgen") {
        return parse_date("übermorgen");
    }

    // Try to extract date patterns
    let date_indicators = [
        "für ",
        "am ",
        "on ",
        "for ",
        "ab ",
        "vom ",
        "bis ",
        "until ",
    ];

    for indicator in &date_indicators {
        if let Some(idx) = input.find(indicator) {
            let after = &input[idx + indicator.len()..];
            let date_part = after
                .split_whitespace()
                .take(3) // Take up to 3 words for dates like "nächsten montag"
                .collect::<Vec<_>>()
                .join(" ");

            if let Some(date) = parse_date(&date_part) {
                return Some(date);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_heute() {
        let today = Local::now().date_naive();
        assert_eq!(parse_date("heute"), Some(today));
    }

    #[test]
    fn parse_morgen() {
        let today = Local::now().date_naive();
        assert_eq!(parse_date("morgen"), Some(today + Duration::days(1)));
    }

    #[test]
    fn parse_uebermorgen() {
        let today = Local::now().date_naive();
        assert_eq!(parse_date("übermorgen"), Some(today + Duration::days(2)));
    }

    #[test]
    fn parse_today() {
        let today = Local::now().date_naive();
        assert_eq!(parse_date("today"), Some(today));
    }

    #[test]
    fn parse_tomorrow() {
        let today = Local::now().date_naive();
        assert_eq!(parse_date("tomorrow"), Some(today + Duration::days(1)));
    }

    #[test]
    fn parse_iso_format() {
        let expected = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();
        assert_eq!(parse_date("2025-01-15"), Some(expected));
    }

    #[test]
    fn parse_german_format() {
        let expected = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();
        assert_eq!(parse_date("15.01.2025"), Some(expected));
    }

    #[test]
    fn parse_weekday_montag() {
        let result = parse_date("montag");
        assert!(result.is_some());
        assert_eq!(result.unwrap().weekday(), Weekday::Mon);
    }

    #[test]
    fn parse_naechsten_montag() {
        let today = Local::now().date_naive();
        let result = parse_date("nächsten montag");
        assert!(result.is_some());
        let date = result.unwrap();
        assert_eq!(date.weekday(), Weekday::Mon);
        assert!(date > today || date == today);
    }

    #[test]
    fn extract_date_fuer_morgen() {
        let today = Local::now().date_naive();
        assert_eq!(
            extract_date_from_text("briefing für morgen"),
            Some(today + Duration::days(1))
        );
    }

    #[test]
    fn extract_date_fuer_heute() {
        let today = Local::now().date_naive();
        assert_eq!(
            extract_date_from_text("was steht für heute an"),
            Some(today)
        );
    }

    #[test]
    fn next_weekday_same_day_not_forced() {
        let monday = NaiveDate::from_ymd_opt(2025, 1, 6).unwrap(); // A Monday
        let result = next_weekday(monday, Weekday::Mon, false);
        assert_eq!(result, monday);
    }

    #[test]
    fn next_weekday_same_day_forced() {
        let monday = NaiveDate::from_ymd_opt(2025, 1, 6).unwrap(); // A Monday
        let result = next_weekday(monday, Weekday::Mon, true);
        assert_eq!(result, NaiveDate::from_ymd_opt(2025, 1, 13).unwrap());
    }

    #[test]
    fn parse_invalid_returns_none() {
        assert_eq!(parse_date("gibberish xyz 123"), None);
    }
}
