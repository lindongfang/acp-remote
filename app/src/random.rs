use crate::error::PlatformError;

/// Fills a fixed-size buffer from the operating system CSPRNG.
///
/// # Errors
///
/// Returns an error when the operating system random source is unavailable.
pub fn bytes<const N: usize>() -> Result<[u8; N], PlatformError> {
    let mut value = [0_u8; N];
    getrandom::fill(&mut value).map_err(|error| PlatformError::Random(error.to_string()))?;
    Ok(value)
}

/// Returns lowercase hexadecimal CSPRNG output.
///
/// # Errors
///
/// Returns an error when the operating system random source is unavailable.
pub fn hex(byte_count: usize) -> Result<String, PlatformError> {
    let mut value = vec![0_u8; byte_count];
    getrandom::fill(&mut value).map_err(|error| PlatformError::Random(error.to_string()))?;
    Ok(hex::encode(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn produces_requested_size_and_does_not_repeat() {
        let first = hex(32).expect("random source");
        let second = hex(32).expect("random source");
        assert_eq!(first.len(), 64);
        assert_ne!(first, second);
    }
}
