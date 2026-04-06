use rand::Rng;

use crate::error::Error;

/// Generate a random short ID: one lowercase letter + one digit (e.g. "a3", "k7").
///
/// Note: This does not guarantee uniqueness on its own. Callers must check
/// the generated ID against existing IDs in the target file and retry if
/// a collision is detected (see `Error::DuplicateId`).
pub fn generate_id() -> String {
    let mut rng = rand::rng();
    let letter = (b'a' + rng.random_range(0..26u8)) as char;
    let digit = rng.random_range(0..10u8);
    format!("{letter}{digit}")
}

/// Validate that a string is a valid bullet ID (one lowercase letter + one digit).
pub fn validate_id(id: &str) -> crate::error::Result<()> {
    let chars: Vec<char> = id.chars().collect();
    if chars.len() == 2 && chars[0].is_ascii_lowercase() && chars[1].is_ascii_digit() {
        Ok(())
    } else {
        Err(Error::InvalidIdFormat { id: id.to_string() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn generate_id_format() {
        let id = generate_id();
        assert_eq!(id.len(), 2);
        let chars: Vec<char> = id.chars().collect();
        assert!(
            chars[0].is_ascii_lowercase(),
            "first char should be a-z: {id}"
        );
        assert!(chars[1].is_ascii_digit(), "second char should be 0-9: {id}");
    }

    #[test]
    fn generate_id_statistical_uniqueness() {
        // With 260 possible IDs, 50 should be unique nearly always
        let ids: HashSet<String> = (0..50).map(|_| generate_id()).collect();
        assert!(
            ids.len() >= 40,
            "expected mostly unique IDs, got {}",
            ids.len()
        );
    }

    #[test]
    fn validate_id_valid() {
        assert!(validate_id("a3").is_ok());
        assert!(validate_id("z0").is_ok());
        assert!(validate_id("m9").is_ok());
    }

    #[test]
    fn validate_id_invalid() {
        assert!(validate_id("").is_err());
        assert!(validate_id("a").is_err());
        assert!(validate_id("3a").is_err());
        assert!(validate_id("A3").is_err());
        assert!(validate_id("ab").is_err());
        assert!(validate_id("a33").is_err());
        assert!(validate_id("a7f3b2c1").is_err());
    }
}
