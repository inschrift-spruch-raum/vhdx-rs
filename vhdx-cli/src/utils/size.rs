/// Parse a human-readable size string (e.g., "10G", "100M", "1GiB") into bytes.
/// Uses byte-unit crate to support various units (KB, KiB, MB, MiB, etc.).
pub fn parse_size(size_str: &str) -> Result<u64, String> {
    use byte_unit::Byte;

    Byte::parse_str(size_str, true)
        .map(|b| b.as_u64())
        .map_err(|e| format!("Invalid size '{}': {e}", size_str))
}

/// Parse block size and validate it's a power of two.
/// Supports all byte-unit formats (1M, 1MiB, 1MB, etc.).
pub fn parse_block_size(size_str: &str) -> Result<u32, String> {
    use byte_unit::Byte;

    let byte = Byte::parse_str(size_str, true)
        .map_err(|e| format!("Invalid block size '{}': {e}", size_str))?;

    let size = byte.as_u64();

    if size == 0 {
        return Err("Block size cannot be zero".to_string());
    }

    if !size.is_power_of_two() {
        return Err(format!(
            "Block size '{}' ({}) must be a power of 2 (e.g., 1M, 2M, 4M, 8M, 16M, 32M, 64M)",
            size_str, byte
        ));
    }

    u32::try_from(size).map_err(|_| {
        format!(
            "Block size '{}' ({}) exceeds maximum allowed (4GB)",
            size_str, byte
        )
    })
}
