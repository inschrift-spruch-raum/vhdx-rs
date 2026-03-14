//! CRC-32C (Castagnoli) implementation for VHDX checksums
//!
//! VHDX uses CRC-32C with polynomial 0x1EDC6F41 for all checksums.
//! This module wraps the `crc32c` crate and adds VHDX-specific helpers.

/// Calculate CRC-32C checksum with a specific field treated as zero.
///
/// This is used when calculating checksums for structures where the
/// checksum field itself should be treated as zeros during calculation.
pub fn crc32c_with_zero_field(data: &[u8], zero_offset: usize, zero_len: usize) -> u32 {
    // Split calculation: prefix + (treated as zeros) + suffix
    let prefix = &data[..zero_offset.min(data.len())];
    let suffix_start = (zero_offset + zero_len).min(data.len());
    let suffix = &data[suffix_start..];

    let mut crc = crc32c::crc32c(prefix);
    crc = crc32c::crc32c_append(
        crc,
        &vec![0u8; zero_len.min(data.len().saturating_sub(zero_offset))],
    );
    crc32c::crc32c_append(crc, suffix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crc32c_with_zero_field() {
        // Test data with embedded checksum field
        let mut data = vec![0u8; 16];
        data[0] = b'h';
        data[1] = b'e';
        data[2] = b'a';
        data[3] = b'd';
        // Bytes 4-7 are the "checksum field" and should be treated as zero
        data[8] = 0x08; // sequence number (little endian)

        let checksum = crc32c_with_zero_field(&data, 4, 4);
        // The checksum should be the same as calculating over data with bytes 4-7 = 0
        let mut expected_data = data.clone();
        expected_data[4..8].fill(0);
        let expected = crc32c::crc32c(&expected_data);
        assert_eq!(checksum, expected);
    }
}
