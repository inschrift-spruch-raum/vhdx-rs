//! VHDX 规范一致性校验模块
//!
//! 本模块提供只读校验入口，用于对已打开的 VHDX 文件执行
//! 结构层面的最小一致性检查。

use crate::File;
use crate::error::{Error, Result};
use crate::file::ParentChainInfo;
use crate::types::Guid;

/// 解析 Parent Locator 中的 GUID 字符串。
///
/// 兼容带花括号和大小写差异的常见表示。
fn parse_locator_guid(value: &str) -> Option<Guid> {
    let trimmed = value.trim().trim_start_matches('{').trim_end_matches('}');
    let parsed = uuid::Uuid::parse_str(trimmed).ok()?;
    let bytes = parsed.as_bytes();

    // uuid::Uuid 字节序为 RFC4122；Guid 内部使用前 3 组小端布局。
    Some(Guid::from_bytes([
        bytes[3], bytes[2], bytes[1], bytes[0], bytes[5], bytes[4], bytes[7], bytes[6], bytes[8],
        bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
    ]))
}

/// 结构化校验问题
///
/// 用于承载可报告的校验问题元信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationIssue {
    /// 问题所属区域
    pub section: &'static str,
    /// 问题代码
    pub code: &'static str,
    /// 人类可读问题描述
    pub message: String,
    /// 规范参考章节
    pub spec_ref: &'static str,
}

/// 规范一致性校验器（只读）
///
/// 该类型绑定到一个已打开的 [`File`]，提供按项与整体校验入口。
pub struct SpecValidator<'a> {
    /// 待校验的 VHDX 文件
    file: &'a File,
}

impl<'a> SpecValidator<'a> {
    /// 从文件句柄创建校验器
    #[must_use]
    pub const fn new(file: &'a File) -> Self {
        Self { file }
    }

    /// 执行全部基础结构校验
    pub fn validate_file(&self) -> Result<()> {
        self.validate_header()?;
        self.validate_region_table()?;
        self.validate_bat()?;
        self.validate_metadata()?;
        self.validate_required_metadata_items()?;
        self.validate_log()?;
        Ok(())
    }

    /// 校验 Header 区域基本可读性
    pub fn validate_header(&self) -> Result<()> {
        let header = self.file.sections().header()?;
        if header.header(0).is_none() {
            return Err(Error::CorruptedHeader(
                "Current header is not available".to_string(),
            ));
        }
        Ok(())
    }

    /// 校验 Region Table 基本可读性
    pub fn validate_region_table(&self) -> Result<()> {
        let header = self.file.sections().header()?;
        if header.region_table(0).is_none() {
            return Err(Error::InvalidRegionTable(
                "Current region table is not available".to_string(),
            ));
        }
        Ok(())
    }

    /// 校验 BAT 区域可读取
    pub fn validate_bat(&self) -> Result<()> {
        let _bat = self.file.sections().bat()?;
        Ok(())
    }

    /// 校验 Metadata 区域可读取
    pub fn validate_metadata(&self) -> Result<()> {
        let _metadata = self.file.sections().metadata()?;
        Ok(())
    }

    /// 校验 required 元数据项存在性
    pub fn validate_required_metadata_items(&self) -> Result<()> {
        let metadata = self.file.sections().metadata()?;
        let items = metadata.items();

        if items.file_parameters().is_none() {
            return Err(Error::InvalidMetadata(
                "Missing required metadata item: file_parameters".to_string(),
            ));
        }

        if items.virtual_disk_size().is_none() {
            return Err(Error::InvalidMetadata(
                "Missing required metadata item: virtual_disk_size".to_string(),
            ));
        }

        if items.virtual_disk_id().is_none() {
            return Err(Error::InvalidMetadata(
                "Missing required metadata item: virtual_disk_id".to_string(),
            ));
        }

        if items.logical_sector_size().is_none() {
            return Err(Error::InvalidMetadata(
                "Missing required metadata item: logical_sector_size".to_string(),
            ));
        }

        if items.physical_sector_size().is_none() {
            return Err(Error::InvalidMetadata(
                "Missing required metadata item: physical_sector_size".to_string(),
            ));
        }

        Ok(())
    }

    /// 校验 Log 区域可读取
    pub fn validate_log(&self) -> Result<()> {
        let _log = self.file.sections().log()?;
        Ok(())
    }

    /// 校验 Parent Locator 的最小键约束
    pub fn validate_parent_locator(&self) -> Result<()> {
        let metadata = self.file.sections().metadata()?;
        let items = metadata.items();
        let Some(file_parameters) = items.file_parameters() else {
            return Err(Error::InvalidMetadata(
                "Missing required metadata item: file_parameters".to_string(),
            ));
        };

        if !file_parameters.has_parent() {
            return Ok(());
        }

        let locator = items.parent_locator().ok_or_else(|| {
            Error::InvalidMetadata("Missing required metadata item: parent_locator".to_string())
        })?;

        let data = locator.key_value_data();
        let entries = locator.entries();

        let mut parent_linkage: Option<Guid> = None;
        let mut has_path = false;

        for entry in entries {
            let Some(key) = entry.key(data) else {
                continue;
            };
            match key.as_str() {
                "parent_linkage" => {
                    let value = entry.value(data).ok_or_else(|| {
                        Error::InvalidMetadata(
                            "Parent locator key parent_linkage has no value".to_string(),
                        )
                    })?;
                    parent_linkage = parse_locator_guid(&value);
                    if parent_linkage.is_none() {
                        return Err(Error::InvalidMetadata(
                            "Parent locator key parent_linkage is not a valid GUID".to_string(),
                        ));
                    }
                }
                "parent_linkage2" => {
                    let value = entry.value(data).ok_or_else(|| {
                        Error::InvalidMetadata(
                            "Parent locator key parent_linkage2 has no value".to_string(),
                        )
                    })?;

                    // parent_linkage2 为可选键：存在时需可解析为 GUID。
                    if parse_locator_guid(&value).is_none() {
                        return Err(Error::InvalidMetadata(
                            "Parent locator key parent_linkage2 is not a valid GUID".to_string(),
                        ));
                    }
                }
                "relative_path" | "volume_path" | "absolute_win32_path" => has_path = true,
                _ => {}
            }
        }

        if parent_linkage.is_none() {
            return Err(Error::InvalidMetadata(
                "Parent locator missing required key: parent_linkage".to_string(),
            ));
        }

        if !has_path {
            return Err(Error::InvalidMetadata(
                "Parent locator must include one path key".to_string(),
            ));
        }

        Ok(())
    }

    /// 差分链校验
    ///
    /// 校验 parent_linkage / parent_linkage2 与父盘 DataWriteGuid 的一致性。
    /// 当前为最小实现：非差分盘返回错误，差分盘返回基本链信息。
    pub fn validate_parent_chain(&self) -> Result<ParentChainInfo> {
        let metadata = self.file.sections().metadata()?;
        let items = metadata.items();

        let file_parameters = items.file_parameters().ok_or_else(|| {
            Error::InvalidMetadata("Missing required metadata item: file_parameters".to_string())
        })?;

        if !file_parameters.has_parent() {
            return Err(Error::InvalidParameter(
                "validate_parent_chain requires a differencing disk".to_string(),
            ));
        }

        let locator = items.parent_locator().ok_or_else(|| {
            Error::InvalidMetadata("Missing required metadata item: parent_locator".to_string())
        })?;

        // 尝试解析父盘路径
        let parent = locator
            .resolve_parent_path()
            .ok_or_else(|| Error::ParentNotFound {
                path: std::path::PathBuf::new(),
            })?;

        // 收集 parent_linkage / parent_linkage2
        let data = locator.key_value_data();
        let entries = locator.entries();
        let mut parent_linkage: Option<Guid> = None;
        let mut parent_linkage2: Option<Guid> = None;

        for entry in entries {
            if let Some(key) = entry.key(data) {
                match key.as_str() {
                    "parent_linkage" => {
                        let value = entry.value(data).ok_or_else(|| {
                            Error::InvalidMetadata(
                                "Parent locator key parent_linkage has no value".to_string(),
                            )
                        })?;
                        parent_linkage = parse_locator_guid(&value);
                    }
                    "parent_linkage2" => {
                        let value = entry.value(data).ok_or_else(|| {
                            Error::InvalidMetadata(
                                "Parent locator key parent_linkage2 has no value".to_string(),
                            )
                        })?;
                        parent_linkage2 = parse_locator_guid(&value);
                    }
                    _ => {}
                }
            }
        }

        let linkage = parent_linkage.ok_or_else(|| {
            Error::InvalidMetadata(
                "Parent locator missing required key: parent_linkage".to_string(),
            )
        })?;

        // 读取父盘 DataWriteGuid 进行链路一致性校验。
        let parent_file = File::open(&parent).finish()?;
        let parent_sections_header = parent_file.sections().header()?;
        let parent_header = parent_sections_header
            .header(0)
            .ok_or_else(|| Error::CorruptedHeader("Current header is not available".to_string()))?;
        let parent_data_write_guid = parent_header.data_write_guid();

        let linkage_matched = parent_data_write_guid == linkage
            || parent_linkage2.is_some_and(|alt| parent_data_write_guid == alt);

        if !linkage_matched {
            return Err(Error::ParentMismatch {
                expected: linkage,
                actual: parent_data_write_guid,
            });
        }

        Ok(ParentChainInfo {
            child: self.file.opened_path().to_path_buf(),
            parent,
            linkage_matched,
        })
    }
}
