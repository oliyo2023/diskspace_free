# Disk Space Analyzer

A command-line tool built with Rust to analyze and display disk space usage in a terminal-based user interface.

## Features

- Scans the specified disk or directory.
- Displays a hierarchical view of files and folders.
- Uses `ratatui` to create an interactive terminal UI.

## How to Build and Run

1.  **Clone the repository:**
    ```sh
    git clone https://github.com/oliyo2023/diskspace_free.git
    cd diskspace_free
    ```

2.  **Build the project in release mode:**
    ```sh
    cargo build --release
    ```

3.  **Run the application:**
    The executable will be located in the `target/release/` directory.
    ```sh
    ./target/release/diskspace_free
    ```

## Dependencies

This project relies on several great Rust libraries, including:
- `ratatui` for the terminal user interface.
- `crossterm` for terminal manipulation.
- `tokio` for asynchronous operations.
- `rayon` for parallel iteration.
- `is-admin` to check for administrator privileges.
