//! VHDX Log System structures and operations
//!
//! The log ensures crash consistency for metadata updates.
//! It must be replayed before any writes are allowed.

mod descriptor;
mod entry;
mod replayer;
mod sector;
mod writer;

pub use descriptor::{DataDescriptor, ZeroDescriptor};
pub use entry::{LogEntry, LogEntryHeader, LogSequence};
pub use replayer::LogReplayer;
pub use sector::DataSector;
pub use writer::LogWriter;

/// Log Entry signature: "loge"
pub const LOG_ENTRY_SIGNATURE: &[u8] = b"loge";

/// Zero Descriptor signature: "zero"
pub const ZERO_DESCRIPTOR_SIGNATURE: &[u8] = b"zero";

/// Data Descriptor signature: "desc"
pub const DATA_DESCRIPTOR_SIGNATURE: &[u8] = b"desc";

/// Data Sector signature: "data"
pub const DATA_SECTOR_SIGNATURE: &[u8] = b"data";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crc32c::crc32c_with_zero_field;
    use byteorder::LittleEndian;

    #[test]
    fn test_log_entry_header() {
        let mut data = vec![0u8; 4096];
        data[0..4].copy_from_slice(LOG_ENTRY_SIGNATURE);
        LittleEndian::write_u32(&mut data[8..12], 4096); // entry_length
        LittleEndian::write_u32(&mut data[12..16], 0); // tail
        LittleEndian::write_u64(&mut data[16..24], 1); // sequence_number
        LittleEndian::write_u32(&mut data[24..28], 0); // descriptor_count

        // Calculate and write checksum
        let checksum = crc32c_with_zero_field(&data, 4, 4);
        LittleEndian::write_u32(&mut data[4..8], checksum);

        let header = LogEntryHeader::from_bytes(&data).unwrap();
        assert!(header.verify_checksum(&data));
        assert_eq!(header.sequence_number, 1);
        assert_eq!(header.entry_length, 4096);
    }

    #[test]
    fn test_zero_descriptor() {
        let mut data = vec![0u8; 32];
        data[0..4].copy_from_slice(ZERO_DESCRIPTOR_SIGNATURE);
        LittleEndian::write_u64(&mut data[8..16], 4096); // zero_length
        LittleEndian::write_u64(&mut data[16..24], 4096); // file_offset
        LittleEndian::write_u64(&mut data[24..32], 1); // sequence_number

        let desc = ZeroDescriptor::from_bytes(&data).unwrap();
        assert_eq!(desc.zero_length, 4096);
        assert_eq!(desc.file_offset, 4096);
        assert!(desc.verify_sequence(1));
    }

    #[test]
    fn test_data_descriptor() {
        let mut data = vec![0u8; 32];
        data[0..4].copy_from_slice(DATA_DESCRIPTOR_SIGNATURE);
        data[4..8].copy_from_slice(&[1, 2, 3, 4]); // trailing_bytes
        data[8..16].copy_from_slice(&[5, 6, 7, 8, 9, 10, 11, 12]); // leading_bytes
        LittleEndian::write_u64(&mut data[16..24], 4096); // file_offset
        LittleEndian::write_u64(&mut data[24..32], 1); // sequence_number

        let desc = DataDescriptor::from_bytes(&data).unwrap();
        assert_eq!(desc.file_offset, 4096);
        assert!(desc.verify_sequence(1));
    }

    #[test]
    fn test_data_sector() {
        let mut data = vec![0u8; 4096];
        data[0..4].copy_from_slice(DATA_SECTOR_SIGNATURE);
        LittleEndian::write_u32(&mut data[4..8], 0); // sequence_high
                                                     // data in middle
        LittleEndian::write_u32(&mut data[4092..4096], 1); // sequence_low

        let sector = DataSector::from_bytes(&data).unwrap();
        assert_eq!(sector.sequence_number(), 1);
    }
}
