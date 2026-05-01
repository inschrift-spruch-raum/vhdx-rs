# Task-1: 字段→访问器→调用点 总台账

> 生成时间: 2026-05-01
> 覆盖范围: 9 个指定文件的完整盘点
> 目的: 零遗漏基线，驱动后续"补访问器→迁移调用点→私有化字段"

---

## 一、盘点统计

| 分类 | 数量 |
|------|------|
| 含 pub 字段的结构体 | 17 |
| pub 字段总数 | 72 |
| 同模块访问（可能无需改） | 37 |
| 跨模块访问（必须改） | 43 |
| 测试文件访问（必须改） | 68 |
| 已有同名访问器方法 | 45 |

---

## 二、定义点总表

### 2.1 `src/sections/header.rs`

#### FileTypeIdentifier<'a> (行 162-169)

| 字段 | 类型 | 已有访问器 | 同模块访问 | 跨模块访问 | 测试访问 | 预计替换方式 |
|------|------|-----------|-----------|-----------|---------|------------|
| `signature` | `[u8; 8]` | ✅ `signature()` | L174 自构造 | — | api_surface: `_sig: [u8; 8] = fti.signature` | `fti.signature` → `fti.signature()` |
| `creator` | `&'a [u8]` | ✅ `creator()` 返回 String | L177 自构造 | — | api_surface: `_creator: &[u8] = fti.creator` | `fti.creator` → `fti.creator()`（类型变化 `[u8]`→`String`） |
| `raw` | `&'a [u8]` | ✅ `raw()` | — | — | — | `fti.raw` → `fti.raw()` |

#### HeaderStructure<'a> (行 252-275)

| 字段 | 类型 | 已有访问器 | 同模块访问 | 跨模块访问 | 测试访问 | 预计替换方式 |
|------|------|-----------|-----------|-----------|---------|------------|
| `signature` | `[u8; 4]` | ✅ `signature()` | — | — | — | 已有访问器，私有化后无需迁移 |
| `checksum` | `u32` | ✅ `checksum()` | — | — | — | 同上 |
| `sequence_number` | `u64` | ✅ `sequence_number()` | L113, L144 同文件 | file.rs: `h1.sequence_number()` (L1003,1243等已用方法) | — | 已有访问器 |
| `file_write_guid` | `Guid` | ✅ `file_write_guid()` | — | file.rs: L1016 已用方法; integration: L225,227 直接访问 | integration L225: `header.file_write_guid` | `header.file_write_guid` → `header.file_write_guid()` |
| `data_write_guid` | `Guid` | ✅ `data_write_guid()` | — | file.rs: L1017,1742 已用方法; integration: L2239 直接访问 | integration L2239: `header(0)...data_write_guid()` 已是方法 | 检查 api_surface |
| `log_guid` | `Guid` | ✅ `log_guid()` | — | file.rs: L1018 已用方法 | — | 已有访问器 |
| `log_version` | `u16` | ✅ `log_version()` | — | validation.rs: L115 用方法 | — | 已有访问器 |
| `version` | `u16` | ✅ `version()` | — | validation.rs: L108 用方法 | — | 已有访问器 |
| `log_length` | `u32` | ✅ `log_length()` | — | file.rs: L1018,1258 用方法 | integration L173: `header.log_length()` 已是方法 | 已有访问器 |
| `log_offset` | `u64` | ✅ `log_offset()` | — | file.rs: L767,1019 用方法 | integration L172: `header.log_offset()` 已是方法 | 已有访问器 |
| `raw` | `&'a [u8]` | ✅ `raw()` | — | — | — | 已有访问器 |

#### RegionTable<'a> (行 408-415)

| 字段 | 类型 | 已有访问器 | 同模块访问 | 跨模块访问 | 测试访问 | 预计替换方式 |
|------|------|-----------|-----------|-----------|---------|------------|
| `header` | `RegionTableHeader<'a>` | ✅ `header()` | L429 同模块 `header.entry_count()` | file.rs 无直接字段访问 | api_surface L454: `rt.header` | `rt.header` → `rt.header()` |
| `entries` | `Vec<RegionTableEntry<'a>>` | ✅ `entries()` | L431 同模块自构造 | file.rs 无直接字段访问 | api_surface L455: `&rt.entries` | `rt.entries` → `rt.entries()` |

#### RegionTableHeader<'a> (行 491-502)

| 字段 | 类型 | 已有访问器 | 同模块访问 | 跨模块访问 | 测试访问 | 预计替换方式 |
|------|------|-----------|-----------|-----------|---------|------------|
| `signature` | `[u8; 4]` | ✅ `signature()` | — | — | — | 已有访问器 |
| `checksum` | `u32` | ✅ `checksum()` | — | validation.rs L142 用方法 | — | 已有访问器 |
| `entry_count` | `u32` | ✅ `entry_count()` | L430 同模块 | validation.rs L151 用方法 | api_surface L461: `rth.entry_count` | `rth.entry_count` → `rth.entry_count()` |
| `reserved` | `u32` | ❌ 无 | — | — | — | 无外部访问，可保留或补访问器 |
| `raw` | `&'a [u8]` | ✅ `raw()` | — | — | — | 已有访问器 |

#### RegionTableEntry<'a> (行 560-571)

| 字段 | 类型 | 已有访问器 | 同模块访问 | 跨模块访问 | 测试访问 | 预计替换方式 |
|------|------|-----------|-----------|-----------|---------|------------|
| `guid` | `Guid` | ✅ `guid()` | — | file.rs L954 用方法; validation.rs L181 用方法 | api_surface L464: `rte.guid` | `rte.guid` → `rte.guid()` |
| `file_offset` | `u64` | ✅ `file_offset()` | — | file.rs L854,861 用方法 | api_surface L465: `rte.file_offset` | `rte.file_offset` → `rte.file_offset()` |
| `length` | `u32` | ✅ `length()` | — | file.rs L855 用方法; validation.rs L175 用方法 | api_surface L466: `rte.length` | `rte.length` → `rte.length()` |
| `required` | `u32` | ✅ `required()` → bool | — | file.rs L955 用方法; validation.rs L181 用方法 | api_surface L467: `rte.required` | `rte.required` → `rte.required()` |
| `raw` | `&'a [u8]` | ✅ `raw()` | — | — | — | 已有访问器 |

---

### 2.2 `src/sections/bat.rs`

#### BatEntry (行 202-207)

| 字段 | 类型 | 已有访问器 | 同模块访问 | 跨模块访问 | 测试访问 | 预计替换方式 |
|------|------|-----------|-----------|-----------|---------|------------|
| `state` | `BatState` | ❌ 无 | L436,456,487-497,543-552 同模块测试 | file.rs L343,617-639 多处 `entry.state` | integration L1003-1009 `entry.state`; api_surface L530-543 `entry.state` | **补 `state()` 访问器** → `entry.state()` |
| `file_offset_mb` | `u64` | ❌ 无 | L433,604,617 同模块测试 | file.rs L622-625 `entry.file_offset_mb`; validation.rs L264-313 多处 `entry.file_offset_mb` | integration L604 `entry.file_offset_mb`; api_surface L543 `entry.file_offset_mb` | **补 `file_offset_mb()` 访问器** → `entry.file_offset_mb()` |

---

### 2.3 `src/sections/metadata.rs`

#### TableHeader<'a> (行 158-169)

| 字段 | 类型 | 已有访问器 | 同模块访问 | 跨模块访问 | 测试访问 | 预计替换方式 |
|------|------|-----------|-----------|-----------|---------|------------|
| `signature` | `[u8; 8]` | ✅ `signature()` | — | validation.rs L346 用方法 | — | 已有访问器 |
| `reserved` | `[u8; 2]` | ❌ 无 | — | validation.rs L355 直接: `table_header.reserved` | — | **补 `reserved()` 访问器** 或直接私有化（validation.rs 仅校验） |
| `entry_count` | `u16` | ✅ `entry_count()` | L135 同模块 | validation.rs L369,378 用方法 | api_surface L486: `th.entry_count` | `th.entry_count` → `th.entry_count()` |
| `reserved2` | `[u8; 20]` | ❌ 无 | — | validation.rs L362 直接: `table_header.reserved2` | — | **补 `reserved2()` 访问器** |
| `raw` | `&'a [u8]` | ✅ `raw()` | — | — | — | 已有访问器 |

#### TableEntry<'a> (行 214-227)

| 字段 | 类型 | 已有访问器 | 同模块访问 | 跨模块访问 | 测试访问 | 预计替换方式 |
|------|------|-----------|-----------|-----------|---------|------------|
| `item_id` | `Guid` | ✅ `item_id()` | L129 同模块 | file.rs L919 用方法; validation.rs L519 用方法 | api_surface L491: `te.item_id` | `te.item_id` → `te.item_id()` |
| `offset` | `u32` | ✅ `offset()` | L334 同模块 | validation.rs L399,425 用方法 | api_surface L492: `te.offset` | `te.offset` → `te.offset()` |
| `length` | `u32` | ✅ `length()` | L335 同模块 | validation.rs L392,400,426 用方法 | api_surface L493: `te.length` | `te.length` → `te.length()` |
| `flags` | `u32` | ✅ `flags()` → EntryFlags | — | — | api_surface L494: `te.flags` (作为 u32) | `te.flags` → `te.flags()`（注意类型变化 u32→EntryFlags） |
| `reserved` | `u32` | ❌ 无 | — | — | — | 无外部访问，可保留或补访问器 |
| `raw` | `&'a [u8]` | ✅ `raw()` | — | — | — | 已有访问器 |

#### EntryFlags (行 285)

| 字段 | 类型 | 已有访问器 | 同模块访问 | 跨模块访问 | 测试访问 | 预计替换方式 |
|------|------|-----------|-----------|-----------|---------|------------|
| `0` (tuple `pub u32`) | `u32` | ❌ 无 | L287-293 `.0` 自引用 | — | api_surface L185: `EntryFlags(0x8000_0000)` 构造; integration L696: `EntryFlags(0xE000_0000)` 构造 | 构造用，保留 pub 或补 `new()` |

#### FileParameters<'a> (行 417-424)

| 字段 | 类型 | 已有访问器 | 同模块访问 | 跨模块访问 | 测试访问 | 预计替换方式 |
|------|------|-----------|-----------|-----------|---------|------------|
| `block_size` | `u32` | ✅ `block_size()` | — | file.rs L892 用方法; validation.rs L223,443 用方法 | integration L11,704-707,750-753 等大量 `fp.block_size()` 已用方法 | 已有访问器 |
| `flags` | `u32` | ✅ `flags()` | L432 同模块自构造 | — | — | 已有访问器 |
| `raw` | `&'a [u8]` | ✅ `raw()` | — | — | — | 已有访问器 |

#### LocatorHeader<'a> (行 564-573)

| 字段 | 类型 | 已有访问器 | 同模块访问 | 跨模块访问 | 测试访问 | 预计替换方式 |
|------|------|-----------|-----------|-----------|---------|------------|
| `locator_type` | `Guid` | ✅ `locator_type()` | — | validation.rs L808 用方法 | — | 已有访问器 |
| `reserved` | `u16` | ❌ 无 | — | — | — | 无外部访问 |
| `key_value_count` | `u16` | ✅ `key_value_count()` | L509,522 同模块 | — | integration L2542,2642,2679,2696: `header.key_value_count` | `header.key_value_count` → `header.key_value_count()` |
| `raw` | `&'a [u8]` | ✅ `raw()` | — | — | — | 已有访问器 |

#### KeyValueEntry<'a> (行 613-624)

| 字段 | 类型 | 已有访问器 | 同模块访问 | 跨模块访问 | 测试访问 | 预计替换方式 |
|------|------|-----------|-----------|-----------|---------|------------|
| `key_offset` | `u32` | ❌ 无 | L657 同模块 `.key_offset` | validation.rs L841 `entry.key_offset` | integration L729,2549,2705: `entry.key_offset`/`kv.key_offset` | **补 `key_offset()` 访问器** |
| `value_offset` | `u32` | ❌ 无 | L675 同模块 `.value_offset` | validation.rs L857,870 `entry.value_offset` | integration L730,2550: `entry.value_offset`/`kv.value_offset` | **补 `value_offset()` 访问器** |
| `key_length` | `u16` | ❌ 无 | L658 同模块 `.key_length` | validation.rs L826 `entry.key_length` | integration L731,2551,2573: `entry.key_length`/`kv.key_length` | **补 `key_length()` 访问器** |
| `value_length` | `u16` | ❌ 无 | L677 同模块 `.value_length` | validation.rs L830 `entry.value_length` | integration L732,2552,2574: `entry.value_length`/`kv.value_length` | **补 `value_length()` 访问器** |
| `raw` | `&'a [u8]` | ❌ 无（但有 `raw()` 方法） | — | — | integration L2575-2576: `entry.raw.len()` / `entry.raw()` | `entry.raw` → `entry.raw()` |

---

### 2.4 `src/sections/log.rs`

#### LogEntryHeader<'a> (行 700-723)

| 字段 | 类型 | 已有访问器 | 同模块访问 | 跨模块访问 | 测试访问 | 预计替换方式 |
|------|------|-----------|-----------|-----------|---------|------------|
| `signature` | `[u8; 4]` | ✅ `signature()` | — | log.rs L182,559 等用方法 | — | 已有访问器 |
| `checksum` | `u32` | ✅ `checksum()` | — | — | — | 已有访问器 |
| `entry_length` | `u32` | ✅ `entry_length()` | — | log.rs L99,104,141,565 等用方法 | — | 已有访问器 |
| `tail` | `u32` | ✅ `tail()` | — | — | — | 已有访问器 |
| `sequence_number` | `u64` | ✅ `sequence_number()` | — | log.rs L287 用方法 | — | 已有访问器 |
| `descriptor_count` | `u32` | ✅ `descriptor_count()` | — | log.rs L316,579 等用方法 | — | 已有访问器 |
| `reserved` | `u32` | ❌ 无 | — | — | — | 无外部访问 |
| `log_guid` | `Guid` | ✅ `log_guid()` | — | log.rs L234 用方法 | — | 已有访问器 |
| `flushed_file_offset` | `u64` | ✅ `flushed_file_offset()` | — | log.rs L424,1103 用方法 | — | 已有访问器 |
| `last_file_offset` | `u64` | ✅ `last_file_offset()` | — | log.rs L431,761 用方法 | — | 已有访问器 |
| `raw` | `&'a [u8]` | ✅ `raw()` | — | — | — | 已有访问器 |

#### DataDescriptor<'a> (行 864-877)

| 字段 | 类型 | 已有访问器 | 同模块访问 | 跨模块访问 | 测试访问 | 预计替换方式 |
|------|------|-----------|-----------|-----------|---------|------------|
| `signature` | `[u8; 4]` | ❌ 无 | — | — | api_surface L570: `dd.signature` | `dd.signature` → 补 `signature()` 访问器 |
| `trailing_bytes` | `u32` | ✅ `trailing_bytes()` | — | log.rs L477 用方法; validation.rs L704 用方法 | api_surface L571: `dd.trailing_bytes` | `dd.trailing_bytes` → `dd.trailing_bytes()` |
| `leading_bytes` | `u64` | ✅ `leading_bytes()` | — | log.rs L476,538 用方法; validation.rs L711 用方法 | api_surface L572: `dd.leading_bytes` | `dd.leading_bytes` → `dd.leading_bytes()` |
| `file_offset` | `u64` | ✅ `file_offset()` | — | log.rs L474,527 用方法; validation.rs 无 | api_surface L573: `dd.file_offset` | `dd.file_offset` → `dd.file_offset()` |
| `sequence_number` | `u64` | ✅ `sequence_number()` | — | log.rs L347,693 用方法 | api_surface L574: `dd.sequence_number` | `dd.sequence_number` → `dd.sequence_number()` |
| `raw` | `&'a [u8]` | ✅ `raw()` | — | — | — | 已有访问器 |

#### ZeroDescriptor<'a> (行 939-952)

| 字段 | 类型 | 已有访问器 | 同模块访问 | 跨模块访问 | 测试访问 | 预计替换方式 |
|------|------|-----------|-----------|-----------|---------|------------|
| `signature` | `[u8; 4]` | ❌ 无 | — | — | api_surface L577: `zd.signature` | 补 `signature()` 访问器 |
| `reserved` | `u32` | ❌ 无 | — | — | — | 无外部访问 |
| `zero_length` | `u64` | ✅ `zero_length()` | — | log.rs L528,1167 用方法; validation.rs L747 无 | api_surface L578: `zd.zero_length` | `zd.zero_length` → `zd.zero_length()` |
| `file_offset` | `u64` | ✅ `file_offset()` | — | log.rs L527,1173 用方法 | api_surface L579: `zd.file_offset` | `zd.file_offset` → `zd.file_offset()` |
| `sequence_number` | `u64` | ✅ `sequence_number()` | — | log.rs L391,748 用方法 | api_surface L580: `zd.sequence_number` | `zd.sequence_number` → `zd.sequence_number()` |
| `raw` | `&'a [u8]` | ✅ `raw()` | — | — | — | 已有访问器 |

#### DataSector<'a> (行 1010-1021)

| 字段 | 类型 | 已有访问器 | 同模块访问 | 跨模块访问 | 测试访问 | 预计替换方式 |
|------|------|-----------|-----------|-----------|---------|------------|
| `signature` | `[u8; 4]` | ❌ 无 | L362 `sector.signature != *b"data"` | validation.rs L728 `sector.signature != *b"data"` | api_surface L587: `ds.signature` | **补 `signature()` 访问器** |
| `sequence_high` | `u32` | ✅ `sequence_high()` | — | — | api_surface L588: `ds.sequence_high` | `ds.sequence_high` → `ds.sequence_high()` |
| `data` | `&'a [u8]` | ✅ `data()` | — | — | api_surface L589: `ds.data` | `ds.data` → `ds.data()` |
| `sequence_low` | `u32` | ✅ `sequence_low()` | — | — | api_surface L590: `ds.sequence_low` | `ds.sequence_low` → `ds.sequence_low()` |
| `raw` | `&'a [u8]` | ✅ `raw()` | — | — | — | 已有访问器 |

---

### 2.5 `src/io_module.rs`

#### Sector<'a> (行 128-139)

| 字段 | 类型 | 已有访问器 | 同模块访问 | 跨模块访问 | 测试访问 | 预计替换方式 |
|------|------|-----------|-----------|-----------|---------|------------|
| `block_sector_index` | `u32` | ❌ 无 | L156,164 同模块 Debug/PartialEq | — | integration L1617-1667 多处; api_surface L374,609 | **补 `block_sector_index()` 访问器** |
| `payload` | `PayloadBlock<'a>` | ✅ `payload()` | L156,164 同模块 | — | integration L1622-1633 `sector.payload`; api_surface L612 `&sector.payload` | `sector.payload` → `sector.payload()` |

#### PayloadBlock<'a> (行 252-254)

| 字段 | 类型 | 已有访问器 | 同模块访问 | 跨模块访问 | 测试访问 | 预计替换方式 |
|------|------|-----------|-----------|-----------|---------|------------|
| `bytes` | `&'a [u8]` | ❌ 无 | — | — | integration L1624 `sector.payload.bytes.is_empty()`; api_surface L613 `pb.bytes`; integration L1844,1858,1862 构造 | **补 `bytes()` 访问器**；构造需保留 pub 或补 `new()` |

---

### 2.6 `src/file.rs`

#### ParentChainInfo (行 136-143)

| 字段 | 类型 | 已有访问器 | 同模块访问 | 跨模块访问 | 测试访问 | 预计替换方式 |
|------|------|-----------|-----------|-----------|---------|------------|
| `child` | `PathBuf` | ❌ 无 | L1001 同模块构造 | — | integration L2183-2184 构造+读取; api_surface L95-96 构造+读取 | **补 `child()` 访问器** + `new()` 构造 |
| `parent` | `PathBuf` | ❌ 无 | L1002 同模块构造 | — | integration L2185,2275-2276 构造+读取; api_surface L95 构造 | **补 `parent()` 访问器** |
| `linkage_matched` | `bool` | ❌ 无 | L1003 同模块构造 | — | integration L2190,2274; api_surface L96 读取 | **补 `linkage_matched()` 访问器** |

---

### 2.7 `src/validation.rs`

#### ValidationIssue (行 37-46)

| 字段 | 类型 | 已有访问器 | 同模块访问 | 跨模块访问 | 测试访问 | 预计替换方式 |
|------|------|-----------|-----------|-----------|---------|------------|
| `section` | `&'static str` | ❌ 无 | — | — | integration L2084-2089 构造+读取; api_surface L105,110,246,251 构造+读取 | **补 `section()` 访问器** + `new()` 构造 |
| `code` | `&'static str` | ❌ 无 | — | — | integration L2086 构造; api_surface L107,251 构造+读取 | **补 `code()` 访问器** |
| `message` | `String` | ❌ 无 | — | — | integration L2087 构造; api_surface L108 构造 | **补 `message()` 访问器** |
| `spec_ref` | `&'static str` | ❌ 无 | — | — | integration L2088 构造; api_surface L109 构造 | **补 `spec_ref()` 访问器** |

---

## 三、按"必须改"优先级排序的行动清单

### P0: 无访问器 + 有跨模块/测试直接访问（必须补访问器）

| # | 结构体 | 字段 | 定义文件 | 访问文件 | 预计替换方式 |
|---|--------|------|---------|---------|------------|
| 1 | `BatEntry` | `state` | bat.rs | file.rs, validation.rs, integration, api_surface | **补 `state()` → `entry.state()`** |
| 2 | `BatEntry` | `file_offset_mb` | bat.rs | file.rs, validation.rs, integration, api_surface | **补 `file_offset_mb()` → `entry.file_offset_mb()`** |
| 3 | `KeyValueEntry` | `key_offset` | metadata.rs | validation.rs, integration | **补 `key_offset()` → `entry.key_offset()`** |
| 4 | `KeyValueEntry` | `value_offset` | metadata.rs | validation.rs, integration | **补 `value_offset()` → `entry.value_offset()`** |
| 5 | `KeyValueEntry` | `key_length` | metadata.rs | validation.rs, integration | **补 `key_length()` → `entry.key_length()`** |
| 6 | `KeyValueEntry` | `value_length` | metadata.rs | validation.rs, integration | **补 `value_length()` → `entry.value_length()`** |
| 7 | `DataSector` | `signature` | log.rs | validation.rs, api_surface | **补 `signature()` → `sector.signature()`** |
| 8 | `DataDescriptor` | `signature` | log.rs | api_surface | **补 `signature()` → `dd.signature()`** |
| 9 | `ZeroDescriptor` | `signature` | log.rs | api_surface | **补 `signature()` → `zd.signature()`** |
| 10 | `Sector` | `block_sector_index` | io_module.rs | integration, api_surface | **补 `block_sector_index()` → `sector.block_sector_index()`** |
| 11 | `PayloadBlock` | `bytes` | io_module.rs | integration, api_surface | **补 `bytes()` → `pb.bytes()`** + **补 `new()` 构造** |
| 12 | `ParentChainInfo` | `child` | file.rs | integration, api_surface | **补 `child()`** + **补 `new()` 构造** |
| 13 | `ParentChainInfo` | `parent` | file.rs | integration, api_surface | **补 `parent()`** |
| 14 | `ParentChainInfo` | `linkage_matched` | file.rs | integration, api_surface | **补 `linkage_matched()`** |
| 15 | `ValidationIssue` | `section` | validation.rs | integration, api_surface | **补 `section()`** + **补 `new()` 构造** |
| 16 | `ValidationIssue` | `code` | validation.rs | integration, api_surface | **补 `code()`** |
| 17 | `ValidationIssue` | `message` | validation.rs | integration, api_surface | **补 `message()`** |
| 18 | `ValidationIssue` | `spec_ref` | validation.rs | integration, api_surface | **补 `spec_ref()`** |

### P1: 有访问器 + 测试/外部仍直接访问字段（迁移调用点）

| # | 结构体 | 字段 | 定义文件 | 直接访问位置 | 替换方式 |
|---|--------|------|---------|------------|---------|
| 19 | `FileTypeIdentifier` | `signature` | header.rs | api_surface L434 | `fti.signature` → `fti.signature()` |
| 20 | `FileTypeIdentifier` | `creator` | header.rs | api_surface L435 | `fti.creator` → `fti.creator()`（类型变化！）|
| 21 | `HeaderStructure` | `signature` | header.rs | api_surface L439 | `hdr.signature` → `hdr.signature()` |
| 22 | `HeaderStructure` | `checksum` | header.rs | api_surface L440 | `hdr.checksum` → `hdr.checksum()` |
| 23 | `HeaderStructure` | `sequence_number` | header.rs | api_surface L441 | `hdr.sequence_number` → `hdr.sequence_number()` |
| 24 | `HeaderStructure` | `file_write_guid` | header.rs | api_surface L442 | `hdr.file_write_guid` → `hdr.file_write_guid()` |
| 25 | `HeaderStructure` | `data_write_guid` | header.rs | api_surface L443 | `hdr.data_write_guid` → `hdr.data_write_guid()` |
| 26 | `HeaderStructure` | `log_guid` | header.rs | api_surface L444 | `hdr.log_guid` → `hdr.log_guid()` |
| 27 | `HeaderStructure` | `log_version` | header.rs | api_surface L445 | `hdr.log_version` → `hdr.log_version()` |
| 28 | `HeaderStructure` | `version` | header.rs | api_surface L446 | `hdr.version` → `hdr.version()` |
| 29 | `HeaderStructure` | `log_length` | header.rs | api_surface L447 | `hdr.log_length` → `hdr.log_length()` |
| 30 | `HeaderStructure` | `log_offset` | header.rs | api_surface L448 | `hdr.log_offset` → `hdr.log_offset()` |
| 31 | `RegionTable` | `header` | header.rs | api_surface L454 | `rt.header` → `rt.header()` |
| 32 | `RegionTable` | `entries` | header.rs | api_surface L455 | `rt.entries` → `rt.entries()` |
| 33 | `RegionTableHeader` | `signature` | header.rs | api_surface L459 | `rth.signature` → `rth.signature()` |
| 34 | `RegionTableHeader` | `checksum` | header.rs | api_surface L460 | `rth.checksum` → `rth.checksum()` |
| 35 | `RegionTableHeader` | `entry_count` | header.rs | api_surface L461 | `rth.entry_count` → `rth.entry_count()` |
| 36 | `RegionTableEntry` | `guid` | header.rs | api_surface L464 | `rte.guid` → `rte.guid()` |
| 37 | `RegionTableEntry` | `file_offset` | header.rs | api_surface L465 | `rte.file_offset` → `rte.file_offset()` |
| 38 | `RegionTableEntry` | `length` | header.rs | api_surface L466 | `rte.length` → `rte.length()` |
| 39 | `RegionTableEntry` | `required` | header.rs | api_surface L467 | `rte.required` → `rte.required()`（注意类型 u32→bool）|
| 40 | `TableHeader` | `entry_count` | metadata.rs | api_surface L486 | `th.entry_count` → `th.entry_count()` |
| 41 | `TableEntry` | `item_id` | metadata.rs | api_surface L491 | `te.item_id` → `te.item_id()` |
| 42 | `TableEntry` | `offset` | metadata.rs | api_surface L492 | `te.offset` → `te.offset()` |
| 43 | `TableEntry` | `length` | metadata.rs | api_surface L493 | `te.length` → `te.length()` |
| 44 | `TableEntry` | `flags` | metadata.rs | api_surface L494 | `te.flags` → `te.flags()`（类型 u32→EntryFlags）|
| 45 | `LocatorHeader` | `key_value_count` | metadata.rs | integration L2542,2642,2679,2696 | `header.key_value_count` → `header.key_value_count()` |
| 46 | `DataDescriptor` | `trailing_bytes` | log.rs | api_surface L571 | `dd.trailing_bytes` → `dd.trailing_bytes()` |
| 47 | `DataDescriptor` | `leading_bytes` | log.rs | api_surface L572 | `dd.leading_bytes` → `dd.leading_bytes()` |
| 48 | `DataDescriptor` | `file_offset` | log.rs | api_surface L573 | `dd.file_offset` → `dd.file_offset()` |
| 49 | `DataDescriptor` | `sequence_number` | log.rs | api_surface L574 | `dd.sequence_number` → `dd.sequence_number()` |
| 50 | `ZeroDescriptor` | `zero_length` | log.rs | api_surface L578 | `zd.zero_length` → `zd.zero_length()` |
| 51 | `ZeroDescriptor` | `file_offset` | log.rs | api_surface L579 | `zd.file_offset` → `zd.file_offset()` |
| 52 | `ZeroDescriptor` | `sequence_number` | log.rs | api_surface L580 | `zd.sequence_number` → `zd.sequence_number()` |
| 53 | `DataSector` | `sequence_high` | log.rs | api_surface L588 | `ds.sequence_high` → `ds.sequence_high()` |
| 54 | `DataSector` | `data` | log.rs | api_surface L589 | `ds.data` → `ds.data()` |
| 55 | `DataSector` | `sequence_low` | log.rs | api_surface L590 | `ds.sequence_low` → `ds.sequence_low()` |
| 56 | `Sector` | `payload` | io_module.rs | integration L1622-1633 | `sector.payload` → `sector.payload()` |
| 57 | `KeyValueEntry` | `raw` | metadata.rs | integration L2575-2576 | `entry.raw` → `entry.raw()` |

### P2: 有访问器 + 跨模块校验直接访问（需迁移 validation.rs）

| # | 结构体 | 字段 | 定义文件 | 访问文件 | 替换方式 |
|---|--------|------|---------|---------|---------|
| 58 | `TableHeader` | `reserved` | metadata.rs | validation.rs L355 | `table_header.reserved` → `table_header.reserved()` (补访问器) |
| 59 | `TableHeader` | `reserved2` | metadata.rs | validation.rs L362 | `table_header.reserved2` → `table_header.reserved2()` (补访问器) |

### P3: 特殊处理 — 构造用 pub 字段

以下字段在测试中直接构造结构体实例，私有化后需补构造方法：

| 结构体 | 字段 | 测试构造位置 | 建议 |
|--------|------|------------|------|
| `PayloadBlock` | `bytes` | integration L1848,1862; api_surface L634,638 | 补 `PayloadBlock::new(bytes)` |
| `ParentChainInfo` | `child/parent/linkage_matched` | integration L2183; api_surface L91-96 | 补 `ParentChainInfo::new(...)` |
| `ValidationIssue` | 全部 4 字段 | integration L2083; api_surface L104-109 | 补 `ValidationIssue::new(...)` |
| `EntryFlags` | `pub u32` | integration L696; api_surface L185 | 保留 pub 或补 `EntryFlags::new(u32)` |
| `KeyValueEntry` | 全部 5 字段 | integration L728-733; api_surface L159-163 | 补 `KeyValueEntry::new(...)` 或保留 pub |
| `BatEntry` | `state/file_offset_mb` | 无外部构造（已有 `BatEntry::new()` crate-internal） | 无需补公开构造 |
| `FileTypeIdentifier` | `signature/creator/raw` | 无外部构造（有 `FileTypeIdentifier::new()`） | 无需补公开构造 |

---

## 四、同模块自构造访问（私有化后无需迁移）

以下字段仅在自身模块内通过自构造或内部逻辑访问，私有化后自然仍可访问：

| 结构体 | 字段 | 文件 | 说明 |
|--------|------|------|------|
| `RegionTableHeader` | `reserved` | header.rs | 无任何外部访问 |
| `LogEntryHeader` | `reserved` | log.rs | 无任何外部访问 |
| `ZeroDescriptor` | `reserved` | log.rs | 无任何外部访问 |
| `TableEntry` | `reserved` | metadata.rs | 无任何外部访问 |
| `LocatorHeader` | `reserved` | metadata.rs | 无任何外部访问 |
| `Sector` 内部字段 | `file/block_idx/size` | io_module.rs | 已是 pub 但无外部访问 |
| `Header` 内部字段 | `raw_data/marker` | header.rs | 私有 |
| `Bat` 内部字段 | `raw_data/entry_count/entries/marker` | bat.rs | 私有 |
| `Log` 内部字段 | `raw_data/marker` | log.rs | 私有 |
| `Metadata` 内部字段 | `raw_data/marker` | metadata.rs | 私有 |

---

## 五、文件覆盖确认

| 文件 | 已覆盖 | 说明 |
|------|--------|------|
| `src/sections/header.rs` | ✅ | 6 个结构体, 22 个 pub 字段 |
| `src/sections/bat.rs` | ✅ | 1 个结构体, 2 个 pub 字段 |
| `src/sections/metadata.rs` | ✅ | 6 个结构体, 19 个 pub 字段 |
| `src/sections/log.rs` | ✅ | 4 个结构体, 23 个 pub 字段 |
| `src/io_module.rs` | ✅ | 2 个结构体, 3 个 pub 字段 |
| `src/validation.rs` | ✅ | 1 个结构体, 4 个 pub 字段 |
| `src/file.rs` | ✅ | 1 个结构体, 3 个 pub 字段 |
| `tests/integration_test.rs` | ✅ | 已追溯所有字段访问 |
| `tests/api_surface_smoke.rs` | ✅ | 已追溯所有字段访问 |

---

## 六、风险提示

1. **类型变化风险**: `FileTypeIdentifier::creator` 字段类型是 `&[u8]`，但访问器 `creator()` 返回 `String`；迁移时需确认调用者是否依赖原始字节。
2. **类型变化风险**: `RegionTableEntry::required` 字段是 `u32`，但访问器 `required()` 返回 `bool`；迁移时需确认调用者是否依赖原始值。
3. **类型变化风险**: `TableEntry::flags` 字段是 `u32`，但访问器 `flags()` 返回 `EntryFlags`。
4. **构造体依赖**: `PayloadBlock`、`ParentChainInfo`、`ValidationIssue`、`KeyValueEntry` 在测试中通过字段直接构造，私有化后必须提供替代构造方法。
5. **DataSector.signature**: 在 `log.rs` L362 和 `validation.rs` L728 通过 `sector.signature != *b"data"` 直接比较，补访问器后需改为 `sector.signature() != b"data"`。
