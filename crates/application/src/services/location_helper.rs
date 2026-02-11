//! Location helper utilities
//!
//! Pure functions for generating maps links and formatting location
//! information for rich messenger messages.

/// Generate a Google Maps search link from a free-form address string
///
/// The link opens in the native Maps app on mobile (iOS/Android) or
/// in the browser on desktop.
#[must_use]
pub fn generate_maps_link(location: &str) -> String {
    let encoded = url_encode(location);
    format!("https://maps.google.com/maps?q={encoded}")
}

/// Generate a Google Maps link from coordinates
#[must_use]
pub fn generate_maps_link_coords(latitude: f64, longitude: f64) -> String {
    format!("https://maps.google.com/maps?q={latitude},{longitude}")
}

/// Format a location with a clickable maps link for messenger messages
///
/// Returns a two-line string with the location and a maps link.
#[must_use]
pub fn format_location_with_link(location: &str) -> String {
    let link = generate_maps_link(location);
    format!("📍 {location}\n🗺️ {link}")
}

/// Format a location with coordinates and a clickable maps link
#[must_use]
pub fn format_location_with_coords_link(location: &str, latitude: f64, longitude: f64) -> String {
    let link = generate_maps_link_coords(latitude, longitude);
    format!("📍 {location}\n🗺️ {link}")
}

/// URL-encode a string for use in query parameters
///
/// Encodes spaces, special characters, and Unicode for safe URL embedding.
fn url_encode(input: &str) -> String {
    let mut result = String::with_capacity(input.len() * 2);
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(byte as char);
            },
            b' ' => result.push('+'),
            _ => {
                result.push('%');
                result.push_str(&format!("{byte:02X}"));
            },
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_maps_link_simple() {
        let link = generate_maps_link("Berlin Hbf");
        assert_eq!(link, "https://maps.google.com/maps?q=Berlin+Hbf");
    }

    #[test]
    fn test_generate_maps_link_with_address() {
        let link = generate_maps_link("Alexanderplatz 1, Berlin");
        assert!(link.starts_with("https://maps.google.com/maps?q="));
        assert!(link.contains("Alexanderplatz"));
    }

    #[test]
    fn test_generate_maps_link_special_chars() {
        let link = generate_maps_link("Straße & Platz");
        assert!(link.contains("%26")); // & is encoded
    }

    #[test]
    fn test_generate_maps_link_coords() {
        let link = generate_maps_link_coords(52.520, 13.405);
        assert_eq!(link, "https://maps.google.com/maps?q=52.52,13.405");
    }

    #[test]
    fn test_format_location_with_link() {
        let formatted = format_location_with_link("Potsdamer Platz 1");
        assert!(formatted.starts_with("📍 Potsdamer Platz 1"));
        assert!(formatted.contains("🗺️ https://maps.google.com/maps"));
    }

    #[test]
    fn test_format_location_with_coords_link() {
        let formatted = format_location_with_coords_link("Berlin Hbf", 52.525, 13.369);
        assert!(formatted.starts_with("📍 Berlin Hbf"));
        assert!(formatted.contains("52.525,13.369"));
    }

    #[test]
    fn test_url_encode_simple() {
        assert_eq!(url_encode("hello"), "hello");
    }

    #[test]
    fn test_url_encode_spaces() {
        assert_eq!(url_encode("hello world"), "hello+world");
    }

    #[test]
    fn test_url_encode_special() {
        let encoded = url_encode("a@b#c");
        assert!(encoded.contains("%40")); // @
        assert!(encoded.contains("%23")); // #
    }

    #[test]
    fn test_url_encode_empty() {
        assert_eq!(url_encode(""), "");
    }

    #[test]
    fn test_url_encode_preserves_safe_chars() {
        assert_eq!(url_encode("a-b_c.d~e"), "a-b_c.d~e");
    }

    #[test]
    fn test_generate_maps_link_empty() {
        let link = generate_maps_link("");
        assert_eq!(link, "https://maps.google.com/maps?q=");
    }
}
