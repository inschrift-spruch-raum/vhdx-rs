//! Log Replay algorithm implementation

use crate::error::Result;
use crate::guid::Guid;
use crate::log::entry::{LogEntry, LogSequence};

/// Log Replay algorithm implementation
pub struct LogReplayer;

impl LogReplayer {
    /// Find the active log sequence
    ///
    /// Implements the algorithm from MS-VHDX section 2.3.3
    pub fn find_active_sequence(
        log_data: &[u8],
        log_size: u32,
        log_guid: &Guid,
    ) -> Result<Option<LogSequence>> {
        let mut candidate_sequence: Option<LogSequence> = None;
        let mut current_tail: u32 = 0;
        let mut old_tail: u32 = 0;

        loop {
            // Try to build a sequence starting at current_tail
            let mut current_sequence = Vec::new();
            let mut current_seq_num: u64 = 0;
            let mut entry_offset = current_tail;

            // Scan forward looking for valid entries
            loop {
                if entry_offset as usize + 64 > log_data.len() {
                    break;
                }

                // Try to parse entry at this offset
                match LogEntry::from_bytes(&log_data[entry_offset as usize..]) {
                    Ok(entry) => {
                        // Verify LogGuid matches
                        if entry.header.log_guid != *log_guid {
                            break;
                        }

                        // Get entry length before moving entry
                        let entry_length = entry.header.entry_length;
                        let seq_num = entry.header.sequence_number;

                        // Check sequence number continuity
                        if current_sequence.is_empty() {
                            current_sequence.push(entry);
                            current_seq_num = seq_num;
                            entry_offset += entry_length;
                        } else if seq_num == current_seq_num + 1 {
                            current_sequence.push(entry);
                            current_seq_num = seq_num;
                            entry_offset += entry_length;
                        } else {
                            break;
                        }

                        // Check if this completes the sequence (tail points to start)
                        if !current_sequence.is_empty() {
                            let head = &current_sequence[current_sequence.len() - 1];
                            if head.header.tail == current_tail {
                                // Found a valid complete sequence
                                let sequence = LogSequence {
                                    head_sequence: head.header.sequence_number,
                                    tail_offset: head.header.tail,
                                    entries: current_sequence.clone(),
                                };

                                // Update candidate if this has higher sequence number
                                if candidate_sequence.is_none()
                                    || sequence.head_sequence
                                        > candidate_sequence.as_ref().unwrap().head_sequence
                                {
                                    candidate_sequence = Some(sequence);
                                }
                                break;
                            }
                        }
                    }
                    Err(_) => break,
                }
            }

            // Move to next position
            if current_sequence.is_empty() {
                current_tail = (current_tail + 4096) % log_size;
            } else {
                current_tail = entry_offset % log_size;
            }

            // Check if we've wrapped around
            if current_tail < old_tail {
                break;
            }
            old_tail = current_tail;
        }

        Ok(candidate_sequence)
    }

    /// Replay a log sequence
    ///
    /// Applies all changes from the sequence to the file
    pub fn replay_sequence<W: std::io::Write + std::io::Seek>(
        sequence: &LogSequence,
        writer: &mut W,
    ) -> Result<u64> {
        // Replay from tail to head
        for entry in &sequence.entries {
            // Replay zero descriptors
            for zero_desc in &entry.zero_descriptors {
                let zeros = vec![0u8; zero_desc.zero_length as usize];
                writer.seek(std::io::SeekFrom::Start(zero_desc.file_offset))?;
                writer.write_all(&zeros)?;
            }

            // Replay data descriptors
            for (i, data_desc) in entry.data_descriptors.iter().enumerate() {
                if let Some(sector) = entry.data_sectors.get(i) {
                    let full_data = sector.reconstruct_sector(data_desc);
                    writer.seek(std::io::SeekFrom::Start(data_desc.file_offset))?;
                    writer.write_all(&full_data)?;
                }
            }
        }

        // Return the flushed file offset from head entry (guaranteed stable size)
        if let Some(head) = sequence.entries.last() {
            Ok(head.header.flushed_file_offset)
        } else {
            Ok(0)
        }
    }
}
