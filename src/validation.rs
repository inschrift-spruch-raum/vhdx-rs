//! VHDX 规范一致性校验模块
//!
//! 本模块提供只读校验入口，用于对已打开的 VHDX 文件执行
//! 结构层面的最小一致性检查。

use crate::File;
use crate::error::{Error, Result};
use crate::file::ParentChainInfo;

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

        let mut has_parent_linkage = false;
        let mut has_path = false;

        for entry in entries {
            let Some(key) = entry.key(data) else {
                continue;
            };
            match key.as_str() {
                "parent_linkage" => has_parent_linkage = true,
                "relative_path" | "volume_path" | "absolute_win32_path" => has_path = true,
                _ => {}
            }
        }

        if !has_parent_linkage {
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

        // 检查 parent_linkage 键是否存在（最小匹配检测）
        let data = locator.key_value_data();
        let entries = locator.entries();
        let mut linkage_matched = false;

        for entry in entries {
            if let Some(key) = entry.key(data) {
                if key == "parent_linkage" {
                    linkage_matched = true;
                    break;
                }
            }
        }

        Ok(ParentChainInfo {
            child: std::path::PathBuf::new(),
            parent,
            linkage_matched,
        })
    }
}
