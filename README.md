# 🧹 DiskSpace Free - Windows 磁盘空间清理工具

[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20Linux-blue.svg)](https://www.rust-lang.org)

一个专为 Windows 和 Linux 设计的高效磁盘空间清理工具，使用 Rust 构建，提供美观的终端用户界面和智能的文件清理功能。

## ✨ 功能特性

### 🚀 核心功能
- **内存优化**: 自动释放系统进程的工作集内存
- **智能清理**: 安全清理系统临时文件和缓存
- **实时界面**: 使用 `ratatui` 构建的现代化终端界面
- **并行处理**: 利用多核处理器加速清理过程
- **安全保护**: 智能识别重要文件，避免误删

### 🎯 清理目标

#### 系统临时文件
- **Windows**:
  - 临时文件夹 (`%TEMP%`, `%TMP%`)
  - 系统临时文件 (`C:\Windows\Temp`)
  - 预读取缓存 (`C:\Windows\Prefetch`)
  - 系统日志文件 (`C:\Windows\Logs`)
- **Linux**:
  - 系统日志文件 (`/var/log`)
  - 临时文件夹 (`/tmp`)

#### 更新和下载缓存
- Windows 更新下载缓存 (`SoftwareDistribution\Download`)
- 用户下载文件夹临时文件（仅清理 `.tmp`, `.crdownload` 等）

#### 浏览器缓存
- Microsoft Edge 缓存
- Google Chrome 缓存
- Mozilla Firefox 缓存

#### 系统诊断文件
- 崩溃转储文件 (`CrashDumps`)
- 错误报告队列 (`WER\ReportQueue`)
- 内核转储文件 (`Minidump`, `LiveKernelReports`)

#### 用户缓存
- Windows Store 缓存
- 缩略图缓存
- 最近使用文件记录

#### 回收站清理
- 自动清空系统回收站
- 释放回收站占用的磁盘空间

#### 磁盘日志文件清理 🆕
- **智能扫描**: 自动扫描指定驱动器（C:、D:、E:）上的日志文件
- **安全过滤**: 只清理超过30天且大于1MB的日志文件
- **全面覆盖**: 扫描系统日志、应用程序日志、用户日志等常见位置
- **多格式支持**: 支持 `.log`、`.log.old`、`.log.1-5` 等多种日志文件格式
- **递归搜索**: 深度遍历目录结构，发现隐藏的日志文件

### 🛡️ 安全特性
- **权限检测**: 自动检测管理员权限状态
- **选择性清理**: 下载文件夹仅清理临时文件，保护重要下载
- **实时反馈**: 显示每个清理操作的详细信息
- **智能过滤**: 基于文件类型和位置的智能清理策略

### 📊 系统通知功能
- **完成通知**: 清理完成后自动发送系统通知
- **详细信息**: 显示清理的文件数量和内存优化进程数
- **非阻塞**: 通知在后台发送，不影响程序运行

## 🚀 快速开始

### 系统要求
- Windows 10/11 或 Linux
- Rust 1.70+ (用于编译)

### 安装和运行

1. **克隆仓库**
   ```bash
   git clone https://github.com/oliyo2023/diskspace_free.git
   cd diskspace_free
   ```

2. **编译项目**
   ```bash
   cargo build --release
   ```

3. **运行程序**
   ```bash
   # 普通用户权限运行
   ./target/release/diskspace_free.exe

   # 建议以管理员权限运行以获得最佳清理效果
   ```

### 使用方法
1. 启动程序后，界面会立即显示操作框架
2. 程序自动开始内存释放和文件清理过程
3. 实时查看清理进度和结果
4. 清理完成后会显示系统通知（如果启用）
5. 按 `q` 键退出程序

### 配置说明

程序已内置配置，无需额外配置文件。如需修改配置，请修改源代码中的常量：

#### 通知配置
```rust
const NOTIFICATION_ENABLED: bool = true;               // 启用系统通知
const NOTIFICATION_TIMEOUT: u64 = 5000;               // 通知显示时间(毫秒)
```

#### 日志文件清理配置
```rust
const LOG_SCAN_ENABLED: bool = true;                    // 启用日志扫描
const LOG_SCAN_DRIVES: &[&str] = &["C:", "D:", "E:"];  // 扫描的驱动器
const LOG_MAX_AGE_DAYS: u64 = 30;                      // 只清理超过30天的日志
const LOG_MIN_SIZE_MB: u64 = 1;                        // 只清理大于1MB的日志
```

## 🔧 技术架构

### 核心依赖
- **ratatui** `0.26.1` - 现代化终端用户界面框架
- **crossterm** `0.27.0` - 跨平台终端操作
- **tokio** `1.36.0` - 异步运行时
- **rayon** `1.5` - 数据并行处理
- **windows-sys** `0.60.2` - Windows API 绑定
- **is-admin** `0.1.1` - 权限检测
- **notify-rust** `4.10` - 系统通知


### 性能优化
- 并行文件处理提升清理速度
- 异步操作避免界面阻塞
- 优化的编译配置减小程序体积

## 📊 清理效果

程序可以有效清理：
- ✅ 系统临时文件和缓存
- ✅ 浏览器缓存和临时文件
- ✅ Windows 更新残留文件
- ✅ 应用程序缓存
- ✅ 系统错误报告和转储文件
- ✅ 回收站内容
- ✅ 释放进程工作集内存

## 🤝 贡献指南

欢迎提交 Issue 和 Pull Request！

1. Fork 本仓库
2. 创建功能分支 (`git checkout -b feature/AmazingFeature`)
3. 提交更改 (`git commit -m 'Add some AmazingFeature'`)
4. 推送到分支 (`git push origin feature/AmazingFeature`)
5. 开启 Pull Request

## 📄 许可证

本项目采用 MIT 许可证 - 查看 [LICENSE](LICENSE) 文件了解详情。

## 📞 联系我们

- 🌐 官方网站: [https://www.oliyo.com](https://www.oliyo.com)
- 📧 问题反馈: 通过 GitHub Issues
- 💡 功能建议: 欢迎在 Issues 中提出

## ⚠️ 免责声明

使用本工具前请注意：
- 建议在清理前备份重要数据
- 程序已经过安全性测试，但请谨慎使用
- 建议以管理员权限运行以获得最佳效果
- 作者不对数据丢失承担责任

---

**让您的 Windows 11 系统保持清洁高效！** 🚀