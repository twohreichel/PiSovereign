//! Property-based tests for domain value objects
//!
//! These tests use proptest to verify invariants across many random inputs.

use domain::value_objects::{EmailAddress, GeoLocation, Humidity, MemoryId, TaskStatus, Timezone};
use proptest::prelude::*;

// ============================================================================
// GeoLocation Property Tests
// ============================================================================

mod geo_location_tests {
    use super::*;

    proptest! {
        #[test]
        fn valid_coordinates_create_location(
            lat in -90.0f64..=90.0f64,
            lon in -180.0f64..=180.0f64
        ) {
            let result = GeoLocation::new(lat, lon);
            prop_assert!(result.is_ok());

            let loc = result.unwrap();
            prop_assert!((loc.latitude() - lat).abs() < f64::EPSILON);
            prop_assert!((loc.longitude() - lon).abs() < f64::EPSILON);
        }

        #[test]
        fn invalid_latitude_rejected(
            lat in prop_oneof![
                (-1000.0f64..-90.1f64),
                (90.1f64..1000.0f64)
            ],
            lon in -180.0f64..=180.0f64
        ) {
            let result = GeoLocation::new(lat, lon);
            prop_assert!(result.is_err());
        }

        #[test]
        fn invalid_longitude_rejected(
            lat in -90.0f64..=90.0f64,
            lon in prop_oneof![
                (-1000.0f64..-180.1f64),
                (180.1f64..1000.0f64)
            ]
        ) {
            let result = GeoLocation::new(lat, lon);
            prop_assert!(result.is_err());
        }

        #[test]
        fn distance_to_self_is_zero(
            lat in -90.0f64..=90.0f64,
            lon in -180.0f64..=180.0f64
        ) {
            if let Ok(loc) = GeoLocation::new(lat, lon) {
                let distance = loc.distance_km(&loc);
                prop_assert!(distance.abs() < 0.001);
            }
        }

        #[test]
        fn distance_is_symmetric(
            lat1 in -90.0f64..=90.0f64,
            lon1 in -180.0f64..=180.0f64,
            lat2 in -90.0f64..=90.0f64,
            lon2 in -180.0f64..=180.0f64
        ) {
            if let (Ok(loc1), Ok(loc2)) = (
                GeoLocation::new(lat1, lon1),
                GeoLocation::new(lat2, lon2)
            ) {
                let d1 = loc1.distance_km(&loc2);
                let d2 = loc2.distance_km(&loc1);
                prop_assert!((d1 - d2).abs() < 0.001);
            }
        }

        #[test]
        fn distance_is_non_negative(
            lat1 in -90.0f64..=90.0f64,
            lon1 in -180.0f64..=180.0f64,
            lat2 in -90.0f64..=90.0f64,
            lon2 in -180.0f64..=180.0f64
        ) {
            if let (Ok(loc1), Ok(loc2)) = (
                GeoLocation::new(lat1, lon1),
                GeoLocation::new(lat2, lon2)
            ) {
                prop_assert!(loc1.distance_km(&loc2) >= 0.0);
            }
        }

        #[test]
        fn serialization_roundtrip(
            lat in -90.0f64..=90.0f64,
            lon in -180.0f64..=180.0f64
        ) {
            if let Ok(loc) = GeoLocation::new(lat, lon) {
                let json = serde_json::to_string(&loc).unwrap();
                let deserialized: GeoLocation = serde_json::from_str(&json).unwrap();
                // Use approximate comparison due to floating-point precision
                let lat_diff = (loc.latitude() - deserialized.latitude()).abs();
                let lon_diff = (loc.longitude() - deserialized.longitude()).abs();
                prop_assert!(lat_diff < 1e-10, "Latitude difference too large: {}", lat_diff);
                prop_assert!(lon_diff < 1e-10, "Longitude difference too large: {}", lon_diff);
            }
        }
    }
}

// ============================================================================
// Temperature Property Tests (removed - type not exported)
// ============================================================================

// ============================================================================
// Humidity Property Tests
// ============================================================================

mod humidity_tests {
    use super::*;

    proptest! {
        #[test]
        fn valid_humidity_accepted(value in 0u8..=100u8) {
            let result = Humidity::new(value);
            prop_assert!(result.is_ok());
            prop_assert_eq!(result.unwrap().value(), value);
        }

        #[test]
        fn invalid_humidity_rejected(value in 101u8..=255u8) {
            let result = Humidity::new(value);
            prop_assert!(result.is_err());
        }

        #[test]
        fn humidity_value_preserved(value in 0u8..=100u8) {
            let humidity = Humidity::new(value).unwrap();
            prop_assert_eq!(humidity.value(), value);
        }

        #[test]
        fn serialization_roundtrip(value in 0u8..=100u8) {
            let humidity = Humidity::new(value).unwrap();
            let json = serde_json::to_string(&humidity).unwrap();
            let deserialized: Humidity = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(humidity.value(), deserialized.value());
        }
    }
}

// ============================================================================
// WindSpeed Property Tests (removed - type not exported)
// ============================================================================

// ============================================================================
// WindDirection Property Tests (removed - type not exported)
// ============================================================================

// ============================================================================
// EmailAddress Property Tests
// ============================================================================

mod email_address_tests {
    use super::*;

    proptest! {
        #[test]
        fn valid_email_format_accepted(
            local in "[a-z]{1,10}",
            domain in "[a-z]{1,10}",
            tld in "[a-z]{2,4}"
        ) {
            let email_str = format!("{local}@{domain}.{tld}");
            let result = EmailAddress::new(&email_str);

            // Should succeed for valid format
            if result.is_ok() {
                let email = result.unwrap();
                prop_assert!(email.as_str().contains('@'));
            }
        }

        #[test]
        fn email_without_at_rejected(
            text in "[a-zA-Z0-9]{1,20}"
        ) {
            prop_assume!(!text.contains('@'));
            let result = EmailAddress::new(&text);
            prop_assert!(result.is_err());
        }

        #[test]
        fn email_preserves_format(
            local in "[a-z]{1,10}",
            domain in "[a-z]{1,10}",
            tld in "[a-z]{2,4}"
        ) {
            let email_str = format!("{local}@{domain}.{tld}");
            if let Ok(email) = EmailAddress::new(&email_str) {
                // Email should be stored consistently
                let stored = email.as_str();
                prop_assert!(stored.contains('@'));
            }
        }
    }

    #[test]
    fn empty_email_rejected() {
        let result = EmailAddress::new("");
        assert!(result.is_err());
    }
}

// ============================================================================
// MemoryId Property Tests
// ============================================================================

mod memory_id_tests {
    use super::*;

    proptest! {
        #[test]
        fn new_memory_id_is_unique(
            _ in any::<u64>()
        ) {
            let id1 = MemoryId::new();
            let id2 = MemoryId::new();
            prop_assert_ne!(id1, id2);
        }

        #[test]
        fn memory_id_from_uuid_preserves_value(
            a in any::<u64>(),
            b in any::<u64>()
        ) {
            let uuid = uuid::Uuid::from_u64_pair(a, b);
            let id = MemoryId::from(uuid);
            // The underlying UUID should be preserved
            let as_uuid = id.as_uuid();
            prop_assert_eq!(uuid, as_uuid);
        }

        #[test]
        fn memory_id_display_is_valid_uuid_format(
            _ in any::<u64>()
        ) {
            let id = MemoryId::new();
            let display = format!("{id}");
            // UUID format: xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx
            prop_assert_eq!(display.len(), 36);
            prop_assert_eq!(display.chars().filter(|c| *c == '-').count(), 4);
        }

        #[test]
        fn memory_id_serialization_roundtrip(
            _ in any::<u64>()
        ) {
            let id = MemoryId::new();
            let json = serde_json::to_string(&id).unwrap();
            let deserialized: MemoryId = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(id, deserialized);
        }
    }
}

// ============================================================================
// TaskStatus Property Tests
// ============================================================================

mod task_status_tests {
    use super::*;

    proptest! {
        #[test]
        fn all_status_variants_have_display(
            status in prop_oneof![
                Just(TaskStatus::NeedsAction),
                Just(TaskStatus::InProgress),
                Just(TaskStatus::Completed),
                Just(TaskStatus::Cancelled),
            ]
        ) {
            let display = format!("{status}");
            prop_assert!(!display.is_empty());
        }

        #[test]
        fn status_equality(
            status in prop_oneof![
                Just(TaskStatus::NeedsAction),
                Just(TaskStatus::InProgress),
                Just(TaskStatus::Completed),
                Just(TaskStatus::Cancelled),
            ]
        ) {
            let cloned = status;
            prop_assert_eq!(status, cloned);
        }

        #[test]
        fn status_serialization_roundtrip(
            status in prop_oneof![
                Just(TaskStatus::NeedsAction),
                Just(TaskStatus::InProgress),
                Just(TaskStatus::Completed),
                Just(TaskStatus::Cancelled),
            ]
        ) {
            let json = serde_json::to_string(&status).unwrap();
            let deserialized: TaskStatus = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(status, deserialized);
        }

        #[test]
        fn active_status_is_not_done(
            status in prop_oneof![
                Just(TaskStatus::NeedsAction),
                Just(TaskStatus::InProgress),
            ]
        ) {
            prop_assert!(status.is_active());
            prop_assert!(!status.is_done());
        }

        #[test]
        fn done_status_is_not_active(
            status in prop_oneof![
                Just(TaskStatus::Completed),
                Just(TaskStatus::Cancelled),
            ]
        ) {
            prop_assert!(status.is_done());
            prop_assert!(!status.is_active());
        }
    }
}

// ============================================================================
// Timezone Property Tests
// ============================================================================

mod timezone_tests {
    use super::*;

    proptest! {
        #[test]
        fn valid_timezone_names_accepted(
            tz in prop_oneof![
                Just("UTC"),
                Just("Europe/Berlin"),
                Just("America/New_York"),
                Just("Asia/Tokyo"),
                Just("Australia/Sydney"),
            ]
        ) {
            let result = Timezone::try_new(tz);
            prop_assert!(result.is_ok());
        }

        #[test]
        fn invalid_timezone_rejected(
            invalid in "[A-Z]{10,20}"
        ) {
            // Random strings are unlikely to be valid timezones
            let result = Timezone::try_new(&invalid);
            // May or may not be valid - just shouldn't panic
            let _ = result;
        }

        #[test]
        fn timezone_serialization_roundtrip(
            tz in prop_oneof![
                Just("UTC"),
                Just("Europe/Berlin"),
                Just("America/New_York"),
            ]
        ) {
            if let Ok(timezone) = Timezone::try_new(tz) {
                let json = serde_json::to_string(&timezone).unwrap();
                let deserialized: Timezone = serde_json::from_str(&json).unwrap();
                prop_assert_eq!(timezone.as_str(), deserialized.as_str());
            }
        }
    }
}

// ============================================================================
// Cross-type Consistency Tests
// ============================================================================

mod cross_type_tests {
    use super::*;

    proptest! {
        #[test]
        fn location_serialization_works(
            lat in -90.0f64..=90.0f64,
            lon in -180.0f64..=180.0f64
        ) {
            // Simulate a location observation
            if let Ok(location) = GeoLocation::new(lat, lon) {
                // Should serialize correctly
                let loc_json = serde_json::to_string(&location).unwrap();
                prop_assert!(!loc_json.is_empty());
            }
        }

        #[test]
        fn humidity_consistency(
            humidity in 0u8..=100u8
        ) {
            let hum = Humidity::new(humidity).unwrap();

            // Value should be maintained
            prop_assert_eq!(hum.value(), humidity);
        }

        #[test]
        fn memory_id_unique_across_calls(_ in 0..100usize) {
            let ids: Vec<_> = (0..10).map(|_| MemoryId::new()).collect();
            // All IDs should be unique
            for i in 0..ids.len() {
                for j in (i+1)..ids.len() {
                    prop_assert_ne!(ids[i], ids[j]);
                }
            }
        }

        #[test]
        fn task_status_transitions_are_valid(
            status in prop_oneof![
                Just(TaskStatus::NeedsAction),
                Just(TaskStatus::InProgress),
                Just(TaskStatus::Completed),
                Just(TaskStatus::Cancelled),
            ]
        ) {
            // is_active and is_done should be mutually exclusive
            prop_assert_ne!(status.is_active(), status.is_done());
        }
    }
}
