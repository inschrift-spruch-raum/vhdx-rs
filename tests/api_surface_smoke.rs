//! API 面 smoke 测试 — 按 docs/plan/API.md 验证所有公共类型的导入路径与最小可调用性。
//!
//! 本文件不包含深度业务断言，侧重"可导入 + 可编译 + 可调用"。
//! 每个 test case 覆盖 API.md 中的一组相关类型或调用路径。

use std::path::PathBuf;

// ────────────────────────────────────────────
// 辅助：临时 VHDX 文件路径
// ────────────────────────────────────────────

/// 生成临时 VHDX 路径，通过 `mem::forget` 阻止自动清理。
fn temp_vhdx_path() -> PathBuf {
    let dir = tempfile::tempdir().expect("Failed to create temp dir");
    let path = dir.path().join("api_smoke.vhdx");
    std::mem::forget(dir);
    path
}

/// 创建 1 MiB 固定磁盘并返回 File 实例，供后续 smoke 测试共享。
fn create_fixed_disk() -> vhdx_rs::File {
    let path = temp_vhdx_path();
    vhdx_rs::File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .finish()
        .expect("smoke: failed to create fixed disk")
}

// ════════════════════════════════════════════
// 1. 根级类型导入 smoke
// ════════════════════════════════════════════

/// 根级核心类型可导入：File / Error / Result / Guid。
#[test]
fn smoke_root_core_types_import() {
    use vhdx_rs::{Error, File, Guid, Result};

    // File 静态方法存在
    let _open_builder: File = {
        let path = temp_vhdx_path();
        vhdx_rs::File::create(&path)
            .size(1024 * 1024)
            .fixed(true)
            .finish()
            .expect("smoke create")
    };

    // Guid 可构造
    let _guid: Guid = Guid::from_bytes([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]);

    // Result 类型可使用
    let _: Result<u8> = Ok(8);

    // Error 可匹配
    let err = Error::InvalidParameter("test".to_string());
    let msg = format!("{err}");
    assert!(!msg.is_empty());
}

/// 根级 IO / Section 类型可导入。
#[test]
fn smoke_root_io_section_types_import() {
    use vhdx_rs::{IO, PayloadBlock, Sector};

    // 仅验证类型名可导入——不构造实例
    let _ = std::marker::PhantomData::<IO<'_>>;
    let _ = std::marker::PhantomData::<Sector<'_>>;
    let _ = std::marker::PhantomData::<PayloadBlock<'_>>;
}

/// 根级 OpenOptions / CreateOptions 可导入。
#[test]
fn smoke_root_options_types_import() {
    use vhdx_rs::{CreateOptions, OpenOptions};

    let _ = std::marker::PhantomData::<OpenOptions>;
    let _ = std::marker::PhantomData::<CreateOptions>;
}

/// 根级 LogReplayPolicy / ParentChainInfo 可导入且所有变体可构造。
#[test]
fn smoke_root_policy_and_chain_info_import() {
    use vhdx_rs::{LogReplayPolicy, ParentChainInfo};

    let _require = LogReplayPolicy::Require;
    let _auto = LogReplayPolicy::Auto;
    let _in_mem = LogReplayPolicy::InMemoryOnReadOnly;
    let _no_replay = LogReplayPolicy::ReadOnlyNoReplay;

    let info = ParentChainInfo {
        child: PathBuf::from("/c.vhdx"),
        parent: PathBuf::from("/p.vhdx"),
        linkage_matched: false,
    };
    assert!(!info.linkage_matched);
}

/// 根级 SpecValidator / ValidationIssue 可导入。
#[test]
fn smoke_root_validation_types_import() {
    use vhdx_rs::{SpecValidator, ValidationIssue};

    let issue = ValidationIssue {
        section: "test",
        code: "SMOKE",
        message: "smoke issue".to_string(),
        spec_ref: "MS-VHDX §1",
    };
    assert_eq!(issue.section, "test");

    let _ = std::marker::PhantomData::<SpecValidator<'_>>;
}

/// 根级辅助导出：SectionsConfig / crc32c_with_zero_field。
#[test]
fn smoke_root_helpers_import() {
    use vhdx_rs::{SectionsConfig, crc32c_with_zero_field};

    let _ = std::marker::PhantomData::<SectionsConfig>;
    // crc32c_with_zero_field 可调用
    let data = [0u8; 32];
    let _checksum: u32 = crc32c_with_zero_field(&data, 4, 4);
}

// ════════════════════════════════════════════
// 2. section 模块导入 smoke
// ════════════════════════════════════════════

/// section 模块 Header 子类型可导入。
#[test]
fn smoke_section_header_types_import() {
    use vhdx_rs::section::{
        FileTypeIdentifier, Header, HeaderStructure, RegionTable, RegionTableEntry,
        RegionTableHeader,
    };

    let _ = std::marker::PhantomData::<Header>;
    let _ = std::marker::PhantomData::<HeaderStructure<'_>>;
    let _ = std::marker::PhantomData::<FileTypeIdentifier<'_>>;
    let _ = std::marker::PhantomData::<RegionTable<'_>>;
    let _ = std::marker::PhantomData::<RegionTableHeader<'_>>;
    let _ = std::marker::PhantomData::<RegionTableEntry<'_>>;
}

/// section 模块 Bat 子类型可导入。
#[test]
fn smoke_section_bat_types_import() {
    use vhdx_rs::section::{Bat, BatEntry, BatState, PayloadBlockState, SectorBitmapState};

    let _ = std::marker::PhantomData::<Bat>;

    // BatState 枚举变体可构造
    let _payload = BatState::Payload(PayloadBlockState::NotPresent);
    let _bitmap = BatState::SectorBitmap(SectorBitmapState::NotPresent);

    // PayloadBlockState 全部变体
    let _ = PayloadBlockState::NotPresent;
    let _ = PayloadBlockState::Undefined;
    let _ = PayloadBlockState::Zero;
    let _ = PayloadBlockState::Unmapped;
    let _ = PayloadBlockState::FullyPresent;
    let _ = PayloadBlockState::PartiallyPresent;

    // SectorBitmapState 全部变体
    let _ = SectorBitmapState::NotPresent;
    let _ = SectorBitmapState::Present;

    // BatEntry 公共字段可访问
    let entry = BatEntry {
        state: BatState::Payload(PayloadBlockState::Zero),
        file_offset_mb: 42,
    };
    assert_eq!(entry.file_offset_mb, 42);
}

/// section 模块 Metadata 子类型可导入。
#[test]
fn smoke_section_metadata_types_import() {
    use vhdx_rs::section::{
        EntryFlags, FileParameters, KeyValueEntry, LocatorHeader, Metadata, MetadataItems,
        MetadataTable, ParentLocator, TableEntry, TableHeader,
    };

    let _ = std::marker::PhantomData::<Metadata>;
    let _ = std::marker::PhantomData::<MetadataTable<'_>>;
    let _ = std::marker::PhantomData::<MetadataItems<'_>>;
    let _ = std::marker::PhantomData::<FileParameters<'_>>;
    let _ = std::marker::PhantomData::<ParentLocator<'_>>;
    let _ = std::marker::PhantomData::<LocatorHeader<'_>>;
    let _ = std::marker::PhantomData::<KeyValueEntry<'_>>;
    let _ = std::marker::PhantomData::<TableHeader<'_>>;
    let _ = std::marker::PhantomData::<TableEntry<'_>>;

    // EntryFlags 可构造
    let flags = EntryFlags(0x8000_0000);
    assert!(flags.is_user());
    assert!(!flags.is_virtual_disk());
    assert!(!flags.is_required());
}

/// section 模块 Log 子类型可导入。
#[test]
fn smoke_section_log_types_import() {
    use vhdx_rs::section::{
        DataDescriptor, DataSector, Descriptor, Entry, Log, LogEntry, LogEntryHeader,
        ZeroDescriptor,
    };

    let _ = std::marker::PhantomData::<Log>;
    let _ = std::marker::PhantomData::<LogEntry<'_>>;
    let _ = std::marker::PhantomData::<Entry<'_>>;
    let _ = std::marker::PhantomData::<LogEntryHeader<'_>>;
    let _ = std::marker::PhantomData::<Descriptor<'_>>;
    let _ = std::marker::PhantomData::<DataDescriptor<'_>>;
    let _ = std::marker::PhantomData::<ZeroDescriptor<'_>>;
    let _ = std::marker::PhantomData::<DataSector<'_>>;
}

/// section 模块 Sections 容器可导入。
#[test]
fn smoke_section_sections_type_import() {
    use vhdx_rs::section::Sections;

    let _ = std::marker::PhantomData::<Sections>;
}

// ════════════════════════════════════════════
// 3. constants 命名空间导入 smoke
// ════════════════════════════════════════════

/// constants 命名空间中基本常量和函数可导入。
#[test]
fn smoke_constants_namespace_import() {
    use vhdx_rs::constants::{
        DEFAULT_BLOCK_SIZE, FILE_TYPE_SIZE, KiB, MAX_BLOCK_SIZE, MIN_BLOCK_SIZE, MiB, align_1mib,
        align_up,
    };

    assert_eq!(KiB, 1024);
    assert_eq!(MiB, 1024 * 1024);
    assert!(DEFAULT_BLOCK_SIZE >= MIN_BLOCK_SIZE);
    assert!(DEFAULT_BLOCK_SIZE <= MAX_BLOCK_SIZE);
    assert!(FILE_TYPE_SIZE > 0);
    assert_eq!(align_up(1, MiB), MiB);
    assert_eq!(align_1mib(1), MiB);
}

/// constants::region_guids 子命名空间可导入。
#[test]
fn smoke_constants_region_guids_import() {
    use vhdx_rs::constants::region_guids;

    assert!(!region_guids::BAT_REGION.is_nil());
    assert!(!region_guids::METADATA_REGION.is_nil());
}

/// constants::metadata_guids 子命名空间可导入。
#[test]
fn smoke_constants_metadata_guids_import() {
    use vhdx_rs::constants::metadata_guids;

    assert!(!metadata_guids::FILE_PARAMETERS.is_nil());
    assert!(!metadata_guids::VIRTUAL_DISK_SIZE.is_nil());
    assert!(!metadata_guids::VIRTUAL_DISK_ID.is_nil());
    assert!(!metadata_guids::LOGICAL_SECTOR_SIZE.is_nil());
    assert!(!metadata_guids::PHYSICAL_SECTOR_SIZE.is_nil());
    assert!(!metadata_guids::PARENT_LOCATOR.is_nil());
}

/// section::StandardItems 命名空间可导入且与 legacy constants 路径一致。
#[test]
fn smoke_section_standard_items_namespace_import() {
    use vhdx_rs::constants::metadata_guids;
    use vhdx_rs::section::StandardItems;

    assert_eq!(
        StandardItems::FILE_PARAMETERS,
        metadata_guids::FILE_PARAMETERS
    );
    assert_eq!(
        StandardItems::VIRTUAL_DISK_SIZE,
        metadata_guids::VIRTUAL_DISK_SIZE
    );
    assert_eq!(
        StandardItems::VIRTUAL_DISK_ID,
        metadata_guids::VIRTUAL_DISK_ID
    );
    assert_eq!(
        StandardItems::LOGICAL_SECTOR_SIZE,
        metadata_guids::LOGICAL_SECTOR_SIZE
    );
    assert_eq!(
        StandardItems::PHYSICAL_SECTOR_SIZE,
        metadata_guids::PHYSICAL_SECTOR_SIZE
    );
    assert_eq!(
        StandardItems::PARENT_LOCATOR,
        metadata_guids::PARENT_LOCATOR
    );
    assert!(!StandardItems::LOCATOR_TYPE_VHDX.is_nil());
}

// ════════════════════════════════════════════
// 4. validation 模块导入 smoke
// ════════════════════════════════════════════

/// validation 模块可作为独立 mod 路径导入。
#[test]
fn smoke_validation_mod_import() {
    use vhdx_rs::validation::{SpecValidator, ValidationIssue};

    let _ = std::marker::PhantomData::<SpecValidator<'_>>;
    let issue = ValidationIssue {
        section: "bat",
        code: "TEST",
        message: "ok".to_string(),
        spec_ref: "§2.5",
    };
    assert_eq!(issue.code, "TEST");
}

// ════════════════════════════════════════════
// 5. File::open / File::create builder 调用路径
// ════════════════════════════════════════════

/// File::open() 链式调用路径可编译：write / strict / log_replay / finish。
#[test]
fn smoke_open_builder_chain_callable() {
    use vhdx_rs::{File, LogReplayPolicy};

    let path = temp_vhdx_path();
    File::create(&path)
        .size(1024 * 1024)
        .fixed(true)
        .finish()
        .expect("smoke: create");

    // 只读默认打开
    let _ro = File::open(&path).finish().expect("smoke: open read-only");

    // 写入模式
    let _rw = File::open(&path)
        .write()
        .finish()
        .expect("smoke: open write");

    // strict + log_replay 链式
    let _strict = File::open(&path)
        .strict(true)
        .log_replay(LogReplayPolicy::Auto)
        .finish()
        .expect("smoke: open strict+auto");

    let _no_replay = File::open(&path)
        .log_replay(LogReplayPolicy::ReadOnlyNoReplay)
        .finish()
        .expect("smoke: open no-replay");
}

/// File::create() 链式调用路径可编译：size / fixed / block_size / logical_sector_size /
/// physical_sector_size / parent_path / finish。
#[test]
fn smoke_create_builder_chain_callable() {
    use vhdx_rs::File;

    // 基本创建
    let _basic = File::create(temp_vhdx_path())
        .size(1024 * 1024)
        .finish()
        .expect("smoke: create basic");

    // 全参数链式创建（不含 parent_path）
    let _full = File::create(temp_vhdx_path())
        .size(2 * 1024 * 1024)
        .fixed(true)
        .block_size(1024 * 1024)
        .logical_sector_size(4096)
        .physical_sector_size(4096)
        .finish()
        .expect("smoke: create full chain");

    // 含 parent_path 的差分盘创建
    let parent = temp_vhdx_path();
    File::create(&parent)
        .size(2 * 1024 * 1024)
        .fixed(true)
        .finish()
        .expect("smoke: create parent");

    let _diff = File::create(temp_vhdx_path())
        .size(2 * 1024 * 1024)
        .parent_path(&parent)
        .finish()
        .expect("smoke: create diff");
}

// ════════════════════════════════════════════
// 6. File 实例方法调用路径
// ════════════════════════════════════════════

/// File::sections() 返回 &Sections，header/bat/metadata/log 可调用。
#[test]
fn smoke_file_sections_methods_callable() {
    let file = create_fixed_disk();

    let sections = file.sections();

    // header()
    let header = sections.header().expect("smoke: header");
    let _fti = header.file_type();
    let _hdr0 = header.header(0);
    let _rt0 = header.region_table(0);

    // bat()
    let bat = sections.bat().expect("smoke: bat");
    assert!(!bat.is_empty());
    assert!(bat.len() > 0);
    let _e0 = bat.entry(0);
    let _entries = bat.entries();

    // metadata()
    let metadata = sections.metadata().expect("smoke: metadata");
    let _table = metadata.table();
    let _items = metadata.items();

    // log()
    let log = sections.log().expect("smoke: log");
    assert!(!log.is_replay_required());
    let _entries = log.entries();
    let _e0 = log.entry(0);
}

/// File::io() 返回 IO，sector() 可调用。
#[test]
fn smoke_file_io_sector_callable() {
    let file = create_fixed_disk();

    let io = file.io();

    // sector() 返回 Option<Sector>
    let sector = io.sector(0).expect("smoke: sector 0");
    assert_eq!(sector.block_sector_index, 0);

    // Sector 方法可调用
    let _payload = sector.payload();
    let mut buf = vec![0u8; 4096];
    let _read = sector.read(&mut buf);
}

/// File::validator() 返回 SpecValidator，所有校验方法可调用。
#[test]
fn smoke_file_validator_methods_callable() {
    let file = create_fixed_disk();

    let v = file.validator();

    // 总入口
    let _ = v.validate_file();

    // 分项
    let _ = v.validate_header();
    let _ = v.validate_region_table();
    let _ = v.validate_bat();
    let _ = v.validate_metadata();
    let _ = v.validate_required_metadata_items();
    let _ = v.validate_log();
    let _ = v.validate_parent_locator();
    let _ = v.validate_parent_chain();
}

/// File::inner() 返回底层文件句柄。
#[test]
fn smoke_file_inner_callable() {
    let file = create_fixed_disk();

    let _inner: &std::fs::File = file.inner();
}

/// File 公共 getter 方法可调用：virtual_disk_size / block_size / logical_sector_size /
/// is_fixed / has_parent / has_pending_logs。
#[test]
fn smoke_file_public_methods_callable() {
    let file = create_fixed_disk();

    let _vds = file.virtual_disk_size();
    let _bs = file.block_size();
    let _lss = file.logical_sector_size();
    let _fixed = file.is_fixed();
    let _parent = file.has_parent();
    let _pending = file.has_pending_logs();
}

// ════════════════════════════════════════════
// 7. Header 子结构公共字段可访问
// ════════════════════════════════════════════

/// HeaderStructure / RegionTableHeader / RegionTableEntry 公共字段可访问。
#[test]
fn smoke_header_substructure_fields_accessible() {
    let file = create_fixed_disk();
    let header = file.sections().header().expect("smoke: header");

    // FileTypeIdentifier 公共字段
    let fti = header.file_type();
    let _sig: [u8; 8] = fti.signature;
    let _creator: &[u8] = fti.creator;

    // HeaderStructure 公共字段
    let hdr = header.header(0).expect("smoke: header structure");
    let _sig: [u8; 4] = hdr.signature;
    let _checksum: u32 = hdr.checksum;
    let _seq: u64 = hdr.sequence_number;
    let _fwg = hdr.file_write_guid;
    let _dwg = hdr.data_write_guid;
    let _lg = hdr.log_guid;
    let _lv: u16 = hdr.log_version;
    let _ver: u16 = hdr.version;
    let _ll: u32 = hdr.log_length;
    let _lo: u64 = hdr.log_offset;

    // RegionTable + entries
    let rt = header.region_table(0).expect("smoke: region table");

    // RegionTable 公共字段可访问
    let _hdr_field: vhdx_rs::section::RegionTableHeader<'_> = rt.header;
    let _entries_field: &[vhdx_rs::section::RegionTableEntry<'_>] = &rt.entries;

    // RegionTable 方法也可调用
    let rth = rt.header();
    let _sig: [u8; 4] = rth.signature;
    let _checksum: u32 = rth.checksum;
    let _ec: u32 = rth.entry_count;

    if let Some(rte) = rt.entries().first() {
        let _guid = rte.guid;
        let _offset = rte.file_offset;
        let _length = rte.length;
        let _required = rte.required;
    }
}

// ════════════════════════════════════════════
// 8. Metadata 子结构调用路径
// ════════════════════════════════════════════

/// MetadataTable / TableHeader / TableEntry / EntryFlags / MetadataItems /
/// FileParameters / ParentLocator 调用路径。
#[test]
fn smoke_metadata_substructure_callable() {
    let file = create_fixed_disk();
    let metadata = file.sections().metadata().expect("smoke: metadata");

    // table()
    let table = metadata.table();
    let th = table.header();
    let _sig: [u8; 8] = th.signature;
    let _ec: u16 = th.entry_count;

    let table_entries = table.entries();
    if let Some(te) = table_entries.first() {
        let _item_id = te.item_id;
        let _offset = te.offset;
        let _length = te.length;
        let _flags_val = te.flags;
        let flags = te.flags();
        let _ = flags.is_user();
        let _ = flags.is_virtual_disk();
        let _ = flags.is_required();
    }

    // items()
    let items = metadata.items();
    let _vds = items.virtual_disk_size();
    let _vdi = items.virtual_disk_id();
    let _lss = items.logical_sector_size();
    let _pss = items.physical_sector_size();

    if let Some(fp) = items.file_parameters() {
        let _bs = fp.block_size();
        let _lba = fp.leave_block_allocated();
        let _hp = fp.has_parent();
    }

    // parent_locator 对非差分盘应为 None
    let _pl = items.parent_locator();
}

// ════════════════════════════════════════════
// 9. BAT 子结构调用路径
// ════════════════════════════════════════════

/// BatEntry / BatState 枚举可匹配。
#[test]
fn smoke_bat_entry_state_pattern_matchable() {
    use vhdx_rs::section::{BatState, PayloadBlockState, SectorBitmapState};

    let file = create_fixed_disk();
    let bat = file.sections().bat().expect("smoke: bat");

    for entry in bat.entries() {
        match entry.state {
            BatState::Payload(s) => match s {
                PayloadBlockState::NotPresent
                | PayloadBlockState::Undefined
                | PayloadBlockState::Zero
                | PayloadBlockState::Unmapped
                | PayloadBlockState::FullyPresent
                | PayloadBlockState::PartiallyPresent => {}
            },
            BatState::SectorBitmap(s) => match s {
                SectorBitmapState::NotPresent | SectorBitmapState::Present => {}
            },
        }
        let _offset = entry.file_offset_mb;
    }
}

// ════════════════════════════════════════════
// 10. Log 子结构调用路径
// ════════════════════════════════════════════

/// LogEntry / LogEntryHeader / Descriptor 枚举可构造和匹配。
#[test]
fn smoke_log_entry_types_callable() {
    use vhdx_rs::section::{Descriptor, LogEntry};

    // LogEntry 最小构造
    let mut data = vec![0u8; 64];
    data[0..4].copy_from_slice(b"loge");
    data[8..12].copy_from_slice(&64u32.to_le_bytes());
    let entry = LogEntry::new(&data).expect("smoke: LogEntry::new");

    let header = entry.header();
    let _sig: &[u8; 4] = header.signature();
    let _seq: u64 = header.sequence_number();

    // Descriptor 枚举可匹配
    for desc in entry.descriptors() {
        match desc {
            Descriptor::Data(dd) => {
                let _sig: [u8; 4] = dd.signature;
                let _tb: u32 = dd.trailing_bytes;
                let _lb: u64 = dd.leading_bytes;
                let _fo: u64 = dd.file_offset;
                let _sn: u64 = dd.sequence_number;
            }
            Descriptor::Zero(zd) => {
                let _sig: [u8; 4] = zd.signature;
                let _zl: u64 = zd.zero_length;
                let _fo: u64 = zd.file_offset;
                let _sn: u64 = zd.sequence_number;
            }
        }
    }

    // DataSector 可访问
    for ds in entry.data() {
        let _sig: [u8; 4] = ds.signature;
        let _sh: u32 = ds.sequence_high;
        let _data: &[u8] = ds.data;
        let _sl: u32 = ds.sequence_low;
    }
}

// ════════════════════════════════════════════
// 11. IO / Sector / PayloadBlock 调用路径
// ════════════════════════════════════════════

/// IO::sector -> Sector -> payload() / read() / write() 调用路径。
#[test]
fn smoke_io_sector_payload_chain() {
    use vhdx_rs::PayloadBlock;

    let file = create_fixed_disk();
    let io = file.io();

    let sector = io.sector(0).expect("smoke: sector 0");

    // Sector 公共字段
    let _idx: u32 = sector.block_sector_index;

    // payload 公共字段
    let pb: &PayloadBlock<'_> = &sector.payload;
    let _bytes: &[u8] = pb.bytes;

    // payload() 方法
    let _via_method = sector.payload();

    // read
    let mut buf = vec![0u8; 4096];
    let _n = sector.read(&mut buf);

    // Sector Clone/Debug/PartialEq
    let cloned = sector.clone();
    assert_eq!(sector, cloned);
    let _debug = format!("{sector:?}");
}

/// PayloadBlock 可手动构造且 PartialEq 可用。
#[test]
fn smoke_payload_block_manual_construction() {
    use vhdx_rs::PayloadBlock;

    let data = b"hello";
    let pb1 = PayloadBlock { bytes: data };
    let pb2 = PayloadBlock { bytes: data };
    assert_eq!(pb1, pb2);

    let pb3 = PayloadBlock { bytes: b"world" };
    assert_ne!(pb1, pb3);

    let _debug = format!("{pb1:?}");
    let _cloned = pb1.clone();
}

// ════════════════════════════════════════════
// 12. ParentLocator / KeyValueEntry 完整调用路径
// ════════════════════════════════════════════

/// ParentLocator 全部 5 个 API.md 方法可调用：header / entry / entries /
/// key_value_data / resolve_parent_path。
#[test]
fn smoke_parent_locator_full_api_callable() {
    use vhdx_rs::section::{KeyValueEntry, LocatorHeader, ParentLocator};

    // 构造含 1 个 entry 的 ParentLocator
    fn utf16(s: &str) -> Vec<u8> {
        s.encode_utf16().flat_map(|c| c.to_le_bytes()).collect()
    }

    let key = "relative_path";
    let val = "../parent.vhdx";
    let key_data = utf16(key);
    let val_data = utf16(val);

    let mut kv_region = Vec::new();
    let k_off = 0u32;
    kv_region.extend_from_slice(&key_data);
    let v_off = kv_region.len() as u32;
    kv_region.extend_from_slice(&val_data);

    let mut buf = vec![0u8; 32 + kv_region.len()];
    buf[18..20].copy_from_slice(&(1u16).to_le_bytes());
    buf[20..24].copy_from_slice(&k_off.to_le_bytes());
    buf[24..28].copy_from_slice(&v_off.to_le_bytes());
    buf[28..30].copy_from_slice(&(key_data.len() as u16).to_le_bytes());
    buf[30..32].copy_from_slice(&(val_data.len() as u16).to_le_bytes());
    buf[32..].copy_from_slice(&kv_region);

    let locator = ParentLocator::new(&buf).expect("smoke: ParentLocator::new");

    // 5 个 API.md 方法
    let _header: LocatorHeader<'_> = locator.header();
    let _e0: Option<KeyValueEntry<'_>> = locator.entry(0);
    let _entries: Vec<KeyValueEntry<'_>> = locator.entries();
    let _kvd: &[u8] = locator.key_value_data();
    let _resolved = locator.resolve_parent_path();
}

// ════════════════════════════════════════════
// 13. Guid 类型基础操作
// ════════════════════════════════════════════

/// Guid 可构造、比较、判空。
#[test]
fn smoke_guid_basic_operations() {
    use vhdx_rs::Guid;

    let g1 = Guid::from_bytes([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]);
    let g2 = Guid::from_bytes([16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1]);
    assert_ne!(g1, g2);
    assert!(!g1.is_nil());
}

// ════════════════════════════════════════════
// 14. Error 变体覆盖
// ════════════════════════════════════════════

/// Error 全部公共变体可构造（按 API.md 列表）。
#[test]
fn smoke_error_variants_constructible() {
    use std::path::PathBuf;
    use vhdx_rs::{Error, Guid};

    let _ = Error::Io(std::io::Error::new(std::io::ErrorKind::Other, "test"));
    let _ = Error::InvalidFile("test".to_string());
    let _ = Error::CorruptedHeader("test".to_string());
    let _ = Error::InvalidChecksum {
        expected: 1,
        actual: 2,
    };
    let _ = Error::UnsupportedVersion(99);
    let _ = Error::InvalidBlockState(255);
    let _ = Error::ParentNotFound {
        path: PathBuf::from("/x.vhdx"),
    };
    let _ = Error::ParentMismatch {
        expected: Guid::from_bytes([1; 16]),
        actual: Guid::from_bytes([2; 16]),
    };
    let _ = Error::LogReplayRequired;
    let _ = Error::InvalidParameter("test".to_string());
    let _ = Error::MetadataNotFound {
        guid: Guid::from_bytes([3; 16]),
    };
    let _ = Error::ReadOnly;
}
