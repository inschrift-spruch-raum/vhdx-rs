pub fn parse_size(size_str: &str) -> Result<u64, String> {
    use byte_unit::Byte;

    Byte::parse_str(size_str, true)
        .map(|b| b.as_u64())
        .map_err(|e| format!("Invalid size '{}': {e}", size_str))
}

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
            "Block size '{}' ({}) must be a power of 2 (e.g., 1MiB, 2MiB, 4MiB, 8MiB, 16MiB, 32MiB, 64MiB)",
            size_str, byte
        ));
    }

    u32::try_from(size).map_err(|_| {
        format!(
            "Block size '{}' ({}) exceeds maximum allowed (4 GiB)",
            size_str, byte
        )
    })
}
