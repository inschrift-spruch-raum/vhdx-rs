# Phase 6: VHDX 高级功能 - Advanced Features

**目标**: 实现 Trim/UNMAP、快照、压缩等高级功能。

**依赖**: Phase 5 (Differencing Disk)

**优先级**: 低（可选功能，初期可跳过）

---

## 6.1 Trim/UNMAP 支持

**定义**: 通知 VHDX 某些块不再使用，可以释放空间

**用途**: 优化动态磁盘文件大小，回收未使用空间

### 6.1.1 UNMAP 处理

**流程**:
```
1. 接收 UNMAP 请求（扇区范围）
2. 计算涉及的 Block 范围
3. 对每个 Block：
   a. 如果是动态磁盘：
      - 将 BAT Entry 状态改为 PAYLOAD_BLOCK_UNMAPPED
      - 可选：将块数据清零
   b. 如果是差分磁盘：
      - 更新 Sector Bitmap，清除对应位
      - 如果整个 Block 都未映射，释放 Payload Block
4. 通过日志记录 BAT/Sector Bitmap 更新
```

### 6.1.2 Compact（压缩）

**定义**: 重新整理 VHDX 文件，释放未使用的空间

**流程**:
```
1. 扫描 BAT，识别未使用的块（UNMAPPED 或 NOT_PRESENT）
2. 将后续使用的块向前移动，填补空洞
3. 更新 BAT Entries 指向新的位置
4. 截断文件到实际使用大小
```

**注意**: Compact 需要独占访问，文件不能正在使用

### 实现任务

- [ ] 实现 UNMAP 命令处理
- [ ] 实现 BAT 状态转换到 UNMAPPED
- [ ] 实现 Sector Bitmap 位清除
- [ ] （可选）实现 Compact 功能

---

## 6.2 快照（Snapshot）

**定义**: 创建虚拟磁盘的某个时间点的副本

### 6.2.1 基于差分磁盘的快照

**原理**: 创建当前磁盘的差分磁盘作为快照

**流程**:
```
1. 冻结当前磁盘（只读）
2. 创建新的差分磁盘，指向当前磁盘作为父
3. 后续写入到新差分磁盘
4. 恢复快照时，删除差分磁盘，回到原状态
```

### 6.2.2 内部快照（COW - Copy-on-Write）

**原理**: 在单个 VHDX 文件内维护多个快照

**复杂度**: 高，需要自定义元数据结构

**建议**: 初期使用差分磁盘方式实现快照

### 实现任务

- [ ] 实现基于差分磁盘的快照创建
- [ ] 实现快照恢复
- [ ] 实现快照删除
- [ ] 管理快照链

---

## 6.3 压缩（Compression）

**定义**: 透明压缩块数据，减少存储空间

**注意**: 标准 VHDX 格式不支持压缩，这是扩展功能

**实现方式**:
- 在写入 Payload Block 前压缩数据
- 在读取 Payload Block 后解压数据
- 需要自定义元数据标识压缩类型

**风险**: 与标准 VHDX 不兼容

### 实现任务（可选）

- [ ] 设计压缩元数据扩展
- [ ] 实现块级压缩（LZ4/zstd）
- [ ] 实现块级解压
- [ ] 性能优化（压缩/解压缓存）

---

## 6.4 加密（Encryption）

**定义**: 透明加密块数据

**注意**: 标准 VHDX 格式不直接支持加密，通常使用 BitLocker 等外部方案

**实现方式**:
- 写入前加密，读取后解密
- 需要密钥管理

**风险**: 与标准 VHDX 不兼容

### 实现任务（可选）

- [ ] 设计加密元数据扩展
- [ ] 实现块级加密（AES-256-XTS）
- [ ] 实现块级解密
- [ ] 密钥管理

---

## 6.5 性能优化

### 6.5.1 异步 I/O

**Linux**: io_uring
**Windows**: Overlapped I/O / IOCP

### 6.5.2 预读（Read-ahead）

**策略**:
- 顺序读取时预加载下一个 Block
- 根据访问模式动态调整预读大小

### 6.5.3 写合并（Write Coalescing）

**策略**:
- 合并相邻的小写入
- 减少 I/O 次数

### 6.5.4 零拷贝

**策略**:
- 使用 mmap 直接访问文件
- 减少数据复制

### 实现任务（可选）

- [ ] 实现 io_uring 支持（Linux）
- [ ] 实现 Overlapped I/O（Windows）
- [ ] 实现预读策略
- [ ] 实现写合并
- [ ] 评估零拷贝方案

---

## 6.6 工具和命令行

### 6.6.1 VHDX 创建工具

```bash
vhdx-tool create --size 100G --type dynamic disk.vhdx
vhdx-tool create --size 100G --type fixed disk.vhdx
vhdx-tool create --size 100G --type differencing --parent parent.vhdx child.vhdx
```

### 6.6.2 VHDX 信息查看

```bash
vhdx-tool info disk.vhdx
# 输出：大小、类型、块大小、父磁盘等
```

### 6.6.3 VHDX 验证

```bash
vhdx-tool check disk.vhdx
# 验证文件完整性、日志重放等
```

### 6.6.4 Compact 工具

```bash
vhdx-tool compact disk.vhdx
```

### 实现任务

- [ ] 实现 vhdx-tool CLI
- [ ] 实现 create 命令
- [ ] 实现 info 命令
- [ ] 实现 check 命令
- [ ] （可选）实现 compact 命令

---

## Phase 6 完成标准

- [ ] Trim/UNMAP 支持
- [ ] （可选）快照功能
- [ ] （可选）压缩支持
- [ ] （可选）加密支持
- [ ] 性能优化（异步 I/O）
- [ ] CLI 工具实现

## 项目完成总结

完成所有 6 个 Phase 后，项目将具备：

1. **完整的 VHDX 解析能力**: 支持固定、动态、差分三种类型
2. **读写支持**: 完整的块读写和元数据管理
3. **崩溃安全**: 日志和重放机制保证数据一致性
4. **跨平台**: 支持 Linux、macOS、Windows
5. **CLI 工具**: 创建、查看、验证 VHDX 文件
6. **高级功能**: Trim、快照、性能优化

---

## 附录：实现顺序建议

### 最小可行产品（MVP）
1. Phase 1: Header 解析
2. Phase 2: Metadata + BAT（只读）
3. Phase 4: 块读取（固定 + 动态，只读）

### 基础功能
4. Phase 3: 日志系统
5. Phase 4: 块写入

### 完整功能
6. Phase 5: 差分磁盘
7. Phase 6: 高级功能
