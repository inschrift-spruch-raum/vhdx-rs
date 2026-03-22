# Sections

[Back to API Documentation](../API.md)

## Overview

VHDX file physical structure mapping via Section containers. The `section` module provides types and containers that map directly to the physical structure of VHDX files, including the Header, BAT (Block Allocation Table), Metadata, and Log sections.

## API Tree

```
section::                               # Section模块 - 物理文件结构映射
├── Sections                            # 容器，管理所有sections (懒加载)
│   ├── header(&self) -> &Header
│   ├── bat(&self) -> &Bat
│   ├── metadata(&self) -> &Metadata
│   └── log(&self) -> &Log
│
├── Header                              # Header Section (1 MB)
│   ├── raw(&self) -> &[u8]
│   ├── file_type(&self) -> &FileTypeIdentifier
│   ├── header(&self, index: usize) -> Option<&HeaderStructure>
│   └── region_table(&self, index: usize) -> Option<&RegionTable>
│
│   └── FileTypeIdentifier
│       └── raw(&self) -> &[u8]
│
│   └── HeaderStructure
│       └── raw(&self) -> &[u8]
│
│   └── RegionTable
│       ├── raw(&self) -> &[u8]
│       └── RegionTableHeader
│           └── raw(&self) -> &[u8]
│       └── RegionTableEntry
│           └── raw(&self) -> &[u8]
│
├── Bat                                 # BAT Section
│   ├── raw(&self) -> &[u8]
│   ├── entry(&self, index: u64) -> Option<&BatEntry>
│   ├── entries(&self) -> &[BatEntry]
│   └── len(&self) -> usize
│
│   └── BatEntry
│       └── raw(&self) -> u64
│
│       └──BatState:
│          ├── Payload(PayloadBlockState)
│          └── SectorBitmap(SectorBitmapState)
│
│          └── PayloadBlockState
│              ├── NotPresent
│              ├── Undefined
│              ├── Zero
│              ├── Unmapped
│              ├── FullyPresent
│              └── PartiallyPresent
│
│          └── SectorBitmapState
│              ├── NotPresent
│              └── Present
│
├── Metadata                            # Metadata Section
│   ├── raw(&self) -> &[u8]
│   ├── table(&self) -> &MetadataTable
│   └── items(&self) -> &MetadataItems
│
│   └── MetadataTable
│       ├── raw(&self) -> &[u8]
│       ├── header(&self) -> &TableHeader
│       ├── entry(&self, item_id: &Guid) -> Option<&TableEntry>
│       └── entries(&self) -> &[TableEntry]
│
│       └── TableHeader
│           └── raw(&self) -> &[u8]
│
│       └── TableEntry
│           ├── raw(&self) -> &[u8]
│           └── flags(&self) -> &EntryFlags
│
│           └── EntryFlags
│               ├── is_user(&self) -> bool
│               ├── is_virtual_disk(&self) -> bool
│               └── is_required(&self) -> bool
│
│   └── MetadataItems
│       ├── file_parameters(&self) -> Option<&FileParameters>
│       ├── virtual_disk_size(&self) -> Option<u64>
│       ├── virtual_disk_id(&self) -> Option<&Guid>
│       ├── logical_sector_size(&self) -> Option<u32>
│       ├── physical_sector_size(&self) -> Option<u32>
│       └── parent_locator(&self) -> Option<&ParentLocator>
│
│       └── FileParameters
│           ├── raw(&self) -> &[u8]
│           ├── block_size(&self) -> u32
│           ├── leave_block_allocated(&self) -> bool
│           └── has_parent(&self) -> bool
│
│       └── ParentLocator
│           ├── raw(&self) -> &[u8]
│           ├── header(&self) -> &LocatorHeader
│           ├── entry(&self, index: usize) -> Option<&KeyValueEntry>
│           ├── entries(&self) -> &[KeyValueEntry]
│           └── key_value_data(&self) -> &[u8]
│
│           └── LocatorHeader
│               └── raw(&self) -> &[u8]
│
│           └── KeyValueEntry
│               ├── raw(&self) -> &[u8]
│               ├── key(&self, data: &[u8]) -> Option<String>
│               └── value(&self, data: &[u8]) -> Option<String>
│
└── Log                                 # Log Section
    ├── raw(&self) -> &[u8]
    ├── entry(&self, index: usize) -> Option<&Entry>
    └── entries(&self) -> &[Entry]

    └── Entry                           # Log Entry
        ├── raw(&self) -> &[u8]
        ├── header(&self) -> &LogEntryHeader
        ├── descriptor(&self, index: usize) -> Option<&Descriptor>
        ├── descriptors(&self) -> &[Descriptor]
        └── data(&self) -> &[DataSector]

        └── Descriptor                  # Descriptor 枚举
            ├── raw(&self) -> &[u8]
            ├── Data(DataDescriptor)    # Data Descriptor 变体
            │
            └── Zero(ZeroDescriptor)    # Zero Descriptor 变体

            └── DataDescriptor          # Data Descriptor
                └── raw(&self) -> &[u8]

            └── ZeroDescriptor          # Zero Descriptor
                └── raw(&self) -> &[u8]

        └── LogEntryHeader              # Log Entry Header
            └── raw(&self) -> &[u8]

        └── DataSector                  # Data Sector
            └── raw(&self) -> &[u8]
```

## Detailed Design

### 4. Section Containers

```rust
/// VHDX文件中的所有Section的容器
pub struct Sections;

impl Sections {
    pub fn header(&self) -> &Header;
    pub fn bat(&self) -> &Bat;
    pub fn metadata(&self) -> &Metadata;
    pub fn log(&self) -> &Log;
}
```

The `Sections` container provides lazy-loaded access to all major sections of a VHDX file. Each section is parsed on first access and cached for subsequent calls.

### 5. Header Section

```rust
/// Header Section (1 MB)
pub struct Header;

impl Header {
    /// Returns the raw bytes of the header section
    pub fn raw(&self) -> &[u8];
    
    /// Returns the file type identifier
    pub fn file_type(&self) -> &FileTypeIdentifier;
    
    /// Returns the header structure at the specified index (0 or 1 for dual headers)
    pub fn header(&self, index: usize) -> Option<&HeaderStructure>;
    
    /// Returns the region table at the specified index (0 or 1 for dual region tables)
    pub fn region_table(&self, index: usize) -> Option<&RegionTable>;
}

/// File Type Identifier
pub struct FileTypeIdentifier;

impl FileTypeIdentifier {
    /// Returns the raw bytes of the file type identifier
    pub fn raw(&self) -> &[u8];
}

/// Header Structure
pub struct HeaderStructure;

impl HeaderStructure {
    /// Returns the raw bytes of the header structure
    pub fn raw(&self) -> &[u8];
}

/// Region Table
pub struct RegionTable;

impl RegionTable {
    /// Returns the raw bytes of the region table
    pub fn raw(&self) -> &[u8];
    
    /// Returns the region table header
    pub fn header(&self) -> &RegionTableHeader;
    
    /// Returns all region table entries
    pub fn entries(&self) -> &[RegionTableEntry];
}

/// Region Table Header
pub struct RegionTableHeader;

impl RegionTableHeader {
    /// Returns the raw bytes of the region table header
    pub fn raw(&self) -> &[u8];
}

/// Region Table Entry
pub struct RegionTableEntry;

impl RegionTableEntry {
    /// Returns the raw bytes of the region table entry
    pub fn raw(&self) -> &[u8];
}
```

The Header section contains the file signature, header structures, and region tables that describe the layout of other sections in the VHDX file. VHDX uses a dual-header design for redundancy.

### 6. BAT Section

```rust
/// BAT (Block Allocation Table) Section
pub struct Bat;

impl Bat {
    /// Returns the raw bytes of the BAT section
    pub fn raw(&self) -> &[u8];
    
    /// Returns the BAT entry at the specified index
    pub fn entry(&self, index: u64) -> Option<&BatEntry>;
    
    /// Returns all BAT entries
    pub fn entries(&self) -> &[BatEntry];
    
    /// Returns the number of BAT entries
    pub fn len(&self) -> usize;
}

/// BAT Entry
pub struct BatEntry;

impl BatEntry {
    /// Returns the raw 64-bit value of the BAT entry
    pub fn raw(&self) -> u64;
    
    /// Returns the state of the BAT entry
    pub fn state(&self) -> BatState;
}

/// BAT State
pub enum BatState {
    /// Payload block state
    Payload(PayloadBlockState),
    /// Sector bitmap state
    SectorBitmap(SectorBitmapState),
}

/// Payload Block State
pub enum PayloadBlockState {
    /// Block is not present in the file
    NotPresent,
    /// Block is in an undefined state
    Undefined,
    /// Block is zero-filled (not stored)
    Zero,
    /// Block is unmapped
    Unmapped,
    /// Block is fully present in the file
    FullyPresent,
    /// Block is partially present (for differencing disks)
    PartiallyPresent,
}

/// Sector Bitmap State
pub enum SectorBitmapState {
    /// Sector bitmap is not present
    NotPresent,
    /// Sector bitmap is present
    Present,
}
```

The BAT (Block Allocation Table) maps virtual disk blocks to their physical locations in the file. Each entry contains the file offset and state of the corresponding block.

### 7. Metadata Section

```rust
/// Metadata Section
pub struct Metadata;

impl Metadata {
    /// Returns the raw bytes of the metadata section
    pub fn raw(&self) -> &[u8];
    
    /// Returns the metadata table
    pub fn table(&self) -> &MetadataTable;
    
    /// Returns the metadata items
    pub fn items(&self) -> &MetadataItems;
}

/// Metadata Table
pub struct MetadataTable;

impl MetadataTable {
    /// Returns the raw bytes of the metadata table
    pub fn raw(&self) -> &[u8];
    
    /// Returns the table header
    pub fn header(&self) -> &TableHeader;
    
    /// Returns the table entry for the specified item ID
    pub fn entry(&self, item_id: &Guid) -> Option<&TableEntry>;
    
    /// Returns all table entries
    pub fn entries(&self) -> &[TableEntry];
}

/// Table Header
pub struct TableHeader;

impl TableHeader {
    /// Returns the raw bytes of the table header
    pub fn raw(&self) -> &[u8];
}

/// Table Entry
pub struct TableEntry;

impl TableEntry {
    /// Returns the raw bytes of the table entry
    pub fn raw(&self) -> &[u8];
    
    /// Returns the entry flags
    pub fn flags(&self) -> &EntryFlags;
}

/// Entry Flags
pub struct EntryFlags;

impl EntryFlags {
    /// Returns true if this is a user metadata entry
    pub fn is_user(&self) -> bool;
    
    /// Returns true if this is a virtual disk metadata entry
    pub fn is_virtual_disk(&self) -> bool;
    
    /// Returns true if this entry is required
    pub fn is_required(&self) -> bool;
}

/// Metadata Items
pub struct MetadataItems;

impl MetadataItems {
    /// Returns the file parameters metadata item
    pub fn file_parameters(&self) -> Option<&FileParameters>;
    
    /// Returns the virtual disk size
    pub fn virtual_disk_size(&self) -> Option<u64>;
    
    /// Returns the virtual disk ID
    pub fn virtual_disk_id(&self) -> Option<&Guid>;
    
    /// Returns the logical sector size
    pub fn logical_sector_size(&self) -> Option<u32>;
    
    /// Returns the physical sector size
    pub fn physical_sector_size(&self) -> Option<u32>;
    
    /// Returns the parent locator (for differencing disks)
    pub fn parent_locator(&self) -> Option<&ParentLocator>;
}

/// File Parameters
pub struct FileParameters;

impl FileParameters {
    /// Returns the raw bytes of the file parameters
    pub fn raw(&self) -> &[u8];
    
    /// Returns the block size in bytes
    pub fn block_size(&self) -> u32;
    
    /// Returns true if blocks should remain allocated after being zeroed
    pub fn leave_block_allocated(&self) -> bool;
    
    /// Returns true if this is a differencing disk
    pub fn has_parent(&self) -> bool;
}

/// Parent Locator
pub struct ParentLocator;

impl ParentLocator {
    /// Returns the raw bytes of the parent locator
    pub fn raw(&self) -> &[u8];
    
    /// Returns the locator header
    pub fn header(&self) -> &LocatorHeader;
    
    /// Returns the key-value entry at the specified index
    pub fn entry(&self, index: usize) -> Option<&KeyValueEntry>;
    
    /// Returns all key-value entries
    pub fn entries(&self) -> &[KeyValueEntry];
    
    /// Returns the raw key-value data
    pub fn key_value_data(&self) -> &[u8];
}

/// Locator Header
pub struct LocatorHeader;

impl LocatorHeader {
    /// Returns the raw bytes of the locator header
    pub fn raw(&self) -> &[u8];
}

/// Key-Value Entry
pub struct KeyValueEntry;

impl KeyValueEntry {
    /// Returns the raw bytes of the key-value entry
    pub fn raw(&self) -> &[u8];
    
    /// Returns the key from the provided data buffer
    pub fn key(&self, data: &[u8]) -> Option<String>;
    
    /// Returns the value from the provided data buffer
    pub fn value(&self, data: &[u8]) -> Option<String>;
}
```

The Metadata section contains key information about the virtual disk, including its size, sector sizes, block size, and parent locator information for differencing disks. The metadata is organized in a table with entries referenced by GUIDs.

### 8. Log Section

```rust
/// Log Section
pub struct Log;

impl Log {
    /// Returns the raw bytes of the log section
    pub fn raw(&self) -> &[u8];
    
    /// Returns the log entry at the specified index
    pub fn entry(&self, index: usize) -> Option<&Entry>;
    
    /// Returns all log entries
    pub fn entries(&self) -> &[Entry];
}

/// Log Entry
pub struct Entry;

impl Entry {
    /// Returns the raw bytes of the log entry
    pub fn raw(&self) -> &[u8];
    
    /// Returns the log entry header
    pub fn header(&self) -> &LogEntryHeader;
    
    /// Returns the descriptor at the specified index
    pub fn descriptor(&self, index: usize) -> Option<&Descriptor>;
    
    /// Returns all descriptors
    pub fn descriptors(&self) -> &[Descriptor];
    
    /// Returns the data sectors
    pub fn data(&self) -> &[DataSector];
}

/// Descriptor
pub enum Descriptor {
    /// Data descriptor
    Data(DataDescriptor),
    /// Zero descriptor
    Zero(ZeroDescriptor),
}

impl Descriptor {
    /// Returns the raw bytes of the descriptor
    pub fn raw(&self) -> &[u8];
}

/// Data Descriptor
pub struct DataDescriptor;

impl DataDescriptor {
    /// Returns the raw bytes of the data descriptor
    pub fn raw(&self) -> &[u8];
}

/// Zero Descriptor
pub struct ZeroDescriptor;

impl ZeroDescriptor {
    /// Returns the raw bytes of the zero descriptor
    pub fn raw(&self) -> &[u8];
}

/// Log Entry Header
pub struct LogEntryHeader;

impl LogEntryHeader {
    /// Returns the raw bytes of the log entry header
    pub fn raw(&self) -> &[u8];
}

/// Data Sector
pub struct DataSector;

impl DataSector {
    /// Returns the raw bytes of the data sector
    pub fn raw(&self) -> &[u8];
}
```

The Log section provides journaling capabilities for crash recovery. Log entries contain descriptors that record changes to the VHDX file, allowing for transactional updates and recovery from interrupted operations.
