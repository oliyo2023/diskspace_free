# PocketBase 数据库设置指南

本文档说明如何设置 PocketBase 数据库来存储磁盘清理记录。

## 1. 安装和启动 PocketBase

1. 从 [PocketBase 官网](https://pocketbase.io/) 下载适合您系统的版本
2. 解压并运行 PocketBase：
   ```bash
   ./pocketbase serve
   ```
3. 访问 `http://localhost:8090/_/` 进入管理界面

## 2. 创建集合 (Collection)

在 PocketBase 管理界面中创建一个名为 `cleanup_records` 的集合，包含以下字段：

### 字段配置

| 字段名 | 类型 | 必填 | 描述 |
|--------|------|------|------|
| `id` | Text | 是 | 记录唯一标识符 |
| `computer_name` | Text | 是 | 计算机名称 |
| `cleanup_time` | Text | 是 | 清理时间 (ISO 8601 格式) |
| `files_cleaned_count` | Number | 是 | 清理的文件/目录数量 |
| `memory_processes_count` | Number | 是 | 内存优化的进程数量 |
| `cleaned_files` | Text | 否 | 清理的文件列表 (JSON 格式) |
| `cleanup_paths` | Text | 否 | 清理的路径列表 (JSON 格式) |
| `is_admin` | Bool | 是 | 是否以管理员权限运行 |
| `total_duration_seconds` | Number | 是 | 清理总耗时（秒） |

### 详细字段设置

#### 1. id (Text)
- 类型：Text
- 必填：是
- 最小长度：1
- 最大长度：50
- 描述：UUID 格式的唯一标识符

#### 2. computer_name (Text)
- 类型：Text
- 必填：是
- 最小长度：1
- 最大长度：100
- 描述：运行清理程序的计算机名称

#### 3. cleanup_time (Text)
- 类型：Text
- 必填：是
- 格式：ISO 8601 日期时间字符串
- 示例：`2024-01-15T10:30:00Z`

#### 4. files_cleaned_count (Number)
- 类型：Number
- 必填：是
- 最小值：0
- 描述：本次清理删除的文件和目录总数

#### 5. memory_processes_count (Number)
- 类型：Number
- 必填：是
- 最小值：0
- 描述：本次内存优化处理的进程数量

#### 6. cleaned_files (Text)
- 类型：Text
- 必填：否
- 描述：清理的文件列表，JSON 数组格式
- 示例：`["C:\\temp\\file1.tmp", "C:\\temp\\file2.log"]`

#### 7. cleanup_paths (Text)
- 类型：Text
- 必填：否
- 描述：扫描的清理路径列表，JSON 数组格式
- 示例：`["C:\\Windows\\Temp", "C:\\Users\\User\\Downloads"]`

#### 8. is_admin (Bool)
- 类型：Bool
- 必填：是
- 描述：程序是否以管理员权限运行

#### 9. total_duration_seconds (Number)
- 类型：Number
- 必填：是
- 最小值：0
- 描述：从开始到完成的总耗时（秒）

## 3. 配置程序

1. 复制 `config.example.toml` 为 `config.toml`
2. 修改配置文件中的 PocketBase URL：
   ```toml
   [pocketbase]
   url = "http://localhost:8090"  # 您的 PocketBase 服务器地址
   collection = "cleanup_records"
   enabled = true
   timeout = 30
   ```

## 4. 权限设置（可选）

如果需要公开访问或特定权限控制，可以在 PocketBase 管理界面中设置集合的访问权限：

- **List/Search**: 根据需要设置
- **View**: 根据需要设置  
- **Create**: 允许匿名创建（如果程序不使用认证）
- **Update**: 根据需要设置
- **Delete**: 根据需要设置

## 5. 数据查询示例

### 查询最近的清理记录
```javascript
// 获取最近 10 条记录
pb.collection('cleanup_records').getList(1, 10, {
    sort: '-cleanup_time',
});
```

### 按计算机名查询
```javascript
// 查询特定计算机的记录
pb.collection('cleanup_records').getList(1, 50, {
    filter: 'computer_name = "DESKTOP-ABC123"',
    sort: '-cleanup_time',
});
```

### 统计查询
```javascript
// 获取总清理文件数
pb.collection('cleanup_records').getList(1, 1, {
    filter: 'files_cleaned_count > 0',
});
```

## 6. 故障排除

### 常见问题

1. **连接失败**
   - 检查 PocketBase 服务是否正在运行
   - 确认 URL 配置正确
   - 检查防火墙设置

2. **权限错误**
   - 确认集合的创建权限设置
   - 检查是否需要认证

3. **字段错误**
   - 确认所有必填字段都已正确配置
   - 检查字段类型是否匹配

### 日志查看

程序运行时会在控制台输出上传状态：
- 成功：`清理记录已成功上传到PocketBase`
- 失败：`上传到PocketBase失败: [错误信息]`

## 7. 数据备份

建议定期备份 PocketBase 数据：
```bash
# 备份数据库文件
cp pb_data/data.db backup/data_$(date +%Y%m%d).db
```

## 8. 生产环境部署

对于生产环境，建议：
1. 使用 HTTPS
2. 设置适当的权限控制
3. 配置数据备份策略
4. 监控数据库性能
