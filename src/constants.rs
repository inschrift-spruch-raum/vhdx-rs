//! Shared internal constants used across modules.

use bitvec::prelude::*;
use core::marker::PhantomData;

use crate::types::Guid;

// ---------------------------------------------------------------------------
// Size units
// ---------------------------------------------------------------------------
//
// The VHDX spec and common tooling use KB/MB/GB/TB to mean 1024-based units
// (kibibytes, mebibytes, etc.).  We follow the IEC 80000-13 standard instead
// and name our constants KiB, MiB, GiB, TiB to make the 1024-based semantics
// unambiguous.

/// Kibibyte.
pub(crate) const KIB: u16 = 1024;

/// Mebibyte.
pub(crate) const MIB: u32 = 1024 * (KIB as u32);

/// Gibibyte.
pub(crate) const GIB: u32 = 1024 * MIB;

/// Tebibyte.
pub(crate) const TIB: u64 = 1024 * (GIB as u64);

// ---------------------------------------------------------------------------
// Header/region layout offsets
// ---------------------------------------------------------------------------

/// Offset of Header 1 within the header section/file.
pub(crate) const HEADER1_OFFSET: u32 = 64 * (KIB as u32);

/// Offset of Header 2 within the header section/file.
pub(crate) const HEADER2_OFFSET: u32 = 128 * (KIB as u32);

/// Offset of Region Table 1 within the header section/file.
pub(crate) const REGION_TABLE1_OFFSET: u32 = 192 * (KIB as u32);

/// Offset of Region Table 2 within the header section/file.
pub(crate) const REGION_TABLE2_OFFSET: u32 = 256 * (KIB as u32);

/// Log starts at 1 MiB (first MiB-aligned slot after the header section).
pub(crate) const LOG_OFFSET: u32 = MIB;

/// Minimum log length is 1 MiB.
pub(crate) const LOG_LENGTH: u32 = MIB;

/// BAT region starts at 2 MiB (right after the log).
pub(crate) const BAT_REGION_OFFSET: u32 = 2 * MIB;

/// Metadata region default size: 1 MiB.
pub(crate) const METADATA_REGION_SIZE: u32 = MIB;

// ---------------------------------------------------------------------------
// Core structure sizes
// ---------------------------------------------------------------------------

/// Size of a VHDX header structure in bytes.
pub(crate) const HEADER_SIZE: u16 = 4096;

/// Size of a VHDX region table in bytes.
pub(crate) const REGION_TABLE_SIZE: u32 = 64 * 1024;

/// Metadata table fixed size: 64 KiB.
pub(crate) const METADATA_TABLE_SIZE: u32 = 64 * 1024;

/// Metadata table header size in bytes.
pub(crate) const TABLE_HEADER_SIZE: u8 = 32;

/// Metadata table entry size in bytes.
pub(crate) const TABLE_ENTRY_SIZE: u8 = 32;

/// Parent locator header size in bytes.
pub(crate) const LOCATOR_HEADER_SIZE: u8 = 20;

/// Parent locator key-value entry size in bytes.
pub(crate) const KV_ENTRY_SIZE: u8 = 12;

// ---------------------------------------------------------------------------
// Region table layout
// ---------------------------------------------------------------------------

/// Size of each region table entry (32 bytes per MS-VHDX).
pub(crate) const REGION_ENTRY_SIZE: u8 = 32;

/// Maximum number of region table entries per MS-VHDX.
pub(crate) const MAX_REGION_ENTRIES: u16 = 2047;

/// Creator field size in the file type identifier (512 bytes).
pub(crate) const CREATOR_SIZE: u16 = 512;

/// Region table header size: 16 bytes (signature + checksum + `entry_count` + reserved).
pub(crate) const RT_HEADER_SIZE: u8 = 16;

// ---------------------------------------------------------------------------
// Log layout
// ---------------------------------------------------------------------------

/// Log entry header size in bytes.
pub(crate) const ENTRY_HEADER_SIZE: u8 = 64;

/// Log descriptor size in bytes.
pub(crate) const DESCRIPTOR_SIZE: u8 = 32;

// ---------------------------------------------------------------------------
// Log signatures
// ---------------------------------------------------------------------------

/// VHDX log/data sector size in bytes.
pub(crate) const SECTOR_SIZE: u16 = 4096;

/// Log entry signature.
pub(crate) const SIGNATURE_LOGE: BitArray<u32, Lsb0> = BitArray {
    data: u32::from_le_bytes(*b"loge"),
    _ord: PhantomData,
};

/// Data descriptor signature.
pub(crate) const SIGNATURE_DESC: BitArray<u32, Lsb0> = BitArray {
    data: u32::from_le_bytes(*b"desc"),
    _ord: PhantomData,
};

/// Zero descriptor signature.
pub(crate) const SIGNATURE_ZERO: BitArray<u32, Lsb0> = BitArray {
    data: u32::from_le_bytes(*b"zero"),
    _ord: PhantomData,
};

/// Data sector signature.
#[cfg(test)]
pub(crate) const SIGNATURE_DATA: BitArray<u32, Lsb0> = BitArray {
    data: u32::from_le_bytes(*b"data"),
    _ord: PhantomData,
};

// ---------------------------------------------------------------------------
// File/structure signatures
// ---------------------------------------------------------------------------

/// VHDX file signature bytes: "vhdxfile" (8 bytes).
pub(crate) const VHDX_SIGNATURE_BYTES: BitArray<u64, Lsb0> = BitArray {
    data: u64::from_le_bytes(*b"vhdxfile"),
    _ord: PhantomData,
};

/// Header structure signature: "head".
pub(crate) const HEADER_SIGNATURE: BitArray<u32, Lsb0> = BitArray {
    data: u32::from_le_bytes(*b"head"),
    _ord: PhantomData,
};

/// Region table signature: "regi".
pub(crate) const REGION_SIGNATURE: BitArray<u32, Lsb0> = BitArray {
    data: u32::from_le_bytes(*b"regi"),
    _ord: PhantomData,
};

/// Metadata table signature: "metadata" (8 bytes).
pub(crate) const METADATA_SIGNATURE: BitArray<u64, Lsb0> = BitArray {
    data: u64::from_le_bytes(*b"metadata"),
    _ord: PhantomData,
};

// ---------------------------------------------------------------------------
// Known region GUIDs
// ---------------------------------------------------------------------------

/// BAT region GUID: 2DC27766-F623-4200-9D64-115E9BFD4A08.
pub(crate) const BAT_REGION_GUID: Guid = Guid::from_bytes([
    0x66, 0x77, 0xC2, 0x2D, 0x23, 0xF6, 0x00, 0x42, 0x9D, 0x64, 0x11, 0x5E, 0x9B, 0xFD, 0x4A, 0x08,
]);

/// Metadata region GUID: 8B7CA206-4790-4B9A-B8FE-575F050F886E.
pub(crate) const METADATA_REGION_GUID: Guid = Guid::from_bytes([
    0x06, 0xA2, 0x7C, 0x8B, 0x90, 0x47, 0x9A, 0x4B, 0xB8, 0xFE, 0x57, 0x5F, 0x05, 0x0F, 0x88, 0x6E,
]);
