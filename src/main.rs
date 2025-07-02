use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    prelude::{CrosstermBackend, Terminal, Color, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};
use rayon::prelude::*;
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
use windows_sys::Win32::{
    System::ProcessStatus::{
        EmptyWorkingSet, EnumProcesses, GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    },
    System::Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ},
};

struct App {
    cleaned_files: Vec<String>,
    is_cleaning: Arc<AtomicBool>,
    messages: Vec<String>,
    cleaning_finished: bool,
    is_releasing_memory: bool,
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
        }
    }

    fn start_release_memory(&mut self, sender: mpsc::Sender<String>) {
        self.is_releasing_memory = true;
        self.messages.push("正在释放内存...".to_string());
        let sender_clone = sender.clone();
        tokio::spawn(async move {
            let released_count = release_memory();
            sender_clone.send(format!("内存释放完成! 共整理了 {} 个进程。", released_count)).unwrap();
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

            sender.send(format!("清理完成! 总共清理了 {} 个文件/目录。", total_cleaned)).unwrap();
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
            if msg.starts_with("内存释放完成") {
                app.is_releasing_memory = false;
                app.messages.retain(|m| m != "正在释放内存...");
                app.messages.push(msg.clone());
                app.cleaned_files.push(msg);
                app.start_cleaning(tx.clone());
            } else if msg.starts_with("清理完成") {
                app.is_cleaning.store(false, Ordering::SeqCst);
                app.cleaning_finished = true;
                app.messages.retain(|m| m != "正在清理中...");
                app.messages.push(msg);
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
            } else if msg.starts_with("内存释放完成") {
                Color::Green
            } else if msg.starts_with("清理完成") {
                Color::Yellow
            } else if msg.starts_with("提示") {
                Color::Magenta
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