//! GUID handling for VHDX
//!
//! VHDX uses 128-bit GUIDs in various structures

use std::fmt;

/// A 128-bit GUID as used in VHDX files
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Guid(pub [u8; 16]);

impl Guid {
    /// Create a new GUID from bytes
    pub fn new(bytes: [u8; 16]) -> Self {
        Guid(bytes)
    }

    /// Create a new random GUID (v4)
    pub fn new_v4() -> Self {
        let mut bytes = [0u8; 16];
        // Use a simple random number generator for now
        // In production, use a proper crypto RNG
        for i in 0..16 {
            bytes[i] = rand::random();
        }
        // Set version and variant bits for v4 UUID
        bytes[6] = (bytes[6] & 0x0F) | 0x40;
        bytes[8] = (bytes[8] & 0x3F) | 0x80;
        Guid(bytes)
    }

    /// Convert to bytes (little-endian as stored in VHDX)
    pub fn to_bytes(&self) -> [u8; 16] {
        self.0
    }

    /// Create from bytes (little-endian as stored in VHDX)
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Guid(bytes)
    }

    /// Check if this is a zero/empty GUID
    pub fn is_zero(&self) -> bool {
        self.0.iter().all(|&b| b == 0)
    }
}

impl fmt::Display for Guid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Format as standard GUID string: xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx
        write!(f, 
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            self.0[3], self.0[2], self.0[1], self.0[0],
            self.0[5], self.0[4],
            self.0[7], self.0[6],
            self.0[8], self.0[9],
            self.0[10], self.0[11], self.0[12], self.0[13], self.0[14], self.0[15]
        )
    }
}

// Known GUIDs for VHDX regions
impl Guid {
    /// BAT Region GUID: 2DC27766-F623-4200-9D64-115E9BFD4A08
    pub const BAT: Guid = Guid([
        0x66, 0x77, 0xC2, 0x2D, 0x23, 0xF6, 0x00, 0x42, 0x9D, 0x64, 0x11, 0x5E, 0x9B, 0xFD, 0x4A,
        0x08,
    ]);

    /// Metadata Region GUID: 8B7CA206-4790-4B9A-B8FE-575F050F886E
    pub const METADATA: Guid = Guid([
        0x06, 0xA2, 0x7C, 0x8B, 0x90, 0x47, 0x9A, 0x4B, 0xB8, 0xFE, 0x57, 0x5F, 0x05, 0x0F, 0x88,
        0x6E,
    ]);
}

// Need rand crate for GUID generation
use rand;
