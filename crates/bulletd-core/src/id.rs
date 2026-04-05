use rand::Rng;

use crate::error::Error;

/// Generate a random 8-character lowercase hex ID.
///
/// Note: This does not guarantee uniqueness on its own. Callers must check
/// the generated ID against existing IDs in the target file and retry if
/// a collision is detected (see `Error::DuplicateId`).
pub fn generate_id() -> String {
    let mut rng = rand::rng();
    let value: u32 = rng.random();
    format!("{value:08x}")
}

/// Validate that a string is a valid bullet ID (8-char lowercase hex).
pub fn validate_id(id: &str) -> crate::error::Result<()> {
    if id.len() != 8
        || !id
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    {
        return Err(Error::InvalidIdFormat { id: id.to_string() });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn generate_id_format() {
        let id = generate_id();
        assert_eq!(id.len(), 8);
        assert!(
            id.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "ID should be lowercase hex: {id}"
        );
    }

    #[test]
    fn generate_id_statistical_uniqueness() {
        // This test verifies that the random distribution is unlikely to produce
        // collisions in practice. It does NOT guarantee uniqueness — callers must
        // check against existing IDs (see Error::DuplicateId).
        let ids: HashSet<String> = (0..1000).map(|_| generate_id()).collect();
        assert_eq!(ids.len(), 1000);
    }

    #[test]
    fn validate_id_valid() {
        assert!(validate_id("a7f3b2c1").is_ok());
        assert!(validate_id("00000000").is_ok());
        assert!(validate_id("ffffffff").is_ok());
    }

    #[test]
    fn validate_id_invalid() {
        assert!(validate_id("short").is_err());
        assert!(validate_id("toolongid").is_err());
        assert!(validate_id("ABCDEF12").is_err());
        assert!(validate_id("a7f3g2c1").is_err());
        assert!(validate_id("").is_err());
    }
}
