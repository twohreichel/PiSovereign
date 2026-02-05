//! Task priority value object

use serde::{Deserialize, Serialize};
use std::fmt;

/// Task priority level
///
/// Represents the importance/urgency of a task.
/// Maps to iCalendar PRIORITY values:
/// - High: 1-3
/// - Medium: 4-6
/// - Low: 7-9 or 0/unset
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    /// High priority - urgent, needs immediate attention
    High,
    /// Medium priority - important but not urgent
    Medium,
    /// Low priority - can wait, nice to have
    #[default]
    Low,
}

impl Priority {
    /// Convert from iCalendar PRIORITY value (0-9)
    ///
    /// RFC 5545 specifies:
    /// - 0: undefined (treated as Low)
    /// - 1-3: High
    /// - 4-6: Medium
    /// - 7-9: Low
    #[must_use]
    pub const fn from_ical(priority: u8) -> Self {
        match priority {
            1..=3 => Self::High,
            4..=6 => Self::Medium,
            _ => Self::Low,
        }
    }

    /// Convert to iCalendar PRIORITY value
    ///
    /// Returns the middle value of each range for consistency
    #[must_use]
    pub const fn to_ical(self) -> u8 {
        match self {
            Self::High => 1,
            Self::Medium => 5,
            Self::Low => 9,
        }
    }

    /// Get a human-readable label
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::High => "High",
            Self::Medium => "Medium",
            Self::Low => "Low",
        }
    }

    /// Get an emoji representation
    #[must_use]
    pub const fn emoji(&self) -> &'static str {
        match self {
            Self::High => "🔴",
            Self::Medium => "🟡",
            Self::Low => "🟢",
        }
    }

    /// Check if this priority is higher than another
    #[must_use]
    pub const fn is_higher_than(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (Self::High, Self::Medium | Self::Low) | (Self::Medium, Self::Low)
        )
    }

    /// Get all priority levels in descending order (highest first)
    #[must_use]
    pub const fn all() -> [Self; 3] {
        [Self::High, Self::Medium, Self::Low]
    }
}

impl fmt::Display for Priority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.label())
    }
}

impl From<u8> for Priority {
    fn from(value: u8) -> Self {
        Self::from_ical(value)
    }
}

impl From<Priority> for u8 {
    fn from(priority: Priority) -> Self {
        priority.to_ical()
    }
}

impl PartialOrd for Priority {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Priority {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Higher priority = smaller number in iCal = comes first
        other.to_ical().cmp(&self.to_ical())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_ical() {
        assert_eq!(Priority::from_ical(1), Priority::High);
        assert_eq!(Priority::from_ical(2), Priority::High);
        assert_eq!(Priority::from_ical(3), Priority::High);
        assert_eq!(Priority::from_ical(4), Priority::Medium);
        assert_eq!(Priority::from_ical(5), Priority::Medium);
        assert_eq!(Priority::from_ical(6), Priority::Medium);
        assert_eq!(Priority::from_ical(7), Priority::Low);
        assert_eq!(Priority::from_ical(8), Priority::Low);
        assert_eq!(Priority::from_ical(9), Priority::Low);
        assert_eq!(Priority::from_ical(0), Priority::Low);
    }

    #[test]
    fn test_to_ical() {
        assert_eq!(Priority::High.to_ical(), 1);
        assert_eq!(Priority::Medium.to_ical(), 5);
        assert_eq!(Priority::Low.to_ical(), 9);
    }

    #[test]
    fn test_default() {
        assert_eq!(Priority::default(), Priority::Low);
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", Priority::High), "High");
        assert_eq!(format!("{}", Priority::Medium), "Medium");
        assert_eq!(format!("{}", Priority::Low), "Low");
    }

    #[test]
    fn test_emoji() {
        assert_eq!(Priority::High.emoji(), "🔴");
        assert_eq!(Priority::Medium.emoji(), "🟡");
        assert_eq!(Priority::Low.emoji(), "🟢");
    }

    #[test]
    fn test_is_higher_than() {
        assert!(Priority::High.is_higher_than(&Priority::Medium));
        assert!(Priority::High.is_higher_than(&Priority::Low));
        assert!(Priority::Medium.is_higher_than(&Priority::Low));
        assert!(!Priority::Low.is_higher_than(&Priority::Medium));
        assert!(!Priority::Medium.is_higher_than(&Priority::High));
        assert!(!Priority::High.is_higher_than(&Priority::High));
    }

    #[test]
    fn test_ordering() {
        let mut priorities = vec![Priority::Low, Priority::High, Priority::Medium];
        priorities.sort();
        // High comes first because Ord is implemented so higher priority = greater
        // But sort() is ascending, so we need to reverse for "highest first"
        priorities.reverse();
        assert_eq!(
            priorities,
            vec![Priority::High, Priority::Medium, Priority::Low]
        );
    }

    #[test]
    fn test_serialization() {
        let priority = Priority::High;
        let json = serde_json::to_string(&priority).expect("serialize");
        assert_eq!(json, "\"high\"");

        let deserialized: Priority = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(priority, deserialized);
    }

    #[test]
    fn test_all() {
        let all = Priority::all();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0], Priority::High);
        assert_eq!(all[1], Priority::Medium);
        assert_eq!(all[2], Priority::Low);
    }

    #[test]
    fn test_from_u8_trait() {
        let priority: Priority = 2u8.into();
        assert_eq!(priority, Priority::High);
    }

    #[test]
    fn test_into_u8_trait() {
        let value: u8 = Priority::Medium.into();
        assert_eq!(value, 5);
    }
}
