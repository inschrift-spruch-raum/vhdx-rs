/// Compute CRC-32C (Castagnoli) checksum of `data`.
#[must_use]
pub fn crc32c(data: &[u8]) -> u32 {
    crc32c::crc32c(data)
}

/// Verify that `data` matches the expected CRC-32C checksum.
#[cfg(test)]
pub fn verify_crc32c(data: &[u8], expected: u32) -> bool {
    crc32c(data) == expected
}

/// Compute the chunk ratio for BAT entry interleaving.
///
/// Formula: `(2^23 * LogicalSectorSize) / BlockSize`
///
/// The chunk ratio determines how many payload block entries appear
/// between each sector bitmap block entry in the BAT.
#[must_use]
pub(crate) fn compute_chunk_ratio(block_size: u64, logical_sector_size: u64) -> u64 {
    (1u64 << 23) * logical_sector_size / block_size
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32c_empty() {
        assert_eq!(crc32c(b""), 0x0000_0000);
    }

    #[test]
    fn crc32c_known_vector() {
        assert_eq!(crc32c(b"123456789"), 0xE306_9283);
    }

    #[test]
    fn verify_crc32c_valid() {
        let checksum = crc32c(b"123456789");
        assert!(verify_crc32c(b"123456789", checksum));
    }

    #[test]
    fn verify_crc32c_invalid() {
        assert!(!verify_crc32c(b"123456789", 0xDEAD_BEEF));
    }
}
