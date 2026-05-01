//! VHDX 文件操作核心模块
//!
//! 本模块提供 VHDX 虚拟硬盘文件的顶层操作接口，包括：
//! - **打开**（[`File::open`]）— 验证签名、解析头部、读取元数据、处理日志回放
//! - **创建**（[`File::create`]）— 计算布局、写入所有结构、返回可操作的文件句柄
//! - **读取**（[`File::read`]）— 支持 Fixed 和 Dynamic 两种类型
//! - **写入**（[`File::write`]）— 支持 Fixed 直接写入和 Dynamic 块级写入
//!
//! # VHDX 文件类型（MS-VHDX §1.3）
//!
//! - **Fixed** — 固定大小，虚拟磁盘数据连续存储，性能最佳
//! - **Dynamic** — 动态分配，按需分配数据块，节省空间
//! - **Differencing** — 差分磁盘，引用父磁盘，支持快照
//!
//! # 使用示例
//!
//! ```no_run
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     use vhdx_rs::File;
//!
//!     // 打开 VHDX 文件
//!     let _file = File::open("disk.vhdx").finish()?;
//!
//!     // 创建新的 VHDX 文件
//!     let _file = File::create("new.vhdx")
//!         .size(10 * 1024 * 1024 * 1024)  // 10GB
//!         .fixed(true)
//!         .finish()?;
//!
//!     Ok(())
//! }
//! ```

use std::fs::{File as StdFile, OpenOptions as StdOpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;

use crate::common::constants::{
    BAT_ENTRY_SIZE, DEFAULT_BLOCK_SIZE, FILE_TYPE_SIGNATURE, FILE_TYPE_SIZE, HEADER_1_OFFSET,
    HEADER_2_OFFSET, HEADER_SECTION_SIZE, LOGICAL_SECTOR_SIZE_512, MAX_BLOCK_SIZE,
    METADATA_SIGNATURE, METADATA_TABLE_SIZE, MIN_BLOCK_SIZE, MiB, REGION_TABLE_1_OFFSET,
    REGION_TABLE_2_OFFSET, REGION_TABLE_SIGNATURE, REGION_TABLE_SIZE, align_1mib,
};
use crate::common::region_guids;
use crate::error::{Error, Result};
use crate::io_module::IO;
use crate::sections::Bat;
use crate::sections::{
    BatEntry, BatState, FileTypeIdentifier, Header, HeaderStructure, PayloadBlockState, Sections,
    SectionsConfig, SectorBitmapState,
};
use crate::types::Guid;

/// VHDX 虚拟硬盘文件句柄
///
/// 提供对 VHDX 文件的完整操作能力，包括读取和写入虚拟磁盘数据。
///
/// # 字段说明
///
/// - `inner` — 底层操作系统文件句柄
/// - `sections` — 各 VHDX 区域的延迟加载容器
/// - `virtual_disk_size` — 虚拟磁盘大小（字节）
/// - `block_size` — 块大小（字节），用于 Dynamic 类型
/// - `logical_sector_size` — 逻辑扇区大小（512 或 4096）
/// - `is_fixed` — 是否为 Fixed 类型（`leave_block_allocated` 标志）
/// - `has_parent` — 是否为差分磁盘（有父磁盘引用）
/// - `has_pending_logs` — 是否存在未回放的日志条目
/// - `writable` — 是否以写入模式打开
pub struct File {
    /// 底层操作系统文件句柄
    inner: StdFile,
    /// 各 VHDX 区域的延迟加载容器
    sections: Sections<'static>,
    /// 虚拟磁盘大小（字节）
    virtual_disk_size: u64,
    /// 块大小（字节），用于 Dynamic 类型
    block_size: u32,
    /// 逻辑扇区大小（512 或 4096）
    logical_sector_size: u32,
    /// 是否为 Fixed 类型（`leave_block_allocated` 标志）
    is_fixed: bool,
    /// 是否为差分磁盘（有父磁盘引用）
    has_parent: bool,
    /// 是否存在未回放的日志条目
    #[allow(dead_code)]
    has_pending_logs: bool,
    /// 是否以写入模式打开
    writable: bool,
    /// 打开该文件时使用的路径
    opened_path: PathBuf,
    /// 只读内存回放覆盖层（按文件绝对偏移覆盖读取结果）
    replay_overlay: Option<ReplayOverlay>,
}

/// 只读内存回放覆盖层
///
/// 当以 `InMemoryOnReadOnly` 策略打开且检测到待回放日志时，
/// 将 replay 结果记录到内存，并在读取时按绝对文件偏移覆盖返回数据。
struct ReplayOverlay {
    /// 按日志顺序收集的写入片段
    writes: Vec<ReplayWrite>,
}

/// 单个覆盖写入片段
struct ReplayWrite {
    /// 文件绝对偏移
    file_offset: u64,
    /// 写入字节内容
    data: Vec<u8>,
}

/// 日志回放策略
///
/// 控制 `File::open(...).finish()` 在检测到未回放日志时的处理方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogReplayPolicy {
    /// 若存在未回放日志则直接返回 `Error::LogReplayRequired`
    Require,
    /// 自动回放日志
    ///
    /// 只读打开时会执行内存回放（不写回磁盘），与 `InMemoryOnReadOnly` 的只读行为一致。
    Auto,
    /// 只读打开时允许以内存方式回放（不写回磁盘）
    InMemoryOnReadOnly,
    /// 只读打开且不回放日志
    ///
    /// 约束：仅允许结构读取（Header/Region/Metadata 等），
    /// 不保证 payload 数据面的一致性读取。
    ReadOnlyNoReplay,
}

/// 差分链校验结果
///
/// 包含当前子盘与解析出的父盘路径，以及父盘 GUID 一致性校验结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParentChainInfo {
    /// 当前子盘路径
    child: PathBuf,
    /// 解析出的父盘路径
    parent: PathBuf,
    /// 是否匹配 `parent_linkage` / `parent_linkage2`
    linkage_matched: bool,
}

impl ParentChainInfo {
    /// 创建差分链校验结果
    ///
    /// # 参数
    ///
    /// - `child` — 当前子盘路径
    /// - `parent` — 解析出的父盘路径
    /// - `linkage_matched` — 是否匹配 `parent_linkage` / `parent_linkage2`
    #[must_use]
    pub fn new(child: PathBuf, parent: PathBuf, linkage_matched: bool) -> Self {
        Self {
            child,
            parent,
            linkage_matched,
        }
    }

    /// 返回当前子盘路径
    #[must_use]
    pub fn child(&self) -> &Path {
        &self.child
    }

    /// 返回解析出的父盘路径
    #[must_use]
    pub fn parent(&self) -> &Path {
        &self.parent
    }

    /// 返回父盘 GUID 一致性校验结果
    #[must_use]
    pub const fn linkage_matched(&self) -> bool {
        self.linkage_matched
    }
}

impl File {
    /// 创建打开选项构建器，用于打开现有 VHDX 文件
    pub fn open(path: impl AsRef<Path>) -> OpenOptions {
        OpenOptions {
            path: path.as_ref().to_path_buf(),
            write: false,
            strict: true,
            log_replay: LogReplayPolicy::Require,
        }
    }

    /// 创建选项构建器，用于创建新的 VHDX 文件
    pub fn create(path: impl AsRef<Path>) -> CreateOptions {
        CreateOptions {
            path: path.as_ref().to_path_buf(),
            size: None,
            fixed: false,
            has_parent: false,
            block_size: DEFAULT_BLOCK_SIZE,
            logical_sector_size: 4096,
            physical_sector_size: 4096,
            parent_path: None,
        }
    }

    /// 获取 VHDX 区域容器的引用
    pub const fn sections(&self) -> &Sections<'_> {
        &self.sections
    }

    /// 获取扇区/块级 IO 操作接口
    pub const fn io(&self) -> IO<'_> {
        IO::new(self)
    }

    /// 获取底层操作系统文件句柄的引用
    pub const fn inner(&self) -> &StdFile {
        &self.inner
    }

    /// 获取规范校验器（只读）
    ///
    /// 校验逻辑独立在 [`validation`](crate::validation) 模块中，
    /// 避免与 File 的打开/创建职责耦合。
    pub fn validator(&self) -> crate::validation::SpecValidator<'_> {
        crate::validation::SpecValidator::new(self)
    }

    /// 获取虚拟磁盘大小（字节）
    pub(crate) const fn virtual_disk_size(&self) -> u64 {
        self.virtual_disk_size
    }

    /// 获取块大小（字节）
    pub(crate) const fn block_size(&self) -> u32 {
        self.block_size
    }

    /// 获取逻辑扇区大小（字节）
    pub(crate) const fn logical_sector_size(&self) -> u32 {
        self.logical_sector_size
    }

    /// 检查是否为 Fixed 类型
    pub(crate) const fn is_fixed(&self) -> bool {
        self.is_fixed
    }

    /// 检查是否为差分磁盘
    pub(crate) const fn has_parent(&self) -> bool {
        self.has_parent
    }

    /// 检查当前文件是否以写入模式打开
    #[must_use]
    pub const fn is_writable(&self) -> bool {
        self.writable
    }

    /// 更新差分盘父定位器中的 `relative_path` 路径
    ///
    /// 仅对以写入模式打开的差分 VHDX 文件有效。
    /// 保留父定位器中所有其他键值对不变，仅修改 `relative_path` 键的值。
    /// 若当前不存在 `relative_path` 键，则新增该条目。
    ///
    /// # 错误
    ///
    /// - `Error::ReadOnly` — 文件以只读模式打开
    /// - `Error::InvalidParameter` — 非差分磁盘
    /// - `Error::InvalidMetadata` — 缺少父定位器元数据或重建失败
    pub fn update_parent_locator_path(&self, new_relative_path: &str) -> Result<()> {
        use crate::common::constants::metadata_guids;

        // 守卫：必须以写入模式打开
        if !self.writable {
            return Err(Error::ReadOnly);
        }

        // 守卫：必须为差分磁盘
        if !self.has_parent {
            return Err(Error::InvalidParameter(
                "update_parent_locator_path requires a differencing disk".to_string(),
            ));
        }

        // 在第一阶段提取所有需要的数据，控制 RefCell 借用范围
        let (new_payload, new_length, data_offset_in_region, metadata_file_offset, entry_index) = {
            let metadata = self.sections.metadata()?;
            let items = metadata.items();
            let locator = items.parent_locator().ok_or_else(|| {
                Error::InvalidMetadata(
                    "Missing required metadata item: parent_locator".to_string(),
                )
            })?;

            let new_payload = locator.rebuild_payload_with_path(new_relative_path)?;
            let new_length = u32::try_from(new_payload.len()).unwrap_or(u32::MAX);

            let table = metadata.table();
            let parent_locator_entry = table.entry(&metadata_guids::PARENT_LOCATOR).ok_or_else(
                || Error::InvalidMetadata("Parent locator table entry not found".to_string()),
            )?;
            let data_offset = u64::from(parent_locator_entry.offset());

            let entries = table.entries();
            let idx = entries
                .iter()
                .position(|e| e.item_id() == metadata_guids::PARENT_LOCATOR)
                .ok_or_else(|| {
                    Error::InvalidMetadata(
                        "Parent locator table entry index not found".to_string(),
                    )
                })?;

            let meta_file_offset = self.sections.metadata_disk_offset();

            (
                new_payload,
                new_length,
                data_offset,
                meta_file_offset,
                idx,
            )
        }; // 此处释放 metadata / items / locator / table 的所有 RefCell 借用

        // 第二阶段：写入新负载到元数据数据区域
        {
            let mut file = self.inner.try_clone()?;
            file.seek(SeekFrom::Start(
                metadata_file_offset + data_offset_in_region,
            ))?;
            file.write_all(&new_payload)?;
        }

        // 第三阶段：更新元数据表项中的长度字段
        {
            // 表项在文件中的字节偏移：表头 32 字节 + entry_index * 32
            // 长度字段位于表项偏移 + 20 处（4 字节）
            let entry_file_offset = metadata_file_offset
                + 32u64
                + u64::try_from(entry_index).unwrap_or(u64::MAX) * 32u64;
            let length_field_offset = entry_file_offset + 20u64;

            let mut file = self.inner.try_clone()?;
            file.seek(SeekFrom::Start(length_field_offset))?;
            file.write_all(&new_length.to_le_bytes())?;
            file.sync_all()?;
        }

        // 失效缓存，使后续读取反映更新后的数据
        self.sections.invalidate_caches();

        Ok(())
    }

    /// 检查是否存在未回放的日志条目
    #[allow(dead_code)]
    pub(crate) const fn has_pending_logs(&self) -> bool {
        self.has_pending_logs
    }

    /// 获取打开该文件时使用的路径（crate 内部）
    pub(crate) fn opened_path(&self) -> &Path {
        &self.opened_path
    }

    /// 打开当前差分盘的父盘文件用于回退读取。
    ///
    /// 父路径解析顺序严格复用 `ParentLocator::resolve_parent_path`
    /// （`relative_path` → `volume_path` → `absolute_win32_path`）。
    fn open_parent_for_read(&self) -> Result<File> {
        let metadata = self.sections.metadata()?;
        let items = metadata.items();
        let locator = items.parent_locator().ok_or_else(|| {
            Error::InvalidMetadata("Missing required metadata item: parent_locator".to_string())
        })?;

        let parent_path = locator
            .resolve_parent_path()
            .ok_or_else(|| Error::ParentNotFound {
                path: std::path::PathBuf::new(),
            })?;

        if !parent_path.exists() {
            return Err(Error::ParentNotFound { path: parent_path });
        }

        File::open(&parent_path).finish()
    }

    /// 显式更新差分盘父定位器中的陈旧路径。
    ///
    /// 该方法仅在可写句柄上可用；只读句柄会返回 `Error::ReadOnly`。
    /// 调用方应先完成父链校验，再显式调用本方法执行回写。
    pub fn update_stale_parent_paths(&self, parent_path: impl AsRef<Path>) -> Result<()> {
        if !self.writable {
            return Err(Error::ReadOnly);
        }
        if !self.has_parent {
            return Err(Error::InvalidParameter(
                "update_stale_parent_paths requires a differencing disk".to_string(),
            ));
        }

        let parent_path = parent_path.as_ref();
        if !parent_path.exists() {
            return Err(Error::ParentNotFound {
                path: parent_path.to_path_buf(),
            });
        }

        let parent_file = File::open(parent_path).finish()?;
        let parent_sections_header = parent_file.sections().header()?;
        let parent_header = parent_sections_header
            .header(0)
            .ok_or_else(|| Error::CorruptedHeader("No valid header found".to_string()))?;
        let parent_linkage = parent_header.data_write_guid();

        let (updated_payload, locator_data_offset, existing_len) = {
            let metadata = self.sections.metadata()?;
            let items = metadata.items();
            let locator = items.parent_locator().ok_or_else(|| {
                Error::InvalidMetadata("Missing required metadata item: parent_locator".to_string())
            })?;

            let updated_payload = locator.rebuild_with_parent_path(parent_path, parent_linkage)?;
            let table = metadata.table();
            let locator_entry = table
                .entry(&crate::common::constants::metadata_guids::PARENT_LOCATOR)
                .ok_or(Error::MetadataNotFound {
                    guid: crate::common::constants::metadata_guids::PARENT_LOCATOR,
                })?;

            (
                updated_payload,
                u64::from(locator_entry.offset()),
                usize::try_from(locator_entry.length()).unwrap_or(0),
            )
        };

        if updated_payload.len() > existing_len {
            return Err(Error::InvalidParameter(format!(
                "Updated parent locator payload {} exceeds existing size {}",
                updated_payload.len(),
                existing_len
            )));
        }

        let metadata_disk_offset = self.sections.metadata_disk_offset();
        let mut file = self.inner.try_clone()?;
        file.seek(SeekFrom::Start(metadata_disk_offset + locator_data_offset))?;
        file.write_all(&updated_payload)?;
        if updated_payload.len() < existing_len {
            file.write_all(&vec![0u8; existing_len - updated_payload.len()])?;
        }
        file.sync_all()?;

        self.sections.invalidate_caches();
        Ok(())
    }

    /// 从父链读取指定虚拟偏移范围（按需懒加载父盘句柄）。
    fn read_from_parent_chain_cached(
        &self, parent_cache: &mut Option<File>, virtual_offset: u64, buf: &mut [u8],
    ) -> Result<()> {
        if !self.has_parent {
            return Ok(());
        }

        if parent_cache.is_none() {
            *parent_cache = Some(self.open_parent_for_read()?);
        }

        let parent = parent_cache.as_ref().ok_or_else(|| Error::ParentNotFound {
            path: std::path::PathBuf::new(),
        })?;

        let bytes_read = parent.read_raw(virtual_offset, buf)?;
        if bytes_read < buf.len() {
            buf[bytes_read..].fill(0);
        }

        Ok(())
    }

    /// 内部读取实现（按虚拟偏移读取）
    fn read(&self, offset: u64, buf: &mut [u8]) -> Result<usize> {
        if offset >= self.virtual_disk_size {
            return Ok(0);
        }

        // 计算本次可读取的字节数，不超过虚拟磁盘剩余空间和缓冲区大小
        let bytes_to_read = usize::try_from(std::cmp::min(
            u64::try_from(buf.len()).unwrap_or(u64::MAX),
            self.virtual_disk_size - offset,
        ))
        .unwrap_or(usize::MAX);

        if self.is_fixed {
            // Fixed 类型：计算文件内偏移（跳过头部区域），直接从文件读取
            let header_size = u64::try_from(HEADER_SECTION_SIZE).unwrap_or(0);
            let file_offset = header_size + offset;

            let mut file = self.inner.try_clone()?;
            file.seek(SeekFrom::Start(file_offset))?;
            let bytes_read = file.read(buf)?;
            if let Some(overlay) = &self.replay_overlay {
                Self::apply_replay_overlay(overlay, file_offset, &mut buf[..bytes_read]);
            }
            Ok(bytes_read)
        } else {
            // Dynamic 类型：按 BAT 语义读取
            // - FullyPresent/Undefined/PartiallyPresent 且 file_offset 非零：从 payload 读取
            // - NotPresent/Zero/Unmapped/无条目/无效偏移：返回零
            //
            // BAT 按 chunk 交错排列（MS-VHDX §2.5.1）：每 chunk_ratio 个 payload 条目后
            // 插入 1 个 sector bitmap 条目。因此 payload block N 对应 BAT 索引 =
            // N + floor(N / chunk_ratio)，与写入路径保持一致。
            let block_size = u64::from(self.block_size);
            let chunk_ratio = u64::from(Bat::calculate_chunk_ratio(
                self.logical_sector_size,
                self.block_size,
            ));
            let mut file = self.inner.try_clone()?;
            let mut parent_cache: Option<File> = None;

            let mut processed = 0usize;
            while processed < bytes_to_read {
                let current_virtual_offset =
                    offset + u64::try_from(processed).unwrap_or(u64::MAX - offset);
                let block_idx = current_virtual_offset / block_size;
                let block_offset = current_virtual_offset % block_size;

                // 计算 payload BAT 索引（MS-VHDX §2.5.1 chunk 交错规则）
                let bat_payload_index = if chunk_ratio > 0 {
                    block_idx + (block_idx / chunk_ratio)
                } else {
                    block_idx
                };

                let remaining = bytes_to_read - processed;
                let bytes_in_block =
                    usize::try_from(block_size - block_offset).unwrap_or(remaining);
                let chunk_len = remaining.min(bytes_in_block);

                let dst = &mut buf[processed..processed + chunk_len];
                dst.fill(0);

                // 查询 BAT 条目并根据状态决定读取策略
                let bat_entry = self.sections.bat()?.entry(bat_payload_index);
                match bat_entry {
                    Some(entry) => match entry.state() {
                        BatState::Payload(state) => match state {
                            PayloadBlockState::FullyPresent | PayloadBlockState::Undefined
                                if entry.file_offset() > 0 =>
                            {
                                // FullyPresent / Undefined：从 payload 直接读取整个范围
                                let file_offset = entry.file_offset() + block_offset;
                                file.seek(SeekFrom::Start(file_offset))?;
                                file.read_exact(dst)?;
                                if let Some(overlay) = &self.replay_overlay {
                                    Self::apply_replay_overlay(overlay, file_offset, dst);
                                }
                            }
                            PayloadBlockState::PartiallyPresent if entry.file_offset() > 0 => {
                                // PartiallyPresent：基于扇区位图逐扇区判定读取（MS-VHDX §2.5.1）
                                let sector_size = u64::from(self.logical_sector_size);
                                let sectors_per_block = block_size / sector_size;

                                // 计算 sector bitmap BAT 索引
                                let chunk_number = if chunk_ratio > 0 {
                                    block_idx / chunk_ratio
                                } else {
                                    0
                                };
                                let block_within_chunk = block_idx - chunk_number * chunk_ratio;
                                let payload_blocks_count = Bat::calculate_payload_blocks(
                                    self.virtual_disk_size,
                                    self.block_size,
                                );
                                let payload_start = chunk_number * chunk_ratio;
                                let remaining = payload_blocks_count.saturating_sub(payload_start);
                                let payload_in_chunk = remaining.min(chunk_ratio);
                                let bitmap_bat_index =
                                    chunk_number * (chunk_ratio + 1) + payload_in_chunk;

                                // 读取位图数据（若位图存在且为 Present）
                                let bitmap_offset =
                                    self.sections.bat()?.entry(bitmap_bat_index).and_then(|be| {
                                        if matches!(
                                            be.state(),
                                            BatState::SectorBitmap(SectorBitmapState::Present)
                                        ) && be.file_offset() > 0
                                        {
                                            Some(be.file_offset())
                                        } else {
                                            None
                                        }
                                    });

                                if let Some(bm_offset) = bitmap_offset {
                                    // 读取当前块对应的位图字节范围
                                    let bitmap_bit_start = block_within_chunk * sectors_per_block;
                                    let bitmap_byte_start = bm_offset + bitmap_bit_start / 8;
                                    let bitmap_byte_len =
                                        usize::try_from(sectors_per_block.div_ceil(8)).unwrap_or(0);

                                    let mut bitmap_data = vec![0u8; bitmap_byte_len];
                                    file.seek(SeekFrom::Start(bitmap_byte_start))?;
                                    let _ = file.read(&mut bitmap_data);

                                    // 逐扇区判定读取范围
                                    let first_sec = block_offset / sector_size;
                                    let bytes_to_cover =
                                        u64::try_from(chunk_len).unwrap_or(u64::MAX);
                                    let last_sec = (block_offset
                                        + bytes_to_cover.saturating_sub(1))
                                        / sector_size;

                                    for sec in first_sec
                                        ..=last_sec.min(sectors_per_block.saturating_sub(1))
                                    {
                                        let byte_idx = usize::try_from(sec / 8).unwrap_or(0);
                                        let bit_in_byte = (sec % 8) as u8;

                                        let is_present = byte_idx < bitmap_data.len()
                                            && (bitmap_data[byte_idx] & (1 << bit_in_byte)) != 0;

                                        if is_present {
                                            // 从子盘 payload 读取此扇区的重叠部分
                                            let sec_start = sec * sector_size;
                                            let sec_end = (sec + 1) * sector_size;
                                            let overlap_start = sec_start.max(block_offset);
                                            let overlap_end =
                                                sec_end.min(block_offset + bytes_to_cover);

                                            if overlap_start < overlap_end {
                                                let dst_start =
                                                    usize::try_from(overlap_start - block_offset)
                                                        .unwrap_or(0);
                                                let dst_end =
                                                    usize::try_from(overlap_end - block_offset)
                                                        .unwrap_or(chunk_len)
                                                        .min(chunk_len);

                                                if dst_start < dst_end && dst_end <= dst.len() {
                                                    let sec_file_offset =
                                                        entry.file_offset() + overlap_start;
                                                    file.seek(SeekFrom::Start(sec_file_offset))?;
                                                    file.read_exact(&mut dst[dst_start..dst_end])?;
                                                    if let Some(overlay) = &self.replay_overlay {
                                                        Self::apply_replay_overlay(
                                                            overlay,
                                                            sec_file_offset,
                                                            &mut dst[dst_start..dst_end],
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                        // bitmap=0：差分盘回退到父链读取
                                        else if self.has_parent {
                                            let sec_start = sec * sector_size;
                                            let sec_end = (sec + 1) * sector_size;
                                            let overlap_start = sec_start.max(block_offset);
                                            let overlap_end =
                                                sec_end.min(block_offset + bytes_to_cover);

                                            if overlap_start < overlap_end {
                                                let dst_start =
                                                    usize::try_from(overlap_start - block_offset)
                                                        .unwrap_or(0);
                                                let dst_end =
                                                    usize::try_from(overlap_end - block_offset)
                                                        .unwrap_or(chunk_len)
                                                        .min(chunk_len);

                                                if dst_start < dst_end && dst_end <= dst.len() {
                                                    let parent_virtual_offset =
                                                        current_virtual_offset
                                                            + u64::try_from(dst_start).unwrap_or(0);
                                                    self.read_from_parent_chain_cached(
                                                        &mut parent_cache,
                                                        parent_virtual_offset,
                                                        &mut dst[dst_start..dst_end],
                                                    )?;
                                                }
                                            }
                                        }
                                    }
                                } else if self.has_parent {
                                    // 位图缺失：整个范围回退父链
                                    self.read_from_parent_chain_cached(
                                        &mut parent_cache,
                                        current_virtual_offset,
                                        dst,
                                    )?;
                                }
                            }
                            _ => {
                                // NotPresent / Zero / Unmapped / 无效偏移：差分盘回退父链
                                self.read_from_parent_chain_cached(
                                    &mut parent_cache,
                                    current_virtual_offset,
                                    dst,
                                )?;
                            }
                        },
                        BatState::SectorBitmap(_) => {
                            // SectorBitmap 条目不应被当作 payload 读取
                        }
                    },
                    None => {
                        // BAT 条目不存在（越界）：差分盘回退父链
                        self.read_from_parent_chain_cached(
                            &mut parent_cache,
                            current_virtual_offset,
                            dst,
                        )?;
                    }
                }

                processed += chunk_len;
            }

            Ok(bytes_to_read)
        }
    }

    /// 内部使用的原始读取方法（公共 `read` 的底层实现）
    pub(crate) fn read_raw(&self, offset: u64, buf: &mut [u8]) -> Result<usize> {
        self.read(offset, buf)
    }

    /// 内部使用的原始写入方法（通过克隆文件句柄实现 &self 写入）
    ///
    /// Fixed 类型直接写入，Dynamic 类型按块写入。
    pub(crate) fn write_raw(&self, offset: u64, data: &[u8]) -> Result<usize> {
        if offset >= self.virtual_disk_size {
            return Err(Error::InvalidParameter(format!(
                "Write offset {} exceeds virtual disk size {}",
                offset, self.virtual_disk_size
            )));
        }

        // 计算本次可写入的字节数，不超过虚拟磁盘剩余空间
        let bytes_to_write = usize::try_from(std::cmp::min(
            u64::try_from(data.len()).unwrap_or(u64::MAX),
            self.virtual_disk_size - offset,
        ))
        .unwrap_or(usize::MAX);

        if self.is_fixed {
            // Fixed 类型：计算文件内偏移（跳过头部区域），通过克隆句柄写入
            let header_size = u64::try_from(HEADER_SECTION_SIZE).unwrap_or(0);
            let file_offset = header_size + offset;

            let mut file = self.inner.try_clone()?;
            file.seek(SeekFrom::Start(file_offset))?;
            file.write_all(&data[..bytes_to_write])?;
            Ok(bytes_to_write)
        } else {
            // Dynamic 类型：委托给 write_dynamic 进行按块写入
            self.write_dynamic(offset, &data[..bytes_to_write])?;
            Ok(bytes_to_write)
        }
    }

    /// Dynamic 类型的按块写入实现
    ///
    /// 支持跨块写入：将数据按块边界分段，对每个块独立处理。
    /// 对于已分配的块（FullyPresent/Undefined/PartiallyPresent 且偏移非零），
    /// 直接写入现有偏移。
    /// 对于未分配的块（NotPresent/Zero/Unmapped），自动分配 payload block：
    /// 1. 计算文件末尾并对齐到 1MiB 边界
    /// 2. 将文件扩展以容纳新块
    /// 3. 更新 BAT 条目为 `FullyPresent` + 新偏移并持久化到磁盘
    fn write_dynamic(&self, offset: u64, data: &[u8]) -> Result<()> {
        let block_size = u64::from(self.block_size);
        let chunk_ratio = u64::from(Bat::calculate_chunk_ratio(
            self.logical_sector_size,
            self.block_size,
        ));
        if chunk_ratio == 0 {
            return Err(Error::InvalidParameter(
                "Invalid dynamic BAT chunk ratio: 0".to_string(),
            ));
        }

        let mut processed = 0usize;
        while processed < data.len() {
            let current_offset = offset + u64::try_from(processed).unwrap_or(u64::MAX - offset);
            let block_idx = current_offset / block_size;
            let block_offset = current_offset % block_size;

            // 计算 payload BAT 索引（MS-VHDX §2.5.1 chunk 交错规则）
            let bat_payload_index = block_idx + (block_idx / chunk_ratio);

            // 计算当前块内可写入的字节数
            let remaining = data.len() - processed;
            let bytes_in_block = usize::try_from(block_size - block_offset).unwrap_or(remaining);
            let chunk_len = remaining.min(bytes_in_block);

            // 获取或分配 payload block 的文件偏移
            let block_file_offset = self.get_or_allocate_block(bat_payload_index, block_idx)?;

            // 写入数据到块内偏移位置
            let mut file = self.inner.try_clone()?;
            file.seek(SeekFrom::Start(block_file_offset + block_offset))?;
            file.write_all(&data[processed..processed + chunk_len])?;

            processed += chunk_len;
        }

        Ok(())
    }

    /// 获取 payload block 的文件偏移，若块未分配则自动分配
    ///
    /// 对于已分配的块（FullyPresent/Undefined/PartiallyPresent 且偏移非零），
    /// 直接返回现有偏移。对于未分配的块（NotPresent/Zero/Unmapped），
    /// 在文件末尾对齐到 1MiB 处分配新的 payload block 并更新 BAT。
    fn get_or_allocate_block(&self, bat_payload_index: u64, block_idx: u64) -> Result<u64> {
        let bat = self.sections.bat()?;
        match bat.entry(bat_payload_index) {
            Some(entry) => match entry.state() {
                BatState::Payload(state) => match state {
                    PayloadBlockState::FullyPresent
                    | PayloadBlockState::Undefined
                    | PayloadBlockState::PartiallyPresent
                        if entry.file_offset() > 0 =>
                    {
                        // 块已分配：返回现有偏移
                        Ok(entry.file_offset())
                    }
                    PayloadBlockState::NotPresent
                    | PayloadBlockState::Zero
                    | PayloadBlockState::Unmapped => {
                        // 块未分配：自动分配
                        drop(bat);
                        self.allocate_payload_block(bat_payload_index)
                    }
                    _ => Err(Error::InvalidParameter(format!(
                        "Dynamic write requires allocated payload offset for state {state:?} at block {block_idx}"
                    ))),
                },
                BatState::SectorBitmap(state) => Err(Error::InvalidParameter(format!(
                    "Dynamic write expects payload BAT entry but got sector bitmap state {state:?} at BAT index {bat_payload_index}"
                ))),
            },
            None => Err(Error::InvalidParameter(format!(
                "Dynamic write BAT payload index {bat_payload_index} out of range"
            ))),
        }
    }

    /// 为未分配的 payload block 分配文件空间并更新 BAT
    ///
    /// 分配策略：在文件末尾对齐到 1MiB 边界处分配 `block_size` 字节空间，
    /// 将 BAT 条目更新为 `FullyPresent` 状态并持久化到磁盘。
    fn allocate_payload_block(&self, bat_payload_index: u64) -> Result<u64> {
        let block_size = u64::from(self.block_size);

        // 获取当前文件大小并对齐到 1MiB
        let file_len = self.inner.metadata()?.len();
        let new_offset = align_1mib(file_len);
        let new_offset_mb = new_offset / MiB;

        // 扩展文件以容纳新块（写入最后一个字节使文件扩展）
        {
            let mut file = self.inner.try_clone()?;
            file.seek(SeekFrom::Start(new_offset + block_size - 1))?;
            file.write_all(&[0u8])?;
        }

        // 更新内存中的 BAT 缓存
        let index = usize::try_from(bat_payload_index)
            .map_err(|_| Error::InvalidParameter("BAT index overflow".to_string()))?;
        {
            let mut bat = self.sections.bat_mut()?;
            bat.update_entry(
                index,
                BatState::Payload(PayloadBlockState::FullyPresent),
                new_offset_mb,
            )?;
        }

        // 将更新后的 BAT 条目持久化到磁盘
        {
            let bat_disk_offset = self.sections.bat_disk_offset()
                + u64::from(BAT_ENTRY_SIZE as u32) * bat_payload_index;
            let mut file = self.inner.try_clone()?;
            file.seek(SeekFrom::Start(bat_disk_offset))?;
            let raw_entry = BatEntry::new(
                BatState::Payload(PayloadBlockState::FullyPresent),
                new_offset_mb,
            );
            file.write_all(&raw_entry.raw().to_le_bytes())?;
            file.sync_all()?;
        }

        Ok(new_offset)
    }

    /// 内部使用的原始刷新方法（通过克隆文件句柄实现 &self 刷新）
    #[allow(dead_code)]
    pub(crate) fn flush_raw(&self) -> Result<()> {
        let file = self.inner.try_clone()?;
        file.sync_all()?;
        Ok(())
    }

    /// 打开 VHDX 文件的核心实现
    ///
    /// 打开流程：
    /// 1. 使用共享模式打开文件（Windows 上允许并发读取）
    /// 2. 验证文件类型签名 "vhdxfile"
    /// 3. 读取并解析 1MB 头部区域
    /// 4. 从头部获取活动头部结构（序列号较大者）
    /// 5. 解析区域表，获取 BAT 和元数据区域位置
    /// 6. 读取元数据，提取虚拟磁盘参数
    /// 7. 处理日志回放（如有未完成的日志条目）
    fn open_file(path: &Path, writable: bool) -> Result<Self> {
        // 约定：内部默认策略与外部 `File::open(...).finish()` 保持一致，均为 Require。
        // 若调用方需要 Auto/InMemory/NoReplay 语义，必须显式传入策略。
        Self::open_file_with_options(path, writable, true, LogReplayPolicy::Require)
    }

    /// 打开 VHDX 文件的核心实现（带策略选项）
    fn open_file_with_options(
        path: &Path, writable: bool, strict: bool, log_replay: LogReplayPolicy,
    ) -> Result<Self> {
        // 步骤 1：以共享模式打开文件
        let mut file = Self::open_file_with_share_mode(path, writable)?;

        // 步骤 2：验证文件类型签名
        let mut file_type_data = [0u8; 8];
        file.read_exact(&mut file_type_data)?;
        if &file_type_data != FILE_TYPE_SIGNATURE {
            return Err(Error::InvalidSignature {
                expected: String::from_utf8_lossy(FILE_TYPE_SIGNATURE).to_string(),
                found: String::from_utf8_lossy(&file_type_data).to_string(),
            });
        }

        // 步骤 3：回到文件起始位置，读取完整的 1MB 头部区域
        file.seek(SeekFrom::Start(0))?;

        let mut header_data = vec![0u8; HEADER_SECTION_SIZE];
        file.read_exact(&mut header_data)?;
        // 解析头部，包含文件类型标识符、两个头部结构和两个区域表
        let header = Header::new(header_data)?;

        // 步骤 4：获取活动头部结构（序列号较大者获胜）
        let current_header = header
            .header(0)
            .ok_or_else(|| Error::CorruptedHeader("No valid header found".to_string()))?;
        // 步骤 5：获取活动区域表
        let region_table = header
            .region_table(0)
            .ok_or_else(|| Error::InvalidRegionTable("No valid region table found".to_string()))?;

        // 区域条目校验：
        // - strict=true：required 和 optional unknown 均拒绝
        // - strict=false：仅拒绝 required unknown，允许 optional unknown
        Self::validate_region_entries(&region_table, strict)?;

        // 从区域表中提取 BAT 和元数据区域的位置和大小
        let (bat_offset, bat_size, metadata_offset, metadata_size) =
            Self::extract_region_info(&region_table)?;
        // 步骤 6：从元数据区域提取虚拟磁盘参数
        let (virtual_disk_size, block_size, is_fixed, has_parent, logical_sector_size) =
            Self::read_metadata(&mut file, metadata_offset, metadata_size, strict)?;

        // 获取日志区域的位置和大小
        let log_offset = current_header.log_offset();
        let log_size = u64::from(current_header.log_length());

        // 计算 BAT 表项总数
        let entry_count =
            Bat::calculate_total_entries(virtual_disk_size, block_size, logical_sector_size);

        // 构建延迟加载的区域容器
        let file_clone2 = file.try_clone()?;
        let sections = Sections::new(SectionsConfig {
            file: file_clone2,
            bat_offset,
            bat_size,
            metadata_offset,
            metadata_size,
            log_offset,
            log_size,
            entry_count,
            logical_sector_size,
            block_size,
        });

        // 步骤 7：处理日志回放
        let (has_pending_logs, replay_overlay) =
            Self::handle_log_replay(&mut file, &sections, &current_header, writable, log_replay)?;

        // 步骤 8：可写打开时执行会话初始化头部更新（MS-VHDX §2.2.2）
        // 更新非活动头部副本，递增序列号并生成新的 FileWriteGuid，
        // 使被更新的副本成为新的活动头部。只读打开不执行此步骤。
        if writable {
            Self::init_session_header(&mut file)?;
            sections.invalidate_caches();
        }

        Ok(Self {
            inner: file,
            sections,
            virtual_disk_size,
            block_size,
            logical_sector_size,
            is_fixed,
            has_parent,
            has_pending_logs,
            writable,
            opened_path: path.to_path_buf(),
            replay_overlay,
        })
    }

    /// 以共享模式打开文件，Windows 上使用 `FILE_SHARE_READ | FILE_SHARE_WRITE` 避免锁定冲突
    fn open_file_with_share_mode(path: &Path, writable: bool) -> Result<StdFile> {
        let mut options = StdOpenOptions::new();
        options.read(true);
        if writable {
            options.write(true);
        }

        // Windows 平台：设置共享模式，允许其他进程同时读写
        #[cfg(windows)]
        {
            const FILE_SHARE_READ: u32 = 0x0000_0001;
            const FILE_SHARE_WRITE: u32 = 0x0000_0002;
            options.share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
        }

        match options.open(path) {
            Ok(f) => Ok(f),
            Err(e) => {
                // Windows 平台：检测文件被锁定的情况（错误码 5 = ACCESS_DENIED）
                #[cfg(windows)]
                {
                    if e.raw_os_error() == Some(5) {
                        return Err(Error::FileLocked);
                    }
                }
                Err(Error::Io(e))
            }
        }
    }

    /// 从区域表提取 BAT 和元数据区域的位置和大小
    fn extract_region_info(
        region_table: &crate::sections::RegionTable<'_>,
    ) -> Result<(u64, u64, u64, u64)> {
        // 查找 BAT 区域条目
        let bat_entry = region_table
            .find_entry(&region_guids::BAT_REGION)
            .ok_or_else(|| Error::InvalidRegionTable("BAT region not found".to_string()))?;
        let bat_offset = bat_entry.file_offset();
        let bat_size = u64::from(bat_entry.length());

        // 查找元数据区域条目
        let metadata_entry = region_table
            .find_entry(&region_guids::METADATA_REGION)
            .ok_or_else(|| Error::InvalidRegionTable("Metadata region not found".to_string()))?;
        let metadata_offset = metadata_entry.file_offset();
        let metadata_size = u64::from(metadata_entry.length());

        Ok((bat_offset, bat_size, metadata_offset, metadata_size))
    }

    /// 读取并解析元数据区域，提取虚拟磁盘参数
    fn read_metadata(
        file: &mut StdFile, metadata_offset: u64, metadata_size: u64, strict: bool,
    ) -> Result<(u64, u32, bool, bool, u32)> {
        // 克隆文件句柄以避免影响原文件指针位置
        let mut file_clone = file.try_clone()?;
        file_clone.seek(SeekFrom::Start(metadata_offset))?;
        let mut metadata_data = vec![0u8; usize::try_from(metadata_size).unwrap_or(0)];
        file_clone.read_exact(&mut metadata_data)?;
        let temp_metadata = crate::sections::Metadata::new(metadata_data)?;
        // 元数据项校验：
        // - strict=true：required 和 optional unknown 均拒绝
        // - strict=false：仅拒绝 required unknown，允许 optional unknown
        Self::validate_metadata_items(&temp_metadata, strict)?;
        let temp_items = temp_metadata.items();

        // 提取虚拟磁盘大小
        let virtual_disk_size = temp_items
            .virtual_disk_size()
            .ok_or_else(|| Error::InvalidMetadata("Virtual disk size not found".to_string()))?;

        // 提取文件参数（块大小、是否 Fixed、是否有父磁盘）
        let file_params = temp_items
            .file_parameters()
            .ok_or_else(|| Error::InvalidMetadata("File parameters not found".to_string()))?;
        let block_size = file_params.block_size();
        let is_fixed = file_params.leave_block_allocated();
        let has_parent = file_params.has_parent();

        // 提取逻辑扇区大小，默认 512 字节
        let logical_sector_size = temp_items
            .logical_sector_size()
            .unwrap_or(LOGICAL_SECTOR_SIZE_512);

        Ok((
            virtual_disk_size,
            block_size,
            is_fixed,
            has_parent,
            logical_sector_size,
        ))
    }

    /// 校验元数据项是否均为已知项
    ///
    /// - `strict=true`：required 和 optional unknown 均拒绝
    /// - `strict=false`：仅拒绝 required unknown，允许 optional unknown
    fn validate_metadata_items(
        metadata: &crate::sections::Metadata<'_>, strict: bool,
    ) -> Result<()> {
        for entry in metadata.table().entries() {
            let item_id = entry.item_id();
            let is_known = Self::is_known_metadata_item_id(&item_id);
            if entry.flags().is_required() && !is_known {
                // required unknown 始终拒绝
                return Err(Error::InvalidMetadata(format!(
                    "Unknown required metadata item: {item_id:?}"
                )));
            }
            if strict && !is_known {
                // strict=true 时 optional unknown 也拒绝
                return Err(Error::InvalidMetadata(format!(
                    "Unknown optional metadata item (strict mode): {item_id:?}"
                )));
            }
        }
        Ok(())
    }

    /// 判断元数据项 GUID 是否为规范已知项
    fn is_known_metadata_item_id(item_id: &Guid) -> bool {
        *item_id == crate::common::constants::metadata_guids::FILE_PARAMETERS
            || *item_id == crate::common::constants::metadata_guids::VIRTUAL_DISK_SIZE
            || *item_id == crate::common::constants::metadata_guids::VIRTUAL_DISK_ID
            || *item_id == crate::common::constants::metadata_guids::LOGICAL_SECTOR_SIZE
            || *item_id == crate::common::constants::metadata_guids::PHYSICAL_SECTOR_SIZE
            || *item_id == crate::common::constants::metadata_guids::PARENT_LOCATOR
    }

    /// 校验区域条目是否均为已知项
    ///
    /// - `strict=true`：required 和 optional unknown 均拒绝
    /// - `strict=false`：仅拒绝 required unknown，允许 optional unknown
    fn validate_region_entries(
        region_table: &crate::sections::RegionTable<'_>, strict: bool,
    ) -> Result<()> {
        for entry in region_table.entries() {
            let is_known = Self::is_known_region_guid(&entry.guid());
            if entry.required() && !is_known {
                // required unknown 始终拒绝
                return Err(Error::InvalidRegionTable(format!(
                    "Unknown required region: {:?}",
                    entry.guid()
                )));
            }
            if strict && !is_known {
                // strict=true 时 optional unknown 也拒绝
                return Err(Error::InvalidRegionTable(format!(
                    "Unknown optional region (strict mode): {:?}",
                    entry.guid()
                )));
            }
        }
        Ok(())
    }

    /// 判断区域 GUID 是否为规范已知项
    fn is_known_region_guid(guid: &Guid) -> bool {
        *guid == region_guids::BAT_REGION || *guid == region_guids::METADATA_REGION
    }

    /// 可写打开时的会话初始化头部更新（MS-VHDX §2.2.2）
    ///
    /// VHDX 规范要求以写入模式打开文件时，必须更新非活动头部副本：
    /// - 将序列号设为当前活动头部的 `sequence_number + 1`
    /// - 生成新的 `FileWriteGuid` 标记本次写入会话
    ///
    /// 此操作使被更新的头部副本成为新的活动头部（序列号更大），
    /// 保持双头安全机制的一致性。只读打开路径不应调用此方法。
    fn init_session_header(file: &mut StdFile) -> Result<()> {
        // 重新读取头部区域以获取经过日志回放后的最新状态
        file.seek(SeekFrom::Start(0))?;
        let mut header_bytes = vec![0u8; HEADER_SECTION_SIZE];
        file.read_exact(&mut header_bytes)?;
        let header = Header::new(header_bytes)?;

        let h1 = header.header(1).ok_or_else(|| {
            Error::CorruptedHeader("Header 1 parse failed during session init".to_string())
        })?;
        let h2 = header.header(2).ok_or_else(|| {
            Error::CorruptedHeader("Header 2 parse failed during session init".to_string())
        })?;

        // 确定当前活动头部和非活动头部的偏移
        // MS-VHDX §2.2.2.1：序列号较大的头部为活动头部；
        // 序列号相等时 header(0) 选取 h2，因此 h1 为非活动头部
        let (current, non_current_file_offset) = if h1.sequence_number() > h2.sequence_number() {
            (h1, HEADER_2_OFFSET as u64) // h1 是活动头部，更新 h2
        } else {
            (h2, HEADER_1_OFFSET as u64) // h2 是活动头部（或两者相等），更新 h1
        };

        // 生成新的 FileWriteGuid 标记本次会话
        let new_file_write_guid = Guid::from(uuid::Uuid::new_v4());

        // 构造更新后的头部结构
        let new_header = HeaderStructure::create(
            current.sequence_number() + 1,
            new_file_write_guid,
            current.data_write_guid(),
            current.log_guid(),
            current.log_length(),
            current.log_offset(),
        );

        // 写入非活动头部位置
        file.seek(SeekFrom::Start(non_current_file_offset))?;
        file.write_all(&new_header)?;
        file.sync_all()?;

        Ok(())
    }

    /// 处理日志回放，如有未完成的日志条目则回放并更新头部
    fn handle_log_replay(
        file: &mut StdFile, sections: &Sections<'_>,
        current_header: &crate::sections::HeaderStructure<'_>, writable: bool,
        policy: LogReplayPolicy,
    ) -> Result<(bool, Option<ReplayOverlay>)> {
        let current_log_guid = current_header.log_guid();

        // 检查日志 GUID 是否为空，非空表示存在日志条目
        if current_log_guid != Guid::nil() {
            let log = sections.log()?;
            if (*log).is_replay_required() {
                match policy {
                    LogReplayPolicy::Require => return Err(Error::LogReplayRequired),
                    LogReplayPolicy::Auto => {
                        if writable {
                            Self::replay_log_and_clear_guid(
                                file,
                                current_header,
                                &log,
                                current_log_guid,
                            )?;
                            // log 借用在此处（`}` 后）释放，以便 invalidate_caches
                            // 可以再次安全地 borrow RefCell
                        } else {
                            let overlay = Self::build_replay_overlay(file, &log, current_log_guid)?;
                            return Ok((false, Some(overlay)));
                        }
                    }
                    LogReplayPolicy::InMemoryOnReadOnly => {
                        if writable {
                            return Err(Error::InvalidParameter(
                                "InMemoryOnReadOnly policy requires read-only open".to_string(),
                            ));
                        }
                        let overlay = Self::build_replay_overlay(file, &log, current_log_guid)?;
                        return Ok((false, Some(overlay)));
                    }
                    LogReplayPolicy::ReadOnlyNoReplay => {
                        if writable {
                            return Err(Error::InvalidParameter(
                                "ReadOnlyNoReplay policy requires read-only open".to_string(),
                            ));
                        }
                        return Ok((true, None));
                    }
                }
            }
        }
        // 回放修改了文件内容（BAT、metadata、header），
        // 清除延迟加载缓存，使后续访问重新从文件读取。
        // 此处已脱离 sections.log() 的 RefCell 借用范围，可以安全操作。
        sections.invalidate_caches();
        Ok((false, None))
    }

    /// 基于日志条目构建只读内存回放覆盖层
    fn build_replay_overlay(
        file: &StdFile, log: &std::cell::Ref<'_, crate::sections::Log>, expected_log_guid: Guid,
    ) -> Result<ReplayOverlay> {
        let mut writes = Vec::new();

        let file_len = file.metadata().map_or(0, |m| m.len());

        for entry in (*log).entries_for_log_guid(expected_log_guid)? {
            let header = entry.header();
            if header.signature() != crate::common::constants::LOG_ENTRY_SIGNATURE {
                return Err(Error::LogEntryCorrupted(
                    "Invalid log entry signature".to_string(),
                ));
            }

            // 文件尺寸约束：文件长度不得小于 flushed_file_offset
            let flushed = header.flushed_file_offset();
            if flushed > 0 && file_len < flushed {
                return Err(Error::LogEntryCorrupted(format!(
                    "File size ({file_len}) is less than flushed_file_offset ({flushed})"
                )));
            }

            let descriptors = entry.descriptors();
            let data_sectors = entry.data();
            let mut data_sector_index = 0usize;

            for desc in descriptors {
                match desc {
                    crate::sections::Descriptor::Data(data_desc) => {
                        if data_sector_index < data_sectors.len() {
                            let sector = &data_sectors[data_sector_index];
                            let leading =
                                usize::try_from(data_desc.leading_bytes()).map_err(|_| {
                                    Error::LogEntryCorrupted(
                                        "leading_bytes exceeds usize::MAX".to_string(),
                                    )
                                })?;
                            let trailing =
                                usize::try_from(data_desc.trailing_bytes()).map_err(|_| {
                                    Error::LogEntryCorrupted(
                                        "trailing_bytes exceeds usize::MAX".to_string(),
                                    )
                                })?;

                            let sector_data = sector.data();

                            // 边界安全：leading + trailing 不得超过扇区数据长度
                            if leading
                                .checked_add(trailing)
                                .is_none_or(|sum| sum > sector_data.len())
                            {
                                return Err(Error::LogEntryCorrupted(format!(
                                    "leading_bytes ({leading}) + trailing_bytes ({trailing}) \
                                     exceeds sector data size ({})",
                                    sector_data.len()
                                )));
                            }

                            let effective_len = sector_data.len() - leading - trailing;

                            writes.push(ReplayWrite {
                                file_offset: data_desc
                                    .file_offset()
                                    .checked_add(u64::try_from(leading).map_err(|_| {
                                        Error::LogEntryCorrupted(
                                            "file_offset + leading overflow".to_string(),
                                        )
                                    })?)
                                    .ok_or_else(|| {
                                        Error::LogEntryCorrupted(
                                            "file_offset + leading overflow".to_string(),
                                        )
                                    })?,
                                data: sector_data[..effective_len].to_vec(),
                            });
                            data_sector_index += 1;
                        }
                    }
                    crate::sections::Descriptor::Zero(zero_desc) => {
                        let zero_len = usize::try_from(zero_desc.zero_length()).map_err(|_| {
                            Error::LogEntryCorrupted(
                                "Log zero_length exceeds usize::MAX".to_string(),
                            )
                        })?;
                        writes.push(ReplayWrite {
                            file_offset: zero_desc.file_offset(),
                            data: vec![0u8; zero_len],
                        });
                    }
                }
            }
        }

        Ok(ReplayOverlay { writes })
    }

    /// 将只读内存回放覆盖层应用到读取缓冲区
    fn apply_replay_overlay(overlay: &ReplayOverlay, read_offset: u64, buf: &mut [u8]) {
        let read_len = u64::try_from(buf.len()).unwrap_or(u64::MAX);
        let read_end = read_offset.saturating_add(read_len);

        for write in &overlay.writes {
            let write_len = u64::try_from(write.data.len()).unwrap_or(u64::MAX);
            let write_end = write.file_offset.saturating_add(write_len);

            let start = read_offset.max(write.file_offset);
            let end = read_end.min(write_end);
            if start >= end {
                continue;
            }

            let dst_start = usize::try_from(start.saturating_sub(read_offset)).unwrap_or(0);
            let src_start = usize::try_from(start.saturating_sub(write.file_offset)).unwrap_or(0);
            let copy_len = usize::try_from(end.saturating_sub(start)).unwrap_or(0);

            if dst_start + copy_len <= buf.len() && src_start + copy_len <= write.data.len() {
                buf[dst_start..dst_start + copy_len]
                    .copy_from_slice(&write.data[src_start..src_start + copy_len]);
            }
        }
    }

    /// 执行日志回放并按照双头一致性策略更新头部（MS-VHDX §2.2.2）
    ///
    /// 回放完成后，将日志 GUID 清零并更新非活动头部副本：
    /// - 序列号设为当前活动头部的 `sequence_number + 1`
    /// - 生成新的 `FileWriteGuid` 标记回放后的写入会话
    /// - 仅写入非活动头部位置，遵循交替写入策略
    fn replay_log_and_clear_guid(
        file: &mut StdFile, current_header: &crate::sections::HeaderStructure<'_>,
        log: &std::cell::Ref<'_, crate::sections::Log>, expected_log_guid: Guid,
    ) -> Result<()> {
        (*log).replay_with_log_guid(file, expected_log_guid)?;
        file.sync_all()?;

        // 重新读取头部区域以获取回放后的最新状态（回放可能修改了 BAT/metadata，
        // 但头部区域本身未被回放修改，使用回放前的 current_header 作为基准）
        let _ = current_header; // 保留参数以维持签名兼容性

        // 确定非活动头部的偏移量
        // 需要重新读取两个头部来确定非活动位置
        file.seek(SeekFrom::Start(0))?;
        let mut header_bytes = vec![0u8; HEADER_SECTION_SIZE];
        file.read_exact(&mut header_bytes)?;
        let header = Header::new(header_bytes)?;

        let parsed_h1 = header.header(1).ok_or_else(|| {
            Error::CorruptedHeader("Header 1 parse failed after replay".to_string())
        })?;
        let parsed_h2 = header.header(2).ok_or_else(|| {
            Error::CorruptedHeader("Header 2 parse failed after replay".to_string())
        })?;

        // 确定当前活动头部及其序列号
        let (active_seq, non_current_file_offset) =
            if parsed_h1.sequence_number() > parsed_h2.sequence_number() {
                (parsed_h1.sequence_number(), HEADER_2_OFFSET as u64)
            } else {
                (parsed_h2.sequence_number(), HEADER_1_OFFSET as u64)
            };

        // 生成新的 FileWriteGuid 标记回放后的会话
        let new_file_write_guid = Guid::from(uuid::Uuid::new_v4());

        // 构造更新后的头部：序列号递增、日志 GUID 清零
        let new_header = HeaderStructure::create(
            active_seq + 1,
            new_file_write_guid,
            current_header.data_write_guid(),
            Guid::nil(), // 清除日志 GUID
            current_header.log_length(),
            current_header.log_offset(),
        );

        // 仅写入非活动头部位置（交替写入策略）
        file.seek(SeekFrom::Start(non_current_file_offset))?;
        file.write_all(&new_header)?;
        file.sync_all()?;
        Ok(())
    }

    /// 创建 VHDX 文件的核心实现
    ///
    /// 创建流程：
    /// 1. 验证参数（大小、块大小、扇区大小）
    /// 2. 计算文件布局（各区域偏移和大小）
    /// 3. 写入文件类型标识符（含签名和创建者信息）
    /// 4. 写入元数据区域（表头 + 表项 + 数据）
    /// 5. 写入 BAT（Fixed 类型标记所有块为 `FullyPresent`）
    /// 6. 写入空的日志区域
    /// 7. 写入两个头部结构（含序列号和 GUID）
    /// 8. 写入两个区域表
    /// 9. Fixed 类型：预分配数据区域
    /// 10. 重新打开文件（通过 `open_file` 验证完整性）
    fn create_file(
        path: &Path, virtual_size: u64, fixed: bool, has_parent: bool, parent_path: Option<&Path>,
        block_size: u32, logical_sector_size: u32, physical_sector_size: u32,
    ) -> Result<Self> {
        // 步骤 1：验证创建参数
        Self::validate_create_params(
            virtual_size,
            block_size,
            logical_sector_size,
            physical_sector_size,
        )?;

        // 确保文件不存在（防止意外覆盖）
        if path.exists() {
            return Err(Error::InvalidParameter(format!(
                "File already exists: {}",
                path.display()
            )));
        }

        // 创建新文件（读写模式）
        let mut file = StdOpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;

        // 生成文件写入 GUID 和数据写入 GUID
        let file_write_guid = Guid::from(uuid::Uuid::new_v4());
        let data_write_guid = Guid::from(uuid::Uuid::new_v4());
        // 日志 GUID 初始为空（无日志活动）
        let log_guid = Guid::nil();

        // 步骤 2：计算文件布局
        let (
            bat_offset,
            bat_size,
            metadata_offset,
            metadata_size,
            log_offset,
            log_size,
            payload_offset,
            bat_entries,
        ) = Self::calculate_layout(virtual_size, block_size, logical_sector_size);

        // 步骤 3：写入文件类型标识符（含签名 "vhdxfile" 和创建者标识）
        let file_type_data = FileTypeIdentifier::create(Some("vhdx-rs"));
        file.write_all(&file_type_data)?;

        // 填充文件类型标识符到 1MB 头部区域
        let header_padding = vec![0u8; HEADER_SECTION_SIZE - FILE_TYPE_SIZE];
        file.write_all(&header_padding)?;

        // 步骤 4：写入元数据区域
        file.seek(SeekFrom::Start(metadata_offset))?;
        let metadata_data = create_metadata(
            virtual_size,
            block_size,
            logical_sector_size,
            physical_sector_size,
            fixed,
            has_parent,
            parent_path,
            data_write_guid,
        )?;
        file.write_all(&metadata_data)?;
        // 补齐元数据区域到计算的大小
        let actual_metadata_size = u64::try_from(metadata_data.len()).unwrap_or(0);
        if actual_metadata_size < metadata_size {
            let padding =
                vec![0u8; usize::try_from(metadata_size - actual_metadata_size).unwrap_or(0)];
            file.write_all(&padding)?;
        }

        // 步骤 5：写入 BAT
        file.seek(SeekFrom::Start(bat_offset))?;
        let bat_data = Self::create_bat_data(
            fixed,
            bat_entries,
            payload_offset,
            block_size,
            logical_sector_size,
            virtual_size,
        );
        file.write_all(&bat_data)?;

        // 步骤 6：写入空的日志区域
        file.seek(SeekFrom::Start(log_offset))?;
        let log_data = vec![0u8; usize::try_from(log_size).unwrap_or(0)];
        file.write_all(&log_data)?;

        // 步骤 7：创建并写入两个头部结构
        let header_data = HeaderStructure::create(
            0, // 初始序列号为 0
            file_write_guid,
            data_write_guid,
            log_guid,
            u32::try_from(log_size).unwrap_or(0),
            log_offset,
        );

        file.seek(SeekFrom::Start(HEADER_1_OFFSET as u64))?;
        file.write_all(&header_data)?;
        file.seek(SeekFrom::Start(HEADER_2_OFFSET as u64))?;
        file.write_all(&header_data)?;

        // 步骤 8：写入两个区域表
        let region_table_data =
            create_region_table(bat_offset, bat_size, metadata_offset, metadata_size);

        file.seek(SeekFrom::Start(REGION_TABLE_1_OFFSET as u64))?;
        file.write_all(&region_table_data)?;
        file.seek(SeekFrom::Start(REGION_TABLE_2_OFFSET as u64))?;
        file.write_all(&region_table_data)?;

        // 步骤 9：Fixed 类型预分配数据区域（写入最后一个字节使文件扩展到目标大小）
        if fixed {
            let total_size = virtual_size;
            file.seek(SeekFrom::Start(payload_offset + total_size - 1))?;
            file.write_all(&[0u8])?;
        }

        file.sync_all()?;

        // 步骤 10：关闭文件并重新打开以验证完整性
        // 说明：此处复用 open_file 的内部默认（Require），
        // 与外部 File::open 默认契约保持一致，避免策略分裂。
        drop(file);
        Self::open_file(path, true)
    }

    /// 验证创建参数的有效性
    ///
    /// 检查项：
    /// - 虚拟磁盘大小不能为零
    /// - 块大小必须是 2 的幂且在 [`MIN_BLOCK_SIZE`]..[`MAX_BLOCK_SIZE`] 范围内
    /// - 逻辑扇区大小必须为 512 或 4096
    fn validate_create_params(
        virtual_size: u64, block_size: u32, logical_sector_size: u32, physical_sector_size: u32,
    ) -> Result<()> {
        const MAX_VIRTUAL_SIZE_64_TIB: u64 = 64_u64 * 1024 * 1024 * 1024 * 1024;

        if virtual_size == 0 {
            return Err(Error::InvalidParameter(
                "Virtual size cannot be zero".to_string(),
            ));
        }
        if virtual_size > MAX_VIRTUAL_SIZE_64_TIB {
            return Err(Error::InvalidParameter(
                "Virtual size must be less than or equal to 64 TiB".to_string(),
            ));
        }
        if !block_size.is_power_of_two() || !(MIN_BLOCK_SIZE..=MAX_BLOCK_SIZE).contains(&block_size)
        {
            return Err(Error::InvalidParameter(format!(
                "Block size must be power of 2 between {MIN_BLOCK_SIZE} and {MAX_BLOCK_SIZE}"
            )));
        }
        if logical_sector_size != 512 && logical_sector_size != 4096 {
            return Err(Error::InvalidParameter(
                "Logical sector size must be 512 or 4096".to_string(),
            ));
        }
        if physical_sector_size != 512 && physical_sector_size != 4096 {
            return Err(Error::InvalidParameter(
                "Physical sector size must be 512 or 4096".to_string(),
            ));
        }
        if physical_sector_size < logical_sector_size {
            return Err(Error::InvalidParameter(
                "Physical sector size must be greater than or equal to logical sector size"
                    .to_string(),
            ));
        }
        if !virtual_size.is_multiple_of(u64::from(logical_sector_size)) {
            return Err(Error::InvalidParameter(
                "Virtual size must be a multiple of logical sector size".to_string(),
            ));
        }
        Ok(())
    }

    /// 计算 VHDX 文件的布局（各区域偏移和大小）
    ///
    /// 布局顺序（从文件起始位置开始）：
    /// - `0x0000_0000` — 文件类型标识符（64KB）
    /// - `0x0001_0000` — Header 1（64KB）
    /// - `0x0002_0000` — Region Table 1（64KB）
    /// - `0x0003_0000` — Header 2（64KB）
    /// - `0x0004_0000` — Region Table 2（64KB）
    /// - `0x0005_0000` — 元数据区域（1MB 对齐）
    /// - 之后   — BAT 区域（1MB 对齐）
    /// - 之后   — 日志区域（1MB）
    /// - 之后   — 数据区域（1MB 对齐）
    fn calculate_layout(
        virtual_size: u64, block_size: u32, logical_sector_size: u32,
    ) -> (u64, u64, u64, u64, u64, u64, u64, u64) {
        // 计算需要的 BAT 表项数
        let bat_entries =
            Bat::calculate_total_entries(virtual_size, block_size, logical_sector_size);
        // BAT 大小向上对齐到 1MB
        let bat_size = align_1mib(bat_entries * BAT_ENTRY_SIZE as u64);

        // 元数据区域大小（表头 + 数据，向上对齐到 1MB）
        let metadata_size = align_1mib(METADATA_TABLE_SIZE as u64 + 256);

        // 日志区域固定 1MB
        let log_size = MiB;

        // 各区域偏移计算：头部占两个 HEADER_SECTION_SIZE
        let metadata_offset = HEADER_SECTION_SIZE as u64 * 2;
        let bat_offset = metadata_offset + metadata_size;
        let log_offset = bat_offset + bat_size;
        // 数据区域起始位置向上对齐到 1MB
        let payload_offset = align_1mib(log_offset + log_size);

        (
            bat_offset,
            bat_size,
            metadata_offset,
            metadata_size,
            log_offset,
            log_size,
            payload_offset,
            bat_entries,
        )
    }

    /// 创建 BAT 原始数据，Fixed 类型标记 Payload 块为 `FullyPresent`
    ///
    /// 每个 BAT 条目为 8 字节，编码了块状态（高 4 位）和块偏移（低 60 位）。
    /// Fixed 类型将 Payload 块标记为 `FullyPresent`（状态值 6），并指向连续的数据区域。
    /// Sector Bitmap 条目保持为零（`NotPresent` + 偏移 0），因为 Fixed 类型不需要位图。
    /// Dynamic 类型创建全零 BAT（所有块标记为 `NotPresent`）。
    ///
    /// BAT 条目按 `chunk_ratio` 个 Payload 条目后接 1 个 Sector Bitmap 条目交错排列
    /// （MS-VHDX §2.5）。使用 `Bat::is_sector_bitmap_entry_index` 判断条目类型。
    fn create_bat_data(
        fixed: bool, bat_entries: u64, payload_offset: u64, block_size: u32,
        logical_sector_size: u32, virtual_size: u64,
    ) -> Vec<u8> {
        if fixed {
            let mut entries = vec![0u8; usize::try_from(bat_entries).unwrap_or(0) * BAT_ENTRY_SIZE];
            let chunk_ratio = Bat::calculate_chunk_ratio(logical_sector_size, block_size);
            let payload_blocks = Bat::calculate_payload_blocks(virtual_size, block_size);
            // Payload 块分配计数器：仅对 Payload 条目递增，跳过 Sector Bitmap 条目
            let mut payload_idx: u64 = 0;
            for i in 0..bat_entries {
                let is_bitmap = Bat::is_sector_bitmap_entry_index(
                    usize::try_from(i).unwrap_or(0),
                    usize::try_from(chunk_ratio).unwrap_or(0),
                    usize::try_from(payload_blocks).unwrap_or(0),
                );
                if is_bitmap {
                    // Sector Bitmap 条目：保持全零（NotPresent + 偏移 0）
                    continue;
                }
                let offset = usize::try_from(i).unwrap_or(0) * BAT_ENTRY_SIZE;
                // 将块偏移转换为 MB 单位，左移 20 位后与状态值 6（FullyPresent）组合
                let payload_offset_mb =
                    (payload_offset + payload_idx * u64::from(block_size)) / MiB;
                let state_and_offset = (payload_offset_mb << 20) | 6u64;
                entries[offset..offset + 8].copy_from_slice(&state_and_offset.to_le_bytes());
                payload_idx += 1;
            }
            entries
        } else {
            // Dynamic 类型：全零表示所有块均未分配
            vec![0u8; usize::try_from(bat_entries).unwrap_or(0) * BAT_ENTRY_SIZE]
        }
    }
}

/// VHDX 文件打开选项构建器
///
/// 使用 Builder 模式配置打开选项。
///
/// # 示例
/// ```no_run
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     use vhdx_rs::File;
///
///     let _file = File::open("disk.vhdx").write().finish()?;
///     Ok(())
/// }
/// ```
pub struct OpenOptions {
    /// 要打开的 VHDX 文件路径
    path: std::path::PathBuf,
    /// 是否以写入模式打开
    write: bool,
    /// 是否启用严格模式
    strict: bool,
    /// 日志回放策略
    log_replay: LogReplayPolicy,
}

impl OpenOptions {
    /// 设置以写入模式打开文件
    #[must_use]
    pub const fn write(mut self) -> Self {
        self.write = true;
        self
    }

    /// 设置严格模式
    ///
    /// strict=true 时启用严格校验。
    ///
    /// 说明：strict=false 仅放宽 optional unknown，
    /// required unknown（Region/Metadata）仍会报错。
    #[must_use]
    pub const fn strict(mut self, strict: bool) -> Self {
        self.strict = strict;
        self
    }

    /// 设置日志回放策略
    #[must_use]
    pub const fn log_replay(mut self, policy: LogReplayPolicy) -> Self {
        self.log_replay = policy;
        self
    }

    /// 完成选项配置并打开 VHDX 文件
    pub fn finish(self) -> Result<File> {
        File::open_file_with_options(&self.path, self.write, self.strict, self.log_replay)
    }
}

/// VHDX 文件创建选项构建器
///
/// 使用 Builder 模式配置创建选项。
///
/// # 示例
/// ```no_run
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     use vhdx_rs::File;
///
///     let _file = File::create("disk.vhdx")
///         .size(10 * 1024 * 1024 * 1024)  // 10GB
///         .fixed(true)
///         .block_size(32 * 1024 * 1024)    // 32MB
///         .finish()?;
///     Ok(())
/// }
/// ```
pub struct CreateOptions {
    /// 要创建的 VHDX 文件路径
    path: std::path::PathBuf,
    /// 虚拟磁盘大小（字节），必填
    size: Option<u64>,
    /// 是否创建 Fixed 类型磁盘
    fixed: bool,
    /// 是否为差分磁盘（有父磁盘引用）
    has_parent: bool,
    /// 差分磁盘父路径
    parent_path: Option<std::path::PathBuf>,
    /// 块大小（字节）
    block_size: u32,
    /// 逻辑扇区大小（512 或 4096）
    logical_sector_size: u32,
    /// 物理扇区大小（512 或 4096）
    physical_sector_size: u32,
}

impl CreateOptions {
    /// 设置虚拟磁盘大小（字节），必填参数
    #[must_use]
    pub const fn size(mut self, size: u64) -> Self {
        self.size = Some(size);
        self
    }

    /// 设置是否创建 Fixed 类型的虚拟磁盘
    #[must_use]
    pub const fn fixed(mut self, fixed: bool) -> Self {
        self.fixed = fixed;
        self
    }

    /// 设置块大小（字节），默认 32MB，必须是 2 的幂且在合法范围内
    #[must_use]
    pub const fn block_size(mut self, block_size: u32) -> Self {
        self.block_size = block_size;
        self
    }

    /// 设置逻辑扇区大小（字节）
    #[must_use]
    pub const fn logical_sector_size(mut self, logical_sector_size: u32) -> Self {
        self.logical_sector_size = logical_sector_size;
        self
    }

    /// 设置物理扇区大小（字节）
    #[must_use]
    pub const fn physical_sector_size(mut self, physical_sector_size: u32) -> Self {
        self.physical_sector_size = physical_sector_size;
        self
    }

    /// 设置父磁盘路径（设置后自动标记为差分磁盘）
    pub fn parent_path(mut self, path: impl AsRef<Path>) -> Self {
        self.parent_path = Some(path.as_ref().to_path_buf());
        self.has_parent = true;
        self
    }

    /// 完成选项配置并创建 VHDX 文件
    pub fn finish(self) -> Result<File> {
        let size = self
            .size
            .ok_or_else(|| Error::InvalidParameter("Virtual disk size is required".to_string()))?;

        if let Some(parent_path) = &self.parent_path
            && !parent_path.exists()
        {
            return Err(Error::ParentNotFound {
                path: parent_path.clone(),
            });
        }

        let has_parent = self.has_parent || self.parent_path.is_some();

        File::create_file(
            &self.path,
            size,
            self.fixed,
            has_parent,
            self.parent_path.as_deref(),
            self.block_size,
            self.logical_sector_size,
            self.physical_sector_size,
        )
    }
}

/// 构造元数据区域的原始字节数据，包含表头、表项和数据
///
/// 元数据区域结构：
/// - 表头：签名、条目数、保留字段
/// - 表项数组：每个条目包含 GUID、偏移、大小和标志位
/// - 数据区域：按表项描述的顺序存储实际数据
fn create_metadata(
    virtual_size: u64, block_size: u32, logical_sector_size: u32, physical_sector_size: u32,
    fixed: bool, has_parent: bool, parent_path: Option<&Path>, disk_id: Guid,
) -> Result<Vec<u8>> {
    use crate::common::metadata_guids;

    let parent_locator_payload = if has_parent {
        let parent = parent_path.ok_or_else(|| {
            Error::InvalidParameter("Differencing disk requires parent_path".to_string())
        })?;
        let parent_file = File::open(parent).finish()?;
        let parent_sections_header = parent_file.sections().header()?;
        let parent_header = parent_sections_header
            .header(0)
            .ok_or_else(|| Error::CorruptedHeader("No valid header found".to_string()))?;
        let parent_linkage = parent_header.data_write_guid();
        Some(build_parent_locator_payload(parent, parent_linkage)?)
    } else {
        None
    };

    let mut data = Vec::with_capacity(METADATA_TABLE_SIZE);

    // 元数据表头：签名 + 保留（2字节）+ 条目数（2字节）+ 保留（20字节）
    let entry_count: u16 = if has_parent { 6 } else { 5 };
    data.extend_from_slice(METADATA_SIGNATURE);
    data.extend_from_slice(&[0u8; 2]); // 保留字段（后续填入校验和）
    data.extend_from_slice(&entry_count.to_le_bytes());
    data.extend_from_slice(&[0u8; 20]); // 保留字段

    // 数据区域从表头之后开始
    let mut current_offset: u32 = u32::try_from(METADATA_TABLE_SIZE).unwrap_or(0);

    // 文件参数标志：bit 0 = leave_block_allocated（Fixed），bit 1 = has_parent
    let fp_flags: u32 = u32::from(fixed) | (u32::from(has_parent) << 1);

    // 表项 1：文件参数（block_size + flags，共 8 字节）
    data.extend_from_slice(metadata_guids::FILE_PARAMETERS.as_bytes());
    data.extend_from_slice(&current_offset.to_le_bytes());
    data.extend_from_slice(&8u32.to_le_bytes()); // 数据大小
    data.extend_from_slice(&0x04u32.to_le_bytes()); // 标志位（is_required）
    data.extend_from_slice(&[0u8; 4]); // 保留
    current_offset += 8;

    // 表项 2：虚拟磁盘大小（8 字节）
    data.extend_from_slice(metadata_guids::VIRTUAL_DISK_SIZE.as_bytes());
    data.extend_from_slice(&current_offset.to_le_bytes());
    data.extend_from_slice(&8u32.to_le_bytes());
    data.extend_from_slice(&0x06u32.to_le_bytes()); // 标志位（is_required | is_virtual_disk_property）
    data.extend_from_slice(&[0u8; 4]);
    current_offset += 8;

    // 表项 3：虚拟磁盘 ID（16 字节 GUID）
    data.extend_from_slice(metadata_guids::VIRTUAL_DISK_ID.as_bytes());
    data.extend_from_slice(&current_offset.to_le_bytes());
    data.extend_from_slice(&16u32.to_le_bytes());
    data.extend_from_slice(&0x06u32.to_le_bytes());
    data.extend_from_slice(&[0u8; 4]);
    current_offset += 16;

    // 表项 4：逻辑扇区大小（4 字节）
    data.extend_from_slice(metadata_guids::LOGICAL_SECTOR_SIZE.as_bytes());
    data.extend_from_slice(&current_offset.to_le_bytes());
    data.extend_from_slice(&4u32.to_le_bytes());
    data.extend_from_slice(&0x06u32.to_le_bytes());
    data.extend_from_slice(&[0u8; 4]);
    current_offset += 4;

    // 表项 5：物理扇区大小（4 字节）
    data.extend_from_slice(metadata_guids::PHYSICAL_SECTOR_SIZE.as_bytes());
    data.extend_from_slice(&current_offset.to_le_bytes());
    data.extend_from_slice(&4u32.to_le_bytes());
    data.extend_from_slice(&0x06u32.to_le_bytes());
    data.extend_from_slice(&[0u8; 4]);
    current_offset += 4;

    // 表项 6（可选）：父磁盘定位器（仅差分磁盘包含）
    if let Some(locator_payload) = parent_locator_payload.as_ref() {
        data.extend_from_slice(metadata_guids::PARENT_LOCATOR.as_bytes());
        data.extend_from_slice(&current_offset.to_le_bytes());
        data.extend_from_slice(
            &u32::try_from(locator_payload.len())
                .map_err(|_| {
                    Error::InvalidParameter(
                        "Parent locator payload exceeds u32::MAX bytes".to_string(),
                    )
                })?
                .to_le_bytes(),
        );
        data.extend_from_slice(&0x06u32.to_le_bytes());
        data.extend_from_slice(&[0u8; 4]);
    }

    // 将表项区域填充到 METADATA_TABLE_SIZE
    while data.len() < METADATA_TABLE_SIZE {
        data.push(0);
    }

    // 数据区域：按表项顺序依次写入

    // 文件参数数据（block_size + flags）
    data.extend_from_slice(&block_size.to_le_bytes());
    data.extend_from_slice(&fp_flags.to_le_bytes());

    // 虚拟磁盘大小
    data.extend_from_slice(&virtual_size.to_le_bytes());

    // 虚拟磁盘 ID（GUID）
    data.extend_from_slice(disk_id.as_bytes());

    // 逻辑扇区大小
    data.extend_from_slice(&logical_sector_size.to_le_bytes());

    // 物理扇区大小
    data.extend_from_slice(&physical_sector_size.to_le_bytes());

    // 差分盘父定位器数据
    if let Some(locator_payload) = parent_locator_payload {
        data.extend_from_slice(&locator_payload);
    }

    Ok(data)
}

/// 构造可被当前解析器读取的 Parent Locator payload。
///
/// 结构（MS-VHDX §2.6.2.6）：
/// - **20 字节头部**：`LocatorType` GUID（16 字节）+ Reserved（2 字节，必须为 0）+ KeyValueCount（2 字节）
/// - **N × 12 字节 entry table**：每项含 `key_offset` / `value_offset` / `key_length` / `value_length`
/// - **UTF-16LE key/value 数据区**：entry 中的偏移量相对于此区域起始位置
fn build_parent_locator_payload(parent_path: &Path, parent_linkage: Guid) -> Result<Vec<u8>> {
    use crate::section::StandardItems::LOCATOR_TYPE_VHDX;

    let parent_path_str = parent_path.to_string_lossy().to_string();
    let entries = [
        ("parent_linkage", format!("{parent_linkage}")),
        ("relative_path", parent_path_str),
    ];

    let key_value_count = u16::try_from(entries.len()).map_err(|_| {
        Error::InvalidParameter("Parent locator key/value count exceeds u16::MAX".to_string())
    })?;

    // 头部 20 字节：LocatorType(16) + Reserved(2) + KeyValueCount(2)
    let mut payload = vec![0u8; 20];
    payload[0..16].copy_from_slice(LOCATOR_TYPE_VHDX.as_bytes());
    // payload[16..18] = Reserved，已为零
    payload[18..20].copy_from_slice(&key_value_count.to_le_bytes());

    let mut entry_table = Vec::with_capacity(entries.len() * 12);
    let mut key_value_data = Vec::new();

    for (key, value) in entries {
        let key_bytes = encode_utf16le(key);
        let value_bytes = encode_utf16le(&value);

        // entry 偏移量相对于 key_value_data 区域起始位置
        let key_offset = u32::try_from(key_value_data.len()).map_err(|_| {
            Error::InvalidParameter("Parent locator key offset exceeds u32::MAX".to_string())
        })?;
        key_value_data.extend_from_slice(&key_bytes);

        let value_offset = u32::try_from(key_value_data.len()).map_err(|_| {
            Error::InvalidParameter("Parent locator value offset exceeds u32::MAX".to_string())
        })?;
        key_value_data.extend_from_slice(&value_bytes);

        let key_length = u16::try_from(key_bytes.len()).map_err(|_| {
            Error::InvalidParameter("Parent locator key length exceeds u16::MAX".to_string())
        })?;
        let value_length = u16::try_from(value_bytes.len()).map_err(|_| {
            Error::InvalidParameter("Parent locator value length exceeds u16::MAX".to_string())
        })?;

        entry_table.extend_from_slice(&key_offset.to_le_bytes());
        entry_table.extend_from_slice(&value_offset.to_le_bytes());
        entry_table.extend_from_slice(&key_length.to_le_bytes());
        entry_table.extend_from_slice(&value_length.to_le_bytes());
    }

    payload.extend_from_slice(&entry_table);
    payload.extend_from_slice(&key_value_data);
    Ok(payload)
}

/// 将字符串编码为 UTF-16LE 字节序列。
fn encode_utf16le(value: &str) -> Vec<u8> {
    value.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

/// 构造区域表的原始字节数据，包含 BAT 和元数据区域条目，自动计算 CRC32C 校验和
///
/// 区域表结构：
/// - 签名（4 字节）
/// - 校验和（4 字节，CRC32C）
/// - 条目数（4 字节）
/// - 保留（4 字节）
/// - 区域条目数组（每个 32 字节：GUID + 偏移 + 大小 + 标志）
fn create_region_table(
    bat_offset: u64, bat_size: u64, metadata_offset: u64, metadata_size: u64,
) -> Vec<u8> {
    use crate::common::region_guids;

    let mut data = vec![0u8; REGION_TABLE_SIZE];

    // 区域表头
    data[0..4].copy_from_slice(REGION_TABLE_SIGNATURE);
    data[4..8].copy_from_slice(&[0; 4]); // 校验和占位，最后计算填入
    data[8..12].copy_from_slice(&2u32.to_le_bytes()); // 2 个区域条目
    data[12..16].copy_from_slice(&[0; 4]); // 保留

    // 区域条目 1：BAT 区域
    data[16..32].copy_from_slice(region_guids::BAT_REGION.as_bytes());
    data[32..40].copy_from_slice(&bat_offset.to_le_bytes());
    data[40..44].copy_from_slice(&(u32::try_from(bat_size).unwrap_or(0_u32)).to_le_bytes());
    data[44..48].copy_from_slice(&1u32.to_le_bytes()); // 标志位（required）

    // 区域条目 2：元数据区域
    data[48..64].copy_from_slice(region_guids::METADATA_REGION.as_bytes());
    data[64..72].copy_from_slice(&metadata_offset.to_le_bytes());
    data[72..76].copy_from_slice(&(u32::try_from(metadata_size).unwrap_or(0_u32)).to_le_bytes());
    data[76..80].copy_from_slice(&1u32.to_le_bytes()); // 标志位（required）

    // 计算 CRC32C 校验和并填入头部
    let checksum = crc32c::crc32c(&data);
    data[4..8].copy_from_slice(&checksum.to_le_bytes());

    data
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 生成临时 VHDX 文件路径
    fn temp_vhdx_path() -> std::path::PathBuf {
        let dir = tempfile::tempdir().expect("Failed to create temp dir");
        let path = dir.path().join("test.vhdx");
        std::mem::forget(dir);
        path
    }

    /// 测试固定磁盘的创建与读写：写入数据后读回并验证一致性
    #[test]
    fn test_create_and_read_fixed_disk() {
        let path = temp_vhdx_path();

        let file = File::create(&path)
            .size(1024 * 1024)
            .fixed(true)
            .finish()
            .expect("Failed to create fixed disk");

        let test_data = b"Hello, VHDX!";
        let bytes_written = file.write_raw(0, test_data).expect("Failed to write");
        assert_eq!(bytes_written, test_data.len());

        file.flush_raw().expect("Failed to flush");

        let mut buf = vec![0u8; test_data.len()];
        let bytes_read = file.read_raw(0, &mut buf).expect("Failed to read");
        assert_eq!(bytes_read, test_data.len());
        assert_eq!(&buf, test_data);
    }

    /// 测试对动态磁盘执行写入操作：写入未分配块应自动分配并成功
    #[test]
    fn test_write_dynamic_disk_auto_allocates() {
        let path = temp_vhdx_path();

        let file = File::create(&path)
            .size(1024 * 1024)
            .fixed(false)
            .finish()
            .expect("Failed to create dynamic disk");

        let result = file.write_raw(0, b"test");
        assert!(
            result.is_ok(),
            "Dynamic write to unallocated block should auto-allocate and succeed"
        );
        assert_eq!(result.unwrap(), 4);
    }

    /// 测试以写入模式打开已有文件并写入数据
    #[test]
    fn test_open_with_write_access() {
        let path = temp_vhdx_path();

        File::create(&path)
            .size(1024 * 1024)
            .fixed(true)
            .finish()
            .expect("Failed to create fixed disk");

        let file = File::open(&path)
            .write()
            .finish()
            .expect("Failed to open with write access");

        let written = file.write_raw(0, b"test data").expect("Failed to write");
        assert_eq!(written, 9);
    }

    /// 测试在非零偏移处写入和读取数据
    #[test]
    fn test_write_and_read_at_offset() {
        let path = temp_vhdx_path();

        let file = File::create(&path)
            .size(1024 * 1024)
            .fixed(true)
            .finish()
            .expect("Failed to create fixed disk");

        let data = b"offset data";
        file.write_raw(512, data)
            .expect("Failed to write at offset");

        let mut buf = vec![0u8; data.len()];
        file.read_raw(512, &mut buf)
            .expect("Failed to read at offset");
        assert_eq!(&buf, data);
    }

    /// 测试读取未写入区域应返回全零
    #[test]
    fn test_read_unwritten_area_returns_zeros() {
        let path = temp_vhdx_path();

        let file = File::create(&path)
            .size(1024 * 1024)
            .fixed(true)
            .finish()
            .expect("Failed to create fixed disk");

        file.write_raw(0, b"some data").expect("Failed to write");

        let mut buf = vec![0u8; 512];
        file.read_raw(4096, &mut buf).expect("Failed to read");
        assert_eq!(buf, vec![0u8; 512], "Unwritten area should be zeros");
    }

    /// 测试多次写入和读取
    #[test]
    fn test_multiple_writes_and_reads() {
        let path = temp_vhdx_path();

        let file = File::create(&path)
            .size(1024 * 1024)
            .fixed(true)
            .finish()
            .expect("Failed to create fixed disk");

        file.write_raw(0, b"block0")
            .expect("Failed to write block0");
        file.write_raw(1024, b"block1")
            .expect("Failed to write block1");
        file.write_raw(2048, b"block2")
            .expect("Failed to write block2");

        let mut buf0 = vec![0u8; 6];
        let mut buf1 = vec![0u8; 6];
        let mut buf2 = vec![0u8; 6];

        file.read_raw(0, &mut buf0).expect("Failed to read block0");
        file.read_raw(1024, &mut buf1)
            .expect("Failed to read block1");
        file.read_raw(2048, &mut buf2)
            .expect("Failed to read block2");

        assert_eq!(&buf0, b"block0");
        assert_eq!(&buf1, b"block1");
        assert_eq!(&buf2, b"block2");
    }

    /// 测试写入后刷新并重新打开文件
    #[test]
    fn test_flush_after_write() {
        let path = temp_vhdx_path();

        let file = File::create(&path)
            .size(1024 * 1024)
            .fixed(true)
            .finish()
            .expect("Failed to create fixed disk");

        file.write_raw(0, b"flush test").expect("Failed to write");
        file.flush_raw().expect("Failed to flush");

        let file = File::open(&path).finish().expect("Failed to reopen");

        let mut buf = vec![0u8; 10];
        file.read_raw(0, &mut buf).expect("Failed to read");
        assert_eq!(&buf, b"flush test");
    }

    // ── strict 语义测试 ──────────────────────────────────────────────

    /// 向 VHDX 文件的两个区域表副本注入一个额外的区域条目
    ///
    /// 参数：
    /// - `required`：注入条目的 required 标志（true=1, false=0）
    fn inject_unknown_region_entry(path: &Path, required: bool) {
        let unknown_guid = Guid::from(uuid::Uuid::new_v4());

        let mut raw = StdOpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .expect("Failed to open VHDX for region injection");

        for &rt_offset in &[REGION_TABLE_1_OFFSET as u64, REGION_TABLE_2_OFFSET as u64] {
            // 读取区域表
            raw.seek(SeekFrom::Start(rt_offset)).expect("seek failed");
            let mut rt_data = vec![0u8; REGION_TABLE_SIZE];
            raw.read_exact(&mut rt_data)
                .expect("read region table failed");

            // 修改 entry_count：+1
            let count = u16::from_le_bytes([rt_data[8], rt_data[9]]) as u32;
            let new_count = count + 1;
            rt_data[8..12].copy_from_slice(&new_count.to_le_bytes());

            // 计算新条目的写入偏移（紧跟已有条目之后）
            let entry_offset = 16 + count as usize * 32;
            if entry_offset + 32 <= REGION_TABLE_SIZE {
                // 写入新条目：GUID(16) + offset(8) + length(4) + required(4)
                rt_data[entry_offset..entry_offset + 16].copy_from_slice(unknown_guid.as_bytes());
                // 文件偏移设为 0（无实际数据区域）
                rt_data[entry_offset + 16..entry_offset + 24]
                    .copy_from_slice(&0x0050_0000_u64.to_le_bytes());
                // 长度设为 1MB
                rt_data[entry_offset + 24..entry_offset + 28]
                    .copy_from_slice(&0x0010_0000_u32.to_le_bytes());
                // required 标志
                let req_flag: u32 = if required { 1 } else { 0 };
                rt_data[entry_offset + 28..entry_offset + 32]
                    .copy_from_slice(&req_flag.to_le_bytes());
            }

            // 重算 CRC32C 校验和（校验和字段本身置零后计算）
            rt_data[4..8].copy_from_slice(&[0; 4]);
            let checksum = crc32c::crc32c(&rt_data);
            rt_data[4..8].copy_from_slice(&checksum.to_le_bytes());

            // 写回
            raw.seek(SeekFrom::Start(rt_offset)).expect("seek failed");
            raw.write_all(&rt_data).expect("write region table failed");
        }
        raw.flush().expect("flush failed");
    }

    /// 向 VHDX 文件的元数据表注入一个额外的元数据条目
    ///
    /// 参数：
    /// - `is_required`：注入条目的 is_required 标志（true=设置 bit 29, false=0）
    fn inject_unknown_metadata_entry(path: &Path, is_required: bool) {
        // 使用一个不属于已知元数据项的 GUID
        let unknown_guid = Guid::from(uuid::Uuid::new_v4());

        // 元数据区域固定起始于 2 * 1MiB（与 create_layout 一致）
        let metadata_offset: u64 = HEADER_SECTION_SIZE as u64 * 2;

        let mut raw = StdOpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .expect("Failed to open VHDX for metadata injection");

        // 读取元数据头部 + 条目区域
        raw.seek(SeekFrom::Start(metadata_offset))
            .expect("seek failed");
        let mut meta_data = vec![0u8; METADATA_TABLE_SIZE];
        raw.read_exact(&mut meta_data)
            .expect("read metadata failed");

        // 当前 entry_count（字节 10..12，u16 LE）
        let count = u16::from_le_bytes([meta_data[10], meta_data[11]]);
        let new_count = count + 1;
        meta_data[10..12].copy_from_slice(&new_count.to_le_bytes());

        // 新条目写入偏移（头部 32 字节 + count 个已有条目）
        let entry_offset = 32 + count as usize * 32;
        if entry_offset + 32 <= METADATA_TABLE_SIZE {
            // GUID(16) + offset(4) + length(4) + flags(4) + reserved(4)
            meta_data[entry_offset..entry_offset + 16].copy_from_slice(unknown_guid.as_bytes());
            // offset=0, length=0
            meta_data[entry_offset + 16..entry_offset + 20].copy_from_slice(&0u32.to_le_bytes());
            meta_data[entry_offset + 20..entry_offset + 24].copy_from_slice(&0u32.to_le_bytes());
            // flags：is_required 对应 bit 29 (0x2000_0000)
            let flags: u32 = if is_required { 0x2000_0000 } else { 0 };
            meta_data[entry_offset + 24..entry_offset + 28].copy_from_slice(&flags.to_le_bytes());
            // reserved
            meta_data[entry_offset + 28..entry_offset + 32].copy_from_slice(&0u32.to_le_bytes());
        }

        // 写回
        raw.seek(SeekFrom::Start(metadata_offset))
            .expect("seek failed");
        raw.write_all(&meta_data).expect("write metadata failed");
        raw.flush().expect("flush failed");
    }

    /// strict=false + optional unknown region => 打开成功
    #[test]
    fn test_strict_false_optional_unknown_region_succeeds() {
        let path = temp_vhdx_path();

        File::create(&path)
            .size(1024 * 1024)
            .fixed(true)
            .finish()
            .expect("Failed to create fixed disk");

        inject_unknown_region_entry(&path, false);

        let result = File::open(&path)
            .strict(false)
            .log_replay(LogReplayPolicy::Require)
            .finish();
        assert!(
            result.is_ok(),
            "strict=false should allow optional unknown region, got: {:?}",
            result.err()
        );
    }

    /// strict=true + optional unknown region => 打开失败
    #[test]
    fn test_strict_true_optional_unknown_region_fails() {
        let path = temp_vhdx_path();

        File::create(&path)
            .size(1024 * 1024)
            .fixed(true)
            .finish()
            .expect("Failed to create fixed disk");

        inject_unknown_region_entry(&path, false);

        let result = File::open(&path)
            .strict(true)
            .log_replay(LogReplayPolicy::Require)
            .finish();
        match result {
            Err(Error::InvalidRegionTable(msg)) => {
                assert!(
                    msg.contains("Unknown optional region (strict mode)"),
                    "Error should mention strict mode, got: {msg}"
                );
            }
            Err(e) => {
                panic!("Expected InvalidRegionTable with strict mode, got different error: {e}")
            }
            Ok(_) => panic!("Expected error, but open succeeded"),
        }
    }

    /// strict=false + required unknown region => 打开失败
    #[test]
    fn test_strict_false_required_unknown_region_fails() {
        let path = temp_vhdx_path();

        File::create(&path)
            .size(1024 * 1024)
            .fixed(true)
            .finish()
            .expect("Failed to create fixed disk");

        inject_unknown_region_entry(&path, true);

        let result = File::open(&path)
            .strict(false)
            .log_replay(LogReplayPolicy::Require)
            .finish();
        match result {
            Err(Error::InvalidRegionTable(msg)) => {
                assert!(
                    msg.contains("Unknown required region"),
                    "Error should mention required region, got: {msg}"
                );
            }
            Err(e) => panic!("Expected InvalidRegionTable, got different error: {e}"),
            Ok(_) => panic!("Expected error, but open succeeded"),
        }
    }

    /// strict=false + optional unknown metadata => 打开成功
    #[test]
    fn test_strict_false_optional_unknown_metadata_succeeds() {
        let path = temp_vhdx_path();

        File::create(&path)
            .size(1024 * 1024)
            .fixed(true)
            .finish()
            .expect("Failed to create fixed disk");

        inject_unknown_metadata_entry(&path, false);

        let result = File::open(&path)
            .strict(false)
            .log_replay(LogReplayPolicy::Require)
            .finish();
        assert!(
            result.is_ok(),
            "strict=false should allow optional unknown metadata, got: {:?}",
            result.err()
        );
    }

    /// strict=true + optional unknown metadata => 打开失败
    #[test]
    fn test_strict_true_optional_unknown_metadata_fails() {
        let path = temp_vhdx_path();

        File::create(&path)
            .size(1024 * 1024)
            .fixed(true)
            .finish()
            .expect("Failed to create fixed disk");

        inject_unknown_metadata_entry(&path, false);

        let result = File::open(&path)
            .strict(true)
            .log_replay(LogReplayPolicy::Require)
            .finish();
        match result {
            Err(Error::InvalidMetadata(msg)) => {
                assert!(
                    msg.contains("Unknown optional metadata item (strict mode)"),
                    "Error should mention strict mode, got: {msg}"
                );
            }
            Err(e) => panic!("Expected InvalidMetadata with strict mode, got different error: {e}"),
            Ok(_) => panic!("Expected error, but open succeeded"),
        }
    }

    /// strict=false + required unknown metadata => 打开失败
    #[test]
    fn test_strict_false_required_unknown_metadata_fails() {
        let path = temp_vhdx_path();

        File::create(&path)
            .size(1024 * 1024)
            .fixed(true)
            .finish()
            .expect("Failed to create fixed disk");

        inject_unknown_metadata_entry(&path, true);

        let result = File::open(&path)
            .strict(false)
            .log_replay(LogReplayPolicy::Require)
            .finish();
        match result {
            Err(Error::InvalidMetadata(msg)) => {
                assert!(
                    msg.contains("Unknown required metadata item"),
                    "Error should mention required metadata, got: {msg}"
                );
            }
            Err(e) => panic!("Expected InvalidMetadata, got different error: {e}"),
            Ok(_) => panic!("Expected error, but open succeeded"),
        }
    }

    /// 默认打开（strict=true）对未知 optional region 的拒绝行为
    #[test]
    fn test_default_strict_rejects_optional_unknown_region() {
        let path = temp_vhdx_path();

        File::create(&path)
            .size(1024 * 1024)
            .fixed(true)
            .finish()
            .expect("Failed to create fixed disk");

        inject_unknown_region_entry(&path, false);

        // 默认 strict=true
        let result = File::open(&path)
            .log_replay(LogReplayPolicy::Require)
            .finish();
        assert!(
            result.is_err(),
            "Default strict=true should reject optional unknown region"
        );
    }

    // ── update_parent_locator_path 测试 ──────────────────────────────

    /// 创建父盘和指向它的差分盘，打开差分盘后可写更新 relative_path，
    /// 重新打开后验证路径已持久化。
    #[test]
    fn test_update_parent_locator_path_persisted() {
        let parent_path = temp_vhdx_path();
        let child_path = temp_vhdx_path();

        // 创建父盘
        File::create(&parent_path)
            .size(1024 * 1024)
            .fixed(true)
            .finish()
            .expect("Failed to create parent disk");

        // 创建差分盘，指向父盘
        File::create(&child_path)
            .size(1024 * 1024)
            .parent_path(&parent_path)
            .finish()
            .expect("Failed to create child disk");

        // 获取原始的 relative_path
        let orig_path = {
            let orig_child = File::open(&child_path).finish().expect("open child ro");
            let orig_metadata = orig_child.sections().metadata().expect("metadata");
            let orig_items = orig_metadata.items();
            let orig_locator = orig_items.parent_locator().expect("parent_locator");
            orig_locator.resolve_parent_path().expect("orig path")
        };

        // 以可写模式打开并更新路径
        let new_path_str = r"D:\moved_parent.vhdx";
        {
            let child = File::open(&child_path)
                .write()
                .finish()
                .expect("open child writable");
            assert!(child.is_writable());

            child
                .update_parent_locator_path(new_path_str)
                .expect("update_parent_locator_path should succeed");
        }

        // 重新打开（只读），验证路径已更新
        let child = File::open(&child_path).finish().expect("reopen child");
        let metadata = child.sections().metadata().expect("metadata");
        let items = metadata.items();
        let locator = items.parent_locator().expect("parent_locator");
        let updated_path = locator.resolve_parent_path().expect("updated path");

        assert_eq!(updated_path, PathBuf::from(new_path_str));
        assert_ne!(updated_path, orig_path);
    }

    /// 只读模式下调用 update_parent_locator_path 应返回 Error::ReadOnly
    #[test]
    fn test_update_parent_locator_path_read_only_rejected() {
        let parent_path = temp_vhdx_path();
        let child_path = temp_vhdx_path();

        File::create(&parent_path)
            .size(1024 * 1024)
            .fixed(true)
            .finish()
            .expect("Failed to create parent disk");

        File::create(&child_path)
            .size(1024 * 1024)
            .parent_path(&parent_path)
            .finish()
            .expect("Failed to create child disk");

        let child = File::open(&child_path).finish().expect("open child ro");
        assert!(!child.is_writable());

        let result = child.update_parent_locator_path("C:\\foobar.vhdx");
        match result {
            Err(Error::ReadOnly) => {} // expected
            Err(e) => panic!("Expected Error::ReadOnly, got: {e}"),
            Ok(()) => panic!("Expected error but update succeeded"),
        }
    }

    /// 对非差分盘（Fixed）调用 update_parent_locator_path 应返回 Error::InvalidParameter
    #[test]
    fn test_update_parent_locator_path_non_differencing_rejected() {
        let path = temp_vhdx_path();

        let file = File::create(&path)
            .size(1024 * 1024)
            .fixed(true)
            .finish()
            .expect("Failed to create fixed disk");

        assert!(!file.has_parent());
        assert!(file.is_writable());

        let result = file.update_parent_locator_path("C:\\foobar.vhdx");
        match result {
            Err(Error::InvalidParameter(msg)) => {
                assert!(
                    msg.contains("differencing"),
                    "Error should mention differencing disk, got: {msg}"
                );
            }
            Err(e) => panic!("Expected Error::InvalidParameter, got: {e}"),
            Ok(()) => panic!("Expected error but update succeeded"),
        }
    }

    /// 更新为与原始相同的路径应成功（幂等性）
    #[test]
    fn test_update_parent_locator_path_idempotent() {
        let parent_path = temp_vhdx_path();
        let child_path = temp_vhdx_path();

        File::create(&parent_path)
            .size(1024 * 1024)
            .fixed(true)
            .finish()
            .expect("Failed to create parent disk");

        File::create(&child_path)
            .size(1024 * 1024)
            .parent_path(&parent_path)
            .finish()
            .expect("Failed to create child disk");

        let child = File::open(&child_path)
            .write()
            .finish()
            .expect("open child writable");

        // 获取当前 relative_path 路径字符串
        let current_path = {
            let metadata = child.sections().metadata().expect("metadata");
            let items = metadata.items();
            let locator = items.parent_locator().expect("parent_locator");
            locator
                .resolve_parent_path()
                .expect("resolve")
                .to_string_lossy()
                .to_string()
        };

        // 用相同路径调用更新（幂等操作）
        child
            .update_parent_locator_path(&current_path)
            .expect("idempotent update should succeed");

        // 验证路径未变
        let metadata = child.sections().metadata().expect("metadata");
        let items = metadata.items();
        let locator = items.parent_locator().expect("parent_locator");
        let path_after = locator.resolve_parent_path().expect("resolve");

        assert_eq!(
            path_after.to_string_lossy(),
            current_path.as_str(),
            "Path should remain unchanged after idempotent update"
        );
    }
}
