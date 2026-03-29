/// Parse a human-readable size string (e.g., "10G", "100M", "512K") into bytes.
pub fn parse_size(size_str: &str) -> u64 {
    let size_str = size_str.trim().to_uppercase();
    let multiplier = if size_str.ends_with('T') {
        1024u64 * 1024 * 1024 * 1024
    } else if size_str.ends_with('G') {
        1024 * 1024 * 1024
    } else if size_str.ends_with('M') {
        1024 * 1024
    } else if size_str.ends_with('K') {
        1024
    } else {
        1
    };

    let number_part = if size_str.ends_with('T')
        || size_str.ends_with('G')
        || size_str.ends_with('M')
        || size_str.ends_with('K')
        || size_str.ends_with('B')
    {
        &size_str[..size_str.len() - 1]
    } else {
        &size_str
    };

    number_part.parse::<u64>().unwrap_or(0) * multiplier
}

/// Convert bytes into a human-readable size string (e.g., "1.50 GB").
pub fn human_readable_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];

    let mut unit_index = 0;
    let mut whole = bytes;
    let mut remainder: u64 = 0;

    // Iteratively divide to find the appropriate unit and track remainder for precision
    while whole >= 1024 && unit_index < UNITS.len() - 1 {
        remainder = whole % 1024;
        whole /= 1024;
        unit_index += 1;
    }

    // Calculate the fractional part (remainder / 1024) as a value 0.0 to 0.999
    // This is safe because remainder is always < 1024, which is well within f64 precision
    let fractional = f64::from(u32::try_from(remainder).unwrap_or(0)) / 1024.0;
    let size_f64 = f64::from(u32::try_from(whole).unwrap_or(u32::MAX)) + fractional;

    format!("{:.2} {}", size_f64, UNITS[unit_index])
}
