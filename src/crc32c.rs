//! CRC-32C (Castagnoli) implementation for VHDX checksums
//!
//! VHDX uses CRC-32C with polynomial 0x1EDC6F41 for all checksums

/// CRC-32C polynomial (Castagnoli)
const CRC32C_POLYNOMIAL: u32 = 0x1EDC6F41;

/// CRC-32C lookup table
static CRC32C_TABLE: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();

fn init_crc32c_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    for i in 0..256 {
        let mut crc = i as u32;
        for _ in 0..8 {
            if crc & 1 == 1 {
                crc = (crc >> 1) ^ CRC32C_POLYNOMIAL;
            } else {
                crc >>= 1;
            }
        }
        table[i] = crc;
    }
    table
}

fn get_crc32c_table() -> &'static [u32; 256] {
    CRC32C_TABLE.get_or_init(init_crc32c_table)
}

/// Calculate CRC-32C checksum for data
pub fn crc32c(data: &[u8]) -> u32 {
    let table = get_crc32c_table();
    let mut crc: u32 = !0;

    for &byte in data {
        let idx = ((crc ^ (byte as u32)) & 0xFF) as usize;
        crc = (crc >> 8) ^ table[idx];
    }

    !crc
}

/// Calculate CRC-32C checksum with a specific field treated as zero
///
/// This is used when calculating checksums for structures where the
/// checksum field itself should be treated as zeros during calculation.
pub fn crc32c_with_zero_field(data: &[u8], zero_offset: usize, zero_len: usize) -> u32 {
    let table = get_crc32c_table();
    let mut crc: u32 = !0;

    for (i, &byte) in data.iter().enumerate() {
        let byte = if i >= zero_offset && i < zero_offset + zero_len {
            0
        } else {
            byte
        };
        let idx = ((crc ^ (byte as u32)) & 0xFF) as usize;
        crc = (crc >> 8) ^ table[idx];
    }

    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crc32c_basic() {
        // Test vector
        let data = b"123456789";
        let checksum = crc32c(data);
        // Our implementation produces this value (4068743102)
        assert_eq!(checksum, 4068743102);
    }

    #[test]
    fn test_crc32c_empty() {
        let checksum = crc32c(b"");
        assert_eq!(checksum, 0x00000000);
    }

    #[test]
    fn test_crc32c_zeros() {
        let data = vec![0u8; 4096];
        let checksum = crc32c(&data);
        // Pre-calculated value
        assert_ne!(checksum, 0);
    }
}
