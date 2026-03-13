//! BAT Entry states for payload blocks and sector bitmap blocks

use crate::error::{Result, VhdxError};

/// BAT Entry state for payload blocks
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadBlockState {
    /// Block is not present (unallocated)
    NotPresent = 0,
    /// Block content is undefined
    Undefined = 1,
    /// Block should be read as all zeros
    Zero = 2,
    /// Block was unmapped via TRIM/UNMAP
    Unmapped = 3,
    /// Block is fully present in this file
    FullyPresent = 6,
    /// Block is partially present (differencing disk only)
    PartiallyPresent = 7,
}

impl PayloadBlockState {
    /// Parse from 3-bit value
    pub fn from_bits(bits: u8) -> Result<Self> {
        match bits {
            0 => Ok(PayloadBlockState::NotPresent),
            1 => Ok(PayloadBlockState::Undefined),
            2 => Ok(PayloadBlockState::Zero),
            3 => Ok(PayloadBlockState::Unmapped),
            6 => Ok(PayloadBlockState::FullyPresent),
            7 => Ok(PayloadBlockState::PartiallyPresent),
            _ => Err(VhdxError::InvalidBatEntry),
        }
    }

    /// Convert to bits
    pub fn to_bits(self) -> u8 {
        self as u8
    }

    /// Check if data should be read from this file
    pub fn is_present(&self) -> bool {
        matches!(
            self,
            PayloadBlockState::FullyPresent | PayloadBlockState::PartiallyPresent
        )
    }

    /// Check if block should return zeros
    pub fn is_zero(&self) -> bool {
        matches!(
            self,
            PayloadBlockState::Zero | PayloadBlockState::NotPresent
        )
    }
}

/// BAT Entry state for sector bitmap blocks
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectorBitmapState {
    /// Block is not present (unallocated)
    NotPresent = 0,
    /// Block is present
    Present = 6,
}

impl SectorBitmapState {
    /// Parse from 3-bit value
    pub fn from_bits(bits: u8) -> Result<Self> {
        match bits {
            0 => Ok(SectorBitmapState::NotPresent),
            6 => Ok(SectorBitmapState::Present),
            _ => Err(VhdxError::InvalidBatEntry),
        }
    }

    /// Convert to bits
    pub fn to_bits(self) -> u8 {
        self as u8
    }
}
