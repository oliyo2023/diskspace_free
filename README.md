# 磁盘空间分析器

一个使用 Rust 构建的命令行工具，用于在终端用户界面中分析和显示磁盘空间使用情况。

## 功能特性

- 扫描指定的磁盘或目录。
- 以层级视图显示文件和文件夹。
- 使用 `ratatui` 创建交互式终端用户界面。

## 如何构建和运行

1.  **克隆仓库:**
    ```sh
    git clone https://github.com/oliyo2023/diskspace_free.git
    cd diskspace_free
    ```

2.  **以 release 模式构建项目:**
    ```sh
    cargo build --release
    ```

3.  **运行程序:**
    可执行文件将位于 `target/release/` 目录下。
    ```sh
    ./target/release/diskspace_free
    ```

## 依赖库

本项目依赖于以下优秀的 Rust 库：
- `ratatui`：用于构建终端用户界面。
- `crossterm`：用于终端处理。
- `tokio`：用于异步操作。
- `rayon`：用于并行迭代。
- `is-admin`：用于检查管理员权限。