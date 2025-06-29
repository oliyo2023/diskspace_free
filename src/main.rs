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

struct App {
    cleaned_files: Vec<String>,
    is_cleaning: Arc<AtomicBool>,
    messages: Vec<String>,
    cleaning_finished: bool,
}

impl App {
    fn new() -> Self {
        let mut messages = vec!["按 'q' 退出".to_string()];
        if !is_admin::is_admin() {
            messages.push("提示: 未以管理员权限运行, 可能部分文件无法清理。".to_string());
        }
        Self {
            cleaned_files: Vec::new(),
            is_cleaning: Arc::new(AtomicBool::new(false)),
            messages,
            cleaning_finished: false,
        }
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

    app.start_cleaning(tx.clone());

    loop {
        terminal.draw(|frame| {
            let main_layout = ratatui::layout::Layout::default()
                .direction(ratatui::layout::Direction::Vertical)
                .margin(1)
                .constraints([ratatui::layout::Constraint::Percentage(80), ratatui::layout::Constraint::Percentage(20)])
                .split(frame.size());

            let cleaned_list: Vec<ListItem> = app.cleaned_files.iter().map(|f| ListItem::new(f.as_str())).collect();
            let cleaned_list_widget = List::new(cleaned_list).block(Block::default().title("已清理文件/目录").borders(Borders::ALL));
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
            if msg.starts_with("清理完成") {
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

