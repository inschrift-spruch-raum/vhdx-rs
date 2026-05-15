use crc32c::{crc32c, crc32c_append};

/// Compute CRC-32C over `data` with bytes `[4..8)` treated as zero.
///
/// VHDX structures store their CRC-32C at bytes 4..8, and the field must
/// be zeroed before computing the checksum (MS-VHDX §2.2.2, §2.2.3, §2.3.1.1).
/// This helper uses incremental CRC to skip the checksum field without
/// allocating or mutating the input: it CRCs `data[..4]`, then 4 zero bytes,
/// then `data[8..]`.
///
/// # Panics
///
/// Panics if `data` is shorter than 8 bytes.
pub(crate) fn crc32c_zeroed_checksum(data: &[u8]) -> u32 {
    let crc = crc32c(&data[..4]);
    let crc = crc32c_append(crc, &[0; 4]);
    crc32c_append(crc, &data[8..])
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
    fn crc32c_zeroed_checksum_zeroes_field_before_compute() {
        // Data with non-zero checksum field (bytes 4..8 = 0xDEAD_BEEF)
        let mut data = vec![0xFFu8; 16];
        data[4..8].copy_from_slice(&0xDEAD_BEEFu32.to_le_bytes());

        let result = crc32c_zeroed_checksum(&data);

        // Manual: zero the field then CRC
        let mut expected_buf = data.clone();
        expected_buf[4..8].fill(0);
        let expected = crc32c(&expected_buf);
        assert_eq!(result, expected);
    }

    #[test]
    fn crc32c_zeroed_checksum_does_not_mutate_original() {
        let mut data = vec![0xFFu8; 16];
        data[4..8].copy_from_slice(&0x12345678u32.to_le_bytes());
        let original = data.clone();

        let _ = crc32c_zeroed_checksum(&data);

        // Original data must be untouched
        assert_eq!(data, original);
    }

    #[test]
    fn crc32c_zeroed_checksum_all_zeros() {
        // [0; 8] with bytes 4..8 already zero → CRC is just CRC of all zeros
        let data = vec![0u8; 8];
        let result = crc32c_zeroed_checksum(&data);
        assert_eq!(result, crc32c(&data));
    }

    #[test]
    fn compute_chunk_ratio_standard_values() {
        // 32 MB blocks, 4096 logical sector size → (2^23 * 4096) / (32*1024*1024)
        assert_eq!(compute_chunk_ratio(32 * 1024 * 1024, 4096), 1024);

        // 1 MB blocks, 4096 logical sector size → (2^23 * 4096) / (1024*1024)
        assert_eq!(compute_chunk_ratio(1024 * 1024, 4096), 32768);

        // 256 MB blocks, 512 logical sector size → (2^23 * 512) / (256*1024*1024)
        assert_eq!(compute_chunk_ratio(256 * 1024 * 1024, 512), 16);
    }

    #[test]
    fn compute_chunk_ratio_large_block_small_sector() {
        // Large blocks reduce chunk ratio
        let small = compute_chunk_ratio(256 * 1024 * 1024, 512);
        let large = compute_chunk_ratio(1024 * 1024, 512);
        assert!(small < large, "larger blocks should produce smaller chunk ratio");
    }
}
