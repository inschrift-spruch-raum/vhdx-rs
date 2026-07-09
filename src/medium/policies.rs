//! File access and log replay policies.

use crate::error::{Error, Result};

// Policies
// ---------------------------------------------------------------------------

/// Log replay policy controlling how pending logs are handled on open.
///
/// # Standard
///
/// MS-VHDX §2.3 + MS-VHDX-只读扩展标准 §4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogReplayPolicy {
    /// If a replayable log exists, `finish()` returns
    /// [`crate::error::Error::LogReplayRequired`]. No implicit replay.
    ///
    /// Standard: MS-VHDX-只读扩展标准 §4.1
    #[default]
    Require,

    /// Automatically replay the log during `finish()`.
    /// On read-only open, replay is done in memory only.
    ///
    /// Standard: MS-VHDX-只读扩展标准 §4.2
    Auto,

    /// In-memory replay is allowed for read-only opens.
    /// Not valid for read-write opens.
    ///
    /// Standard: MS-VHDX-只读扩展标准 §4.3
    InMemoryOnReadOnly,

    /// Open read-only without replaying the log.
    /// Only structure-level reads are guaranteed consistent;
    /// payload data-plane reads may be inconsistent.
    ///
    /// Standard: MS-VHDX-只读扩展标准 §4.4
    ReadOnlyNoReplay,
}

/// BAT read semantics policy.
///
/// Controls whether effective data or raw data is preferred when resolving
/// block reads. For differencing disks, child data is always preferred
/// regardless of this setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReadSemanticsPolicy {
    /// Prefer effective (possibly parent-assembled) data.
    #[default]
    EffectiveDataPreferred,
    /// Prefer raw on-disk data.
    RawDataPreferred,
}

/// Request passed to a parent resolver when a differencing disk needs parent data.
pub struct ParentRequest<'a> {
    pub locator: crate::metadata::ParentLocator<'a>,
    pub expected_data_write_guid: crate::types::Guid,
    pub child_logical_sector_size: u32,
    pub child_virtual_disk_size: u64,
}

impl ParentRequest<'_> {
    /// Parent locator metadata from the child disk.
    #[must_use]
    pub fn locator(&self) -> &crate::metadata::ParentLocator<'_> {
        &self.locator
    }

    /// Logical sector size required by the child disk.
    #[must_use]
    pub fn child_logical_sector_size(&self) -> u32 {
        self.child_logical_sector_size
    }

    /// Parent Data Write GUID expected by the child locator.
    #[must_use]
    pub fn expected_data_write_guid(&self) -> crate::types::Guid {
        self.expected_data_write_guid
    }

    /// Virtual disk size required by the child disk.
    #[must_use]
    pub fn child_virtual_disk_size(&self) -> u64 {
        self.child_virtual_disk_size
    }
}

/// A resolved parent medium that can serve effective parent sectors.
pub trait ParentMedium: Send {
    /// Parent Data Write GUID used for differencing linkage validation.
    ///
    /// # Errors
    ///
    /// Returns an error if the parent GUID cannot be read.
    fn data_write_guid(&mut self) -> Result<crate::types::Guid>;

    /// Parent logical sector size in bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the parent sector size cannot be read.
    fn logical_sector_size(&mut self) -> Result<u32>;

    /// Read one effective parent sector into `buf`.
    ///
    /// # Errors
    ///
    /// Returns an error if the requested parent sector cannot be read.
    fn read_sector(&mut self, sector: u64, buf: &mut [u8]) -> Result<()>;

    /// Read a contiguous range of effective parent sectors into `buf`.
    ///
    /// The default implementation loops through [`ParentMedium::read_sector`]
    /// so existing parent implementations only need to override this when they
    /// can satisfy the range more efficiently.
    ///
    /// # Errors
    ///
    /// Returns an error if `buf` does not exactly match the requested sector
    /// range or if any sector in the range cannot be read.
    fn read_sector_range(
        &mut self, start_sector: u64, sector_count: u64, buf: &mut [u8],
    ) -> Result<()> {
        if sector_count == 0 {
            return Err(Error::InvalidParameter("sector_count must be >= 1".into()));
        }

        let logical_sector_size = usize::try_from(self.logical_sector_size()?).map_err(|_| {
            Error::InvalidParameter("logical sector size does not fit usize".into())
        })?;
        let expected_len = usize::try_from(sector_count)
            .ok()
            .and_then(|count| count.checked_mul(logical_sector_size))
            .ok_or_else(|| {
                Error::InvalidParameter("sector_count * logical sector size overflow".into())
            })?;
        if buf.len() != expected_len {
            return Err(Error::InvalidParameter(format!(
                "parent sector range buffer length must equal sector_count * logical sector size: got {}, expected {expected_len}",
                buf.len()
            )));
        }

        for offset_sector in 0..sector_count {
            let sector = start_sector
                .checked_add(offset_sector)
                .ok_or_else(|| Error::InvalidParameter("start_sector + offset overflow".into()))?;
            let offset = usize::try_from(offset_sector).expect("sector offset fits usize")
                * logical_sector_size;
            self.read_sector(sector, &mut buf[offset..offset + logical_sector_size])?;
        }

        Ok(())
    }
}

impl<T> ParentMedium for std::cell::RefCell<T>
where
    T: ParentMedium,
{
    fn data_write_guid(&mut self) -> Result<crate::types::Guid> {
        self.borrow_mut().data_write_guid()
    }

    fn logical_sector_size(&mut self) -> Result<u32> {
        self.borrow_mut().logical_sector_size()
    }

    fn read_sector(&mut self, sector: u64, buf: &mut [u8]) -> Result<()> {
        self.borrow_mut().read_sector(sector, buf)
    }

    fn read_sector_range(
        &mut self, start_sector: u64, sector_count: u64, buf: &mut [u8],
    ) -> Result<()> {
        self.borrow_mut()
            .read_sector_range(start_sector, sector_count, buf)
    }
}

/// User-provided resolver for differencing disk parent media.
pub trait ParentResolver {
    /// Resolve the parent medium for a child request.
    ///
    /// # Errors
    ///
    /// Returns an error if the parent medium cannot be resolved.
    fn resolve_parent(&mut self, request: ParentRequest<'_>) -> Result<Box<dyn ParentMedium>>;
}
