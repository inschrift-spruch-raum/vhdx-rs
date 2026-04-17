//! 大小解析工具模块
//!
//! 提供将人类可读的大小字符串（如 "10GB"、"32MiB"）解析为字节数的函数。
//! 用于命令行参数中磁盘大小和块大小的解析。

/// 解析大小字符串为字节数
///
/// 支持常见的大小单位（如 KB、MB、GB、KiB、MiB、GiB 等）。
///
/// # 参数
/// - `size_str`: 大小字符串，如 "10GB"、"100MB"、"32MiB"
///
/// # 返回
/// - `Ok(u64)`: 解析成功，返回字节数
/// - `Err(String)`: 解析失败，返回错误信息
///
/// # 示例
/// ```ignore
/// parse_size("10GB")   // -> Ok(10_000_000_000)
/// parse_size("32MiB")  // -> Ok(33_554_432)
/// ```
pub fn parse_size(size_str: &str) -> Result<u64, String> {
    use byte_unit::Byte;

    Byte::parse_str(size_str, true)
        .map(|b| b.as_u64())
        .map_err(|e| format!("Invalid size '{}': {e}", size_str))
}

/// 解析块大小字符串并进行验证
///
/// 除了解析大小字符串外，还会验证：
/// 1. 块大小不能为零
/// 2. 块大小必须是 2 的幂（如 1MiB、2MiB、4MiB 等）
/// 3. 块大小不能超过 u32 最大值（4 GiB）
///
/// # 参数
/// - `size_str`: 块大小字符串，如 "1MB"、"32MiB"
///
/// # 返回
/// - `Ok(u32)`: 解析并验证成功
/// - `Err(String)`: 解析失败或验证不通过
pub fn parse_block_size(size_str: &str) -> Result<u32, String> {
    use byte_unit::Byte;

    let byte = Byte::parse_str(size_str, true)
        .map_err(|e| format!("Invalid block size '{}': {e}", size_str))?;

    let size = byte.as_u64();

    // 块大小不能为零
    if size == 0 {
        return Err("Block size cannot be zero".to_string());
    }

    // 块大小必须是 2 的幂（VHDX 规范要求）
    if !size.is_power_of_two() {
        return Err(format!(
            "Block size '{}' ({}) must be a power of 2 (e.g., 1MiB, 2MiB, 4MiB, 8MiB, 16MiB, 32MiB, 64MiB)",
            size_str, byte
        ));
    }

    // 确保块大小不超过 u32 范围（最大 4 GiB）
    u32::try_from(size).map_err(|_| {
        format!(
            "Block size '{}' ({}) exceeds maximum allowed (4 GiB)",
            size_str, byte
        )
    })
}
