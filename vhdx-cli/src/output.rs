use std::fmt::Write;
use std::path::Path;
use vhdx::Guid;

pub(crate) struct InfoOutput<'a> {
    pub(crate) creator: &'a str,
    pub(crate) sequence_number: u64,
    pub(crate) file_write_guid: Guid,
    pub(crate) data_write_guid: Guid,
    pub(crate) version: u16,
    pub(crate) block_size: u32,
    pub(crate) logical_sector: u32,
    pub(crate) physical_sector: u32,
    pub(crate) virtual_size: u64,
    pub(crate) disk_type: &'a str,
}

pub(crate) fn print_text(path: &Path, info: &InfoOutput<'_>) {
    println!("VHDX File: {}", path.display());
    println!("Signature: vhdxfile");
    println!("Creator: {}", info.creator);
    println!("Header:");
    println!("  Sequence Number: {}", info.sequence_number);
    println!("  File Write GUID: {}", info.file_write_guid);
    println!("  Data Write GUID: {}", info.data_write_guid);
    println!("  Version: {}", info.version);
    println!("Metadata:");
    if info.block_size > 0 {
        println!("  Block Size: {} bytes", info.block_size);
        println!("  Logical Sector Size: {}", info.logical_sector);
        println!("  Physical Sector Size: {}", info.physical_sector);
        println!("  Virtual Disk Size: {}", info.virtual_size);
        println!("  Disk Type: {}", info.disk_type);
    } else {
        println!("  (no metadata found)");
    }
}

// ---------------------------------------------------------------------------
// Output helpers – JSON
// ---------------------------------------------------------------------------

pub(crate) fn print_json(path: &Path, info: &InfoOutput<'_>) {
    println!("{{");
    println!("  \"file\": {},", json_escape(&path.display().to_string()));
    println!("  \"signature\": \"vhdxfile\",");
    println!("  \"creator\": {},", json_escape(info.creator));
    println!("  \"header\": {{");
    println!("    \"sequence_number\": {},", info.sequence_number);
    println!("    \"file_write_guid\": \"{}\",", info.file_write_guid);
    println!("    \"data_write_guid\": \"{}\",", info.data_write_guid);
    println!("    \"version\": {}", info.version);
    println!("  }},");
    println!("  \"metadata\": {{");
    if info.block_size > 0 {
        println!("    \"block_size\": {},", info.block_size);
        println!("    \"logical_sector_size\": {},", info.logical_sector);
        println!("    \"physical_sector_size\": {},", info.physical_sector);
        println!("    \"virtual_disk_size\": {},", info.virtual_size);
        println!("    \"disk_type\": \"{}\"", info.disk_type);
    }
    println!("  }}");
    println!("}}");
}

/// Basic JSON string escaping.
pub(crate) fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str(r#"\""#),
            '\\' => out.push_str(r"\\"),
            '\n' => out.push_str(r"\n"),
            '\r' => out.push_str(r"\r"),
            '\t' => out.push_str(r"\t"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Decode a null-terminated (or fixed-length) UTF-16LE byte slice into a
/// [`String`].  Stops at the first `\0` code unit; trailing bytes beyond a
/// complete code unit are ignored.
pub(crate) fn decode_utf16le_creator(data: &[u8]) -> String {
    // Find the null terminator (two consecutive zero bytes).
    let end = data
        .chunks_exact(2)
        .position(|c| c[0] == 0 && c[1] == 0)
        .map_or(data.len() & !1, |pos| pos * 2); // round down to even

    let units: Vec<u16> = data[..end]
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();

    String::from_utf16(&units).unwrap_or_default()
}
