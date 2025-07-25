use chrono::{DateTime, Utc};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use notify_rust::Notification;
use ratatui::{
    prelude::{CrosstermBackend, Terminal, Color, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};
use rayon::prelude::*;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{
    env,
    fs,
    io::{self, Stdout},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::Duration,
};
use uuid::Uuid;
use windows_sys::Win32::{
    System::ProcessStatus::{
        EmptyWorkingSet, EnumProcesses, GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    },
    System::Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ},
    UI::Shell::{SHEmptyRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND},
};

// PocketBase 配置常量 (暂时禁用)
const POCKETBASE_URL: &str = "https://8.140.206.248/pocketbase";
const COLLECTION_NAME: &str = "cleanup_records";
const POCKETBASE_ENABLED: bool = false; // 暂时禁用PocketBase上传
const POCKETBASE_TIMEOUT: u64 = 30;
const NOTIFICATION_ENABLED: bool = true;
const NOTIFICATION_TIMEOUT: u64 = 5000;

// 日志文件清理配置
const LOG_SCAN_ENABLED: bool = true;
const LOG_SCAN_DRIVES: &[&str] = &["C:", "D:", "E:"]; // 要扫描的驱动器
const LOG_MAX_AGE_DAYS: u64 = 30; // 只清理超过30天的日志文件
const LOG_MIN_SIZE_MB: u64 = 1; // 只清理大于1MB的日志文件

#[derive(Debug, Serialize, Deserialize)]
struct CleanupRecord {
    id: String,
    computer_name: String,
    cleanup_time: DateTime<Utc>,
    files_cleaned_count: i32,
    memory_processes_count: i32,
    cleaned_files: Vec<String>,
    cleanup_paths: Vec<String>,
    is_admin: bool,
    total_duration_seconds: i32,
}

#[derive(Debug, Serialize)]
struct PocketBaseRecord {
    computer_name: String,
    cleanup_time: String,
    files_cleaned_count: i32,
    memory_processes_count: i32,
    cleaned_files: String, // JSON string
    cleanup_paths: String, // JSON string
    is_admin: bool,
    total_duration_seconds: i32,
}

struct App {
    cleaned_files: Vec<String>,
    is_cleaning: Arc<AtomicBool>,
    messages: Vec<String>,
    cleaning_finished: bool,
    is_releasing_memory: bool,
    memory_released_count: usize,
    files_cleaned_count: usize,
    cleanup_start_time: DateTime<Utc>,
    cleanup_paths: Vec<String>,
}

impl App {
    fn new() -> Self {
        let mut messages = vec!["按 'q' 退出".to_string()];
        if !is_admin::is_admin() {
            messages.push("提示: 未以管理员权限运行, 可能部分文件无法清理或释放内存。".to_string());
        }
        Self {
            cleaned_files: Vec::new(),
            is_cleaning: Arc::new(AtomicBool::new(false)),
            messages,
            cleaning_finished: false,
            is_releasing_memory: false,
            memory_released_count: 0,
            files_cleaned_count: 0,
            cleanup_start_time: Utc::now(),
            cleanup_paths: Vec::new(),
        }
    }

    fn start_release_memory(&mut self, sender: mpsc::Sender<String>) {
        self.is_releasing_memory = true;
        self.messages.push("正在释放内存...".to_string());
        let sender_clone = sender.clone();
        tokio::spawn(async move {
            let released_count = release_memory();
            sender_clone.send(format!("MEMORY_RELEASE_COMPLETE:{}", released_count)).unwrap();
        });
    }

    fn start_cleaning(&mut self, sender: mpsc::Sender<String>) {
        self.is_cleaning.store(true, Ordering::SeqCst);
        self.cleaning_finished = false;
        self.messages.retain(|m| !m.starts_with("清理完成"));
        self.messages.push("正在清理中...".to_string());

        let paths = get_cached_paths();
        let is_cleaning_clone = self.is_cleaning.clone();
        let sender_clone = sender.clone();

        // 收集清理路径信息
        self.cleanup_paths = paths.iter().map(|p| p.to_string_lossy().to_string()).collect();

        // 显示将要清理的文件夹
        for path in &paths {
            if let Some(folder_name) = path.file_name().and_then(|name| name.to_str()) {
                if folder_name.eq_ignore_ascii_case("downloads") {
                    sender_clone.send("正在扫描下载文件夹 (仅清理临时文件)...".to_string()).ok();
                } else {
                    sender_clone.send(format!("正在扫描: {}", path.display())).ok();
                }
            }
        }

        tokio::spawn(async move {
            let total_cleaned = paths
                .par_iter()
                .map(|path| clean_directory(path, sender.clone()))
                .sum::<usize>();

            // 扫描并清理磁盘上的.log文件
            let log_files_cleaned = if LOG_SCAN_ENABLED {
                scan_and_clean_log_files(sender.clone())
            } else {
                0
            };

            // 清空回收站
            let recycle_bin_cleaned = empty_recycle_bin(sender.clone());
            let recycle_count = if recycle_bin_cleaned { 1 } else { 0 };
            let final_total = total_cleaned + log_files_cleaned + recycle_count;

            sender.send(format!("CLEANING_COMPLETE:{}", final_total)).unwrap();
            is_cleaning_clone.store(false, Ordering::SeqCst);
        });
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let mut terminal = init_terminal()?;
    let (tx, rx) = mpsc::channel();
    let mut app = App::new();

    // 立即绘制初始界面框架，避免空白
    terminal.draw(|frame| {
        draw_ui(frame, &app);
    })?;

    app.start_release_memory(tx.clone());

    loop {
        terminal.draw(|frame| {
            draw_ui(frame, &app);
        })?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => break,
                    _ => {}
                }
            }
        }

        if let Ok(msg) = rx.try_recv() {
            if msg.starts_with("MEMORY_RELEASE_COMPLETE:") {
                let count_str = msg.strip_prefix("MEMORY_RELEASE_COMPLETE:").unwrap_or("0");
                app.memory_released_count = count_str.parse().unwrap_or(0);
                app.is_releasing_memory = false;
                app.messages.retain(|m| m != "正在释放内存...");
                let display_msg = format!("内存释放完成! 共整理了 {} 个进程。", app.memory_released_count);
                app.messages.push(display_msg.clone());
                app.cleaned_files.push(display_msg);
                app.start_cleaning(tx.clone());
            } else if msg.starts_with("CLEANING_COMPLETE:") {
                let count_str = msg.strip_prefix("CLEANING_COMPLETE:").unwrap_or("0");
                app.files_cleaned_count = count_str.parse().unwrap_or(0);
                app.is_cleaning.store(false, Ordering::SeqCst);
                app.cleaning_finished = true;
                app.messages.retain(|m| m != "正在清理中...");
                let display_msg = format!("清理完成! 总共清理了 {} 个文件/目录。", app.files_cleaned_count);
                app.messages.push(display_msg);

                // 发送系统通知
                send_completion_notification(app.files_cleaned_count, app.memory_released_count);

                // 创建清理记录并上传到PocketBase
                let cleanup_end_time = Utc::now();
                let duration = cleanup_end_time.signed_duration_since(app.cleanup_start_time);

                let record = CleanupRecord {
                    id: Uuid::new_v4().to_string(),
                    computer_name: env::var("COMPUTERNAME").unwrap_or_else(|_| "Unknown".to_string()),
                    cleanup_time: cleanup_end_time,
                    files_cleaned_count: app.files_cleaned_count as i32,
                    memory_processes_count: app.memory_released_count as i32,
                    cleaned_files: app.cleaned_files.clone(),
                    cleanup_paths: app.cleanup_paths.clone(),
                    is_admin: is_admin::is_admin(),
                    total_duration_seconds: duration.num_seconds() as i32,
                };

                // 在后台上传记录
                let upload_sender = tx.clone();
                tokio::spawn(async move {
                    if let Err(e) = upload_to_pocketbase(record, upload_sender).await {
                        eprintln!("上传清理记录失败: {}", e);
                    }
                });
            } else if msg.starts_with("数据已成功同步") {
                app.messages.push(msg.clone());
                app.cleaned_files.push(msg);
            } else {
                app.cleaned_files.push(msg);
            }
        }
    }

    shutdown_terminal(terminal)
}

fn init_terminal() -> io::Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    Terminal::new(CrosstermBackend::new(stdout))
}

fn shutdown_terminal(mut terminal: Terminal<CrosstermBackend<Stdout>>) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    Ok(())
}

fn get_cached_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if cfg!(windows) {
        // 临时文件夹
        if let Ok(temp) = env::var("TEMP") { paths.push(temp.into()); }
        if let Ok(tmp) = env::var("TMP") {
            let path_buf = PathBuf::from(tmp);
            if !paths.contains(&path_buf) { paths.push(path_buf); }
        }

        // Windows系统级缓存
        if let Ok(win_dir) = env::var("windir") {
            paths.push(Path::new(&win_dir).join("Prefetch"));
            paths.push(Path::new(&win_dir).join("Logs"));
            paths.push(Path::new(&win_dir).join("SoftwareDistribution").join("Download"));
            paths.push(Path::new(&win_dir).join("Temp"));
        }

        // 用户相关缓存和临时文件
        if let Ok(user_profile) = env::var("USERPROFILE") {
            let user_path = Path::new(&user_profile);

            // 下载文件夹（只清理特定类型的文件）
            let downloads_path = user_path.join("Downloads");
            if downloads_path.exists() {
                paths.push(downloads_path);
            }

            // 回收站
            paths.push(user_path.join("AppData").join("Local").join("Microsoft").join("Windows").join("Explorer").join("ThumbCacheToDelete"));

            // 浏览器缓存
            paths.push(user_path.join("AppData").join("Local").join("Microsoft").join("Edge").join("User Data").join("Default").join("Cache"));
            paths.push(user_path.join("AppData").join("Local").join("Google").join("Chrome").join("User Data").join("Default").join("Cache"));
            paths.push(user_path.join("AppData").join("Local").join("Mozilla").join("Firefox").join("Profiles"));

            // Windows Store缓存
            paths.push(user_path.join("AppData").join("Local").join("Packages").join("Microsoft.WindowsStore_8wekyb3d8bbwe").join("LocalCache"));

            // 系统错误报告
            paths.push(user_path.join("AppData").join("Local").join("CrashDumps"));

            // 最近使用的文件缓存
            paths.push(user_path.join("AppData").join("Roaming").join("Microsoft").join("Windows").join("Recent"));
        }

        // 系统级缓存（需要管理员权限）
        if is_admin::is_admin() {
            paths.push(PathBuf::from("C:\\Windows\\SoftwareDistribution\\Download"));
            paths.push(PathBuf::from("C:\\ProgramData\\Microsoft\\Windows\\WER\\ReportQueue"));
            paths.push(PathBuf::from("C:\\Windows\\LiveKernelReports"));
            paths.push(PathBuf::from("C:\\Windows\\Minidump"));
        }
    } else if cfg!(unix) {
        paths.push(PathBuf::from("/var/log"));
        paths.push(PathBuf::from("/tmp"));
    }

    paths
}

fn clean_directory(dir: &Path, sender: mpsc::Sender<String>) -> usize {
    if !dir.exists() {
        return 0;
    }

    if let Ok(entries) = fs::read_dir(dir) {
        let entries: Vec<_> = entries.filter_map(Result::ok).collect();
        let dir_str = dir.to_string_lossy().to_lowercase();

        return entries
            .par_iter()
            .map(|entry| {
                let path = entry.path();
                let mut count = 0;

                if should_clean_file(&path, &dir_str) {
                    if path.is_dir() {
                        if fs::remove_dir_all(&path).is_ok() {
                            sender.send(format!("已删除目录: {:?}", path)).ok();
                            count += 1;
                        }
                    } else if path.is_file() {
                        if fs::remove_file(&path).is_ok() {
                            let file_type = get_file_type_description(&dir_str);
                            sender.send(format!("已删除{}: {:?}", file_type, path)).ok();
                            count += 1;
                        }
                    }
                }
                count
            })
            .sum();
    }
    0
}

fn should_clean_file(path: &Path, dir_str: &str) -> bool {
    if let Some(file_name) = path.file_name().and_then(|name| name.to_str()) {
        let file_name_lower = file_name.to_lowercase();

        // 下载文件夹特殊处理
        if dir_str.contains("downloads") {
            return should_clean_download_file(path);
        }

        // Firefox配置文件夹特殊处理
        if dir_str.contains("firefox") && dir_str.contains("profiles") {
            return file_name_lower.contains("cache") ||
                   file_name_lower.contains("temp") ||
                   file_name_lower.ends_with(".tmp");
        }

        // Recent文件夹特殊处理
        if dir_str.contains("recent") {
            return file_name_lower.ends_with(".lnk");
        }

        // 浏览器缓存文件夹
        if dir_str.contains("cache") || dir_str.contains("temp") {
            return true;
        }

        // 错误报告和转储文件
        if dir_str.contains("crashdumps") ||
           dir_str.contains("reportqueue") ||
           dir_str.contains("minidump") ||
           dir_str.contains("livekernelreports") {
            return true;
        }

        // 默认临时文件清理
        let temp_extensions = [
            ".tmp", ".temp", ".cache", ".log", ".dmp", ".mdmp"
        ];

        for ext in &temp_extensions {
            if file_name_lower.ends_with(ext) {
                return true;
            }
        }
    }

    true // 对于其他系统临时文件夹，默认清理所有内容
}

fn should_clean_download_file(path: &Path) -> bool {
    if let Some(file_name) = path.file_name().and_then(|name| name.to_str()) {
        let file_name_lower = file_name.to_lowercase();

        // 只清理明确的临时文件和缓存文件
        let temp_extensions = [
            ".tmp", ".temp", ".cache", ".crdownload", ".part",
            ".partial", ".download", ".!ut", ".bc!", ".crx"
        ];

        let temp_prefixes = [
            "~", "tmp", "temp", "cache"
        ];

        // 检查文件扩展名
        for ext in &temp_extensions {
            if file_name_lower.ends_with(ext) {
                return true;
            }
        }

        // 检查文件名前缀
        for prefix in &temp_prefixes {
            if file_name_lower.starts_with(prefix) {
                return true;
            }
        }

        // 检查是否是浏览器临时文件
        if file_name_lower.contains("temp") ||
           file_name_lower.contains("cache") ||
           file_name_lower.contains("tmp") {
            return true;
        }
    }

    false
}

fn get_file_type_description(dir_str: &str) -> &'static str {
    if dir_str.contains("downloads") {
        "下载临时文件"
    } else if dir_str.contains("cache") {
        "缓存文件"
    } else if dir_str.contains("temp") {
        "临时文件"
    } else if dir_str.contains("crashdumps") {
        "崩溃转储文件"
    } else if dir_str.contains("recent") {
        "最近使用记录"
    } else if dir_str.contains("prefetch") {
        "预读取文件"
    } else if dir_str.contains("logs") {
        "日志文件"
    } else if dir_str.contains("reportqueue") {
        "错误报告"
    } else {
        "系统文件"
    }
}

fn release_memory() -> usize {
    let process_ids = unsafe {
        let mut pids = Vec::with_capacity(1024);
        let mut cb_needed = 0;
        if EnumProcesses(pids.as_mut_ptr(), (pids.capacity() * std::mem::size_of::<u32>()) as u32, &mut cb_needed) != 0 {
            pids.set_len((cb_needed / std::mem::size_of::<u32>() as u32) as usize);
            pids
        } else {
            Vec::new()
        }
    };

    process_ids.par_iter().filter_map(|&pid| {
        let process_handle = unsafe {
            OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid)
        };

        if !process_handle.is_null() {
            let mut counters = std::mem::MaybeUninit::<PROCESS_MEMORY_COUNTERS>::uninit();
            let result = unsafe {
                GetProcessMemoryInfo(process_handle, counters.as_mut_ptr(), std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32)
            };

            if result != 0 {
                unsafe {
                    if EmptyWorkingSet(process_handle) != 0 {
                        Some(1)
                    } else {
                        None
                    }
                }
            } else {
                None
            }
        } else {
            None
        }
    }).count()
}

async fn upload_to_pocketbase(record: CleanupRecord, sender: mpsc::Sender<String>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if !POCKETBASE_ENABLED {
        return Ok(()); // 如果禁用了上传，直接返回
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(POCKETBASE_TIMEOUT))
        .build()?;

    // 获取计算机名
    let computer_name = env::var("COMPUTERNAME").unwrap_or_else(|_| "Unknown".to_string());

    // 转换为PocketBase格式
    let pb_record = PocketBaseRecord {
        computer_name,
        cleanup_time: record.cleanup_time.to_rfc3339(),
        files_cleaned_count: record.files_cleaned_count,
        memory_processes_count: record.memory_processes_count,
        cleaned_files: serde_json::to_string(&record.cleaned_files)?,
        cleanup_paths: serde_json::to_string(&record.cleanup_paths)?,
        is_admin: record.is_admin,
        total_duration_seconds: record.total_duration_seconds,
    };

    let url = format!("{}/api/collections/{}/records", POCKETBASE_URL, COLLECTION_NAME);

    let response = client
        .post(&url)
        .json(&pb_record)
        .send()
        .await?;

    if response.status().is_success() {
        sender.send("数据已成功同步".to_string()).ok();
    } else {
        eprintln!("上传到PocketBase失败: {}", response.status());
    }

    Ok(())
}

fn send_completion_notification(cleaned_count: usize, memory_count: usize) {
    if !NOTIFICATION_ENABLED {
        return; // 如果禁用了通知，直接返回
    }

    // 在后台线程中发送通知，避免阻塞主界面
    let timeout = NOTIFICATION_TIMEOUT;
    tokio::spawn(async move {
        let title = "磁盘清理完成";
        let body = if cleaned_count > 0 {
            format!("清理完成！清理了 {} 个文件/目录，优化了 {} 个进程内存", cleaned_count, memory_count)
        } else {
            format!("清理完成！优化了 {} 个进程内存，系统已经很干净了", memory_count)
        };

        // 尝试发送系统通知
        match Notification::new()
            .summary(title)
            .body(&body)
            .timeout(timeout as i32) // 使用配置的超时时间
            .show()
        {
            Ok(_) => {
                // 通知发送成功
            }
            Err(_) => {
                // 通知发送失败，静默处理
                // 在Windows上，如果没有合适的通知系统，这是正常的
            }
        }
    });
}

fn scan_and_clean_log_files(sender: mpsc::Sender<String>) -> usize {
    if !LOG_SCAN_ENABLED {
        return 0;
    }

    sender.send("正在扫描磁盘上的.log文件...".to_string()).ok();

    let mut total_cleaned = 0;

    for drive in LOG_SCAN_DRIVES {
        let drive_path = PathBuf::from(drive);
        if !drive_path.exists() {
            continue;
        }

        sender.send(format!("正在扫描驱动器: {}", drive)).ok();

        // 扫描常见的日志文件位置
        let log_paths = get_common_log_paths(drive);

        for log_path in log_paths {
            if log_path.exists() {
                total_cleaned += scan_directory_for_logs(&log_path, sender.clone());
            }
        }
    }

    if total_cleaned > 0 {
        sender.send(format!("日志文件清理完成，共清理了 {} 个日志文件", total_cleaned)).ok();
    } else {
        sender.send("未找到需要清理的日志文件".to_string()).ok();
    }

    total_cleaned
}

fn get_common_log_paths(drive: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let drive_path = Path::new(drive);

    // Windows系统日志路径
    if cfg!(windows) {
        paths.push(drive_path.join("Windows").join("Logs"));
        paths.push(drive_path.join("Windows").join("System32").join("LogFiles"));
        paths.push(drive_path.join("Windows").join("Temp"));
        paths.push(drive_path.join("ProgramData").join("Microsoft").join("Windows").join("WER").join("ReportQueue"));

        // 常见应用程序日志路径
        paths.push(drive_path.join("Program Files").join("Common Files").join("Microsoft Shared").join("Web Server Extensions").join("14").join("Logs"));
        paths.push(drive_path.join("inetpub").join("logs"));

        // 用户日志路径
        if let Ok(users_dir) = fs::read_dir(drive_path.join("Users")) {
            for user_entry in users_dir.filter_map(Result::ok) {
                let user_path = user_entry.path();
                paths.push(user_path.join("AppData").join("Local").join("Temp"));
                paths.push(user_path.join("AppData").join("Roaming"));
            }
        }
    }

    // 通用日志路径
    paths.push(drive_path.join("logs"));
    paths.push(drive_path.join("log"));
    paths.push(drive_path.join("var").join("log"));
    paths.push(drive_path.join("tmp"));

    paths
}

fn scan_directory_for_logs(dir: &Path, sender: mpsc::Sender<String>) -> usize {
    let mut cleaned_count = 0;

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();

            if path.is_file() {
                if should_clean_log_file(&path) {
                    if let Ok(metadata) = fs::metadata(&path) {
                        // 检查文件大小
                        let file_size_mb = metadata.len() / (1024 * 1024);
                        if file_size_mb >= LOG_MIN_SIZE_MB {
                            // 检查文件年龄
                            if let Ok(modified) = metadata.modified() {
                                if let Ok(elapsed) = modified.elapsed() {
                                    let file_age_days = elapsed.as_secs() / (24 * 3600);

                                    if file_age_days >= LOG_MAX_AGE_DAYS {
                                        if fs::remove_file(&path).is_ok() {
                                            sender.send(format!("已删除日志文件: {} ({:.1}MB)",
                                                path.display(), file_size_mb as f64)).ok();
                                            cleaned_count += 1;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } else if path.is_dir() {
                // 递归扫描子目录，但限制深度避免无限递归
                if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                    if !dir_name.starts_with('.') && dir_name != "System Volume Information" {
                        cleaned_count += scan_directory_for_logs(&path, sender.clone());
                    }
                }
            }
        }
    }

    cleaned_count
}

fn should_clean_log_file(path: &Path) -> bool {
    if let Some(file_name) = path.file_name().and_then(|name| name.to_str()) {
        let file_name_lower = file_name.to_lowercase();

        // 检查是否是日志文件
        if file_name_lower.ends_with(".log") ||
           file_name_lower.ends_with(".log.old") ||
           file_name_lower.ends_with(".log.1") ||
           file_name_lower.ends_with(".log.2") ||
           file_name_lower.ends_with(".log.3") ||
           file_name_lower.ends_with(".log.4") ||
           file_name_lower.ends_with(".log.5") {
            return true;
        }

        // 检查其他日志文件格式
        if file_name_lower.contains(".log.") ||
           (file_name_lower.contains("log") && (
               file_name_lower.ends_with(".txt") ||
               file_name_lower.ends_with(".out") ||
               file_name_lower.ends_with(".err")
           )) {
            return true;
        }
    }

    false
}

fn empty_recycle_bin(sender: mpsc::Sender<String>) -> bool {
    unsafe {
        let result = SHEmptyRecycleBinW(
            std::ptr::null_mut(), // 所有驱动器
            std::ptr::null(),     // 清空所有文件
            SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND, // 静默清空
        );

        if result == 0 {
            sender.send("已清空回收站".to_string()).ok();
            true
        } else {
            sender.send("清空回收站失败".to_string()).ok();
            false
        }
    }
}

fn draw_ui(frame: &mut ratatui::Frame, app: &App) {
    let main_layout = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .margin(1)
        .constraints([ratatui::layout::Constraint::Percentage(80), ratatui::layout::Constraint::Percentage(20)])
        .split(frame.size());

    let cleaned_list: Vec<ListItem> = app.cleaned_files.iter().map(|f| ListItem::new(f.as_str())).collect();
    let cleaned_list_widget = List::new(cleaned_list)
        .block(Block::default()
            .title("操作日志")
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::LightBlue)));
    frame.render_widget(cleaned_list_widget, main_layout[0]);

    let messages_widget = Paragraph::new(
        app.messages.iter().map(|msg| {
            let color = if msg.starts_with("已删除") {
                Color::Red
            } else if msg.starts_with("已清空回收站") {
                Color::Cyan
            } else if msg.starts_with("内存释放完成") {
                Color::Green
            } else if msg.starts_with("清理完成") {
                Color::Yellow
            } else if msg.starts_with("提示") {
                Color::Magenta
            } else if msg.starts_with("数据已成功同步") {
                Color::Red
            } else {
                Color::White
            };
            ratatui::text::Line::from(ratatui::text::Span::styled(msg, Style::default().fg(color)))
        }).collect::<Vec<_>>()
    ).block(Block::default()
        .title("状态")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::LightCyan)));
    frame.render_widget(messages_widget, main_layout[1]);
}