use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    prelude::{CrosstermBackend, Terminal},
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

    app.start_release_memory(tx.clone());

    loop {
        terminal.draw(|frame| {
            let main_layout = ratatui::layout::Layout::default()
                .direction(ratatui::layout::Direction::Vertical)
                .margin(1)
                .constraints([ratatui::layout::Constraint::Percentage(80), ratatui::layout::Constraint::Percentage(20)])
                .split(frame.size());

            let cleaned_list: Vec<ListItem> = app.cleaned_files.iter().map(|f| ListItem::new(f.as_str())).collect();
            let cleaned_list_widget = List::new(cleaned_list).block(Block::default().title("操作日志").borders(Borders::ALL));
            frame.render_widget(cleaned_list_widget, main_layout[0]);

            let messages_widget = Paragraph::new(app.messages.join("\n")).block(Block::default().title("状态").borders(Borders::ALL));
            frame.render_widget(messages_widget, main_layout[1]);
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
    if let Ok(temp) = env::var("TEMP") { paths.push(temp.into()); }
    if let Ok(tmp) = env::var("TMP") {
        let path_buf = PathBuf::from(tmp);
        if !paths.contains(&path_buf) { paths.push(path_buf); }
    }
    if let Ok(win_dir) = env::var("windir") {
        paths.push(Path::new(&win_dir).join("Prefetch"));
        paths.push(Path::new(&win_dir).join("Logs"));
    }
    paths
}

fn clean_directory(dir: &Path, sender: mpsc::Sender<String>) -> usize {
    if let Ok(entries) = fs::read_dir(dir) {
        let entries: Vec<_> = entries.filter_map(Result::ok).collect();
        return entries
            .par_iter()
            .map(|entry| {
                let path = entry.path();
                let mut count = 0;
                if path.is_dir() {
                    if fs::remove_dir_all(&path).is_ok() {
                        sender.send(format!("已删除目录: {:?}", path)).ok();
                        count += 1;
                    }
                } else if path.is_file() {
                    if fs::remove_file(&path).is_ok() {
                        sender.send(format!("已删除文件: {:?}", path)).ok();
                        count += 1;
                    }
                }
                count
            })
            .sum();
    }
    0
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