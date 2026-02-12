//! URL encoding utility for web search query parameters
//!
//! Shared by both the Brave and DuckDuckGo search clients.

/// Percent-encode a string for use in URL query parameters
///
/// Encodes all characters except unreserved characters (`A-Z`, `a-z`, `0-9`,
/// `-`, `_`, `.`, `~`). Spaces are encoded as `+`.
pub fn encode(input: &str) -> String {
    let mut result = String::with_capacity(input.len() * 3);
    for c in input.chars() {
        match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
            ' ' => result.push('+'),
            _ => {
                for b in c.to_string().as_bytes() {
                    result.push_str(&format!("%{b:02X}"));
                }
            },
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_simple_text() {
        assert_eq!(encode("hello world"), "hello+world");
    }

    #[test]
    fn encode_special_chars() {
        assert_eq!(encode("a&b=c"), "a%26b%3Dc");
    }

    #[test]
    fn encode_unreserved_chars() {
        assert_eq!(encode("abc-123_test.file~v2"), "abc-123_test.file~v2");
    }

    #[test]
    fn encode_empty() {
        assert_eq!(encode(""), "");
    }

    #[test]
    fn encode_unicode() {
        let encoded = encode("München");
        assert!(encoded.starts_with("M%C3%BC"));
    }
}
