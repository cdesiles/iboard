//! iboard: a fast, local kanban board + markdown reader for the
//! `issues/` directories used across projects.

mod app;
mod dialog;
mod label_picker;
mod recents;
mod review_settings;

// The shared modules live in the `iboard` library. Re-export them at this crate
// root so the view modules keep addressing them as `crate::…`.
use iboard::{
    agent, commits, design_ui, discovery, files, filter, general, issue, job, labels, process,
    settings, sheet, theme, titles, widgets,
};

use std::path::PathBuf;

use eframe::egui;

fn main() -> eframe::Result {
    let mut path: Option<PathBuf> = None;
    let mut list = false;
    let mut init = false;

    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print_help();
                return Ok(());
            }
            "-V" | "--version" => {
                println!("{} {}", env!("CARGO_BIN_NAME"), env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "init" => init = true,
            "-l" | "--list" => list = true,
            other if other.starts_with('-') => {
                eprintln!("iboard: unknown option `{other}`\n");
                print_help();
                std::process::exit(2);
            }
            other => path = Some(PathBuf::from(other)),
        }
    }

    let start = match path {
        Some(path) => path,
        None => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    };
    // Normalize so `iboard .` and `iboard ..` report a real project name.
    let start = std::fs::canonicalize(&start).unwrap_or(start);

    if init {
        // Create the layout, then carry on and open the board on it as usual.
        match discovery::init(&start) {
            Ok(located) => println!(
                "iboard: initialized {} ({})",
                located.root.display(),
                discovery::INIT_COLUMNS.join(", ")
            ),
            Err(err) => {
                eprintln!("iboard: {err:#}");
                std::process::exit(1);
            }
        }
    }

    let located = match discovery::locate(&start) {
        Ok(located) => located,
        Err(err) => {
            eprintln!("iboard: {err:#}");
            std::process::exit(1);
        }
    };

    if list {
        match discovery::build(&located) {
            Ok(board) => {
                print_listing(&board);
                return Ok(());
            }
            Err(err) => {
                eprintln!("iboard: {err:#}");
                std::process::exit(1);
            }
        }
    }

    let app_title = discovery::window_title(&located);

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1280.0, 820.0])
        .with_min_inner_size([880.0, 480.0]);
    if app::UNIFIED_TITLEBAR {
        // macOS names the app menu after the title the window starts with
        // (given the Info.plist `build.rs` embeds), so start as "iboard" and
        // rename the window once it is up. The header bar is the title bar:
        // no native title, content under the traffic lights.
        viewport = viewport
            .with_title("iboard")
            .with_fullsize_content_view(true)
            .with_titlebar_shown(false)
            .with_title_shown(false);
    } else {
        viewport = viewport.with_title(&app_title);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "iboard",
        options,
        Box::new(move |cc| Ok(Box::new(app::IboardApp::new(cc, located, app_title)))),
    )
}

/// Text-mode dump of the board, useful for scripting and for checking discovery
/// without opening a window.
fn print_listing(board: &discovery::Board) {
    let title = board
        .title
        .clone()
        .or_else(|| {
            board
                .project_dir
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "iboard".to_string());

    println!("{title}  ({})", board.root.display());
    for column in &board.columns {
        println!("\n{} ({})", column.name, column.issues.len());
        if column.issues.is_empty() {
            println!("  (empty)");
            continue;
        }
        for issue in &column.issues {
            match issue.done_at {
                Some(date) => println!("  • {}  {}", issue.title, date.format("%Y-%m-%d")),
                None => println!("  • {}", issue.title),
            }
            if !issue.summary.is_empty() {
                println!("    {}", issue.summary);
            }
        }
    }
    for warning in &board.warnings {
        eprintln!("warning: {warning}");
    }
}

fn print_help() {
    println!(
        "{name} {version}\n\
         \n\
         A fast, local kanban board and markdown reader for `issues/` directories.\n\
         \n\
         USAGE:\n    {name} [OPTIONS] [PATH]\n    {name} init [OPTIONS] [PATH]\n\
         \n\
         PATH may be a project root, a `issues/` directory, or any folder inside\n\
         one. Defaults to the current directory; parent folders are searched too.\n\
         \n\
         COMMANDS:\n    init             Create issues/ with todo, ready, doing and done,\n                     then open the board on it\n\
         \n\
         OPTIONS:\n    -l, --list       Print the board to stdout and exit (no window)\n    -h, --help       Print this help\n    -V, --version    Print the version",
        name = env!("CARGO_BIN_NAME"),
        version = env!("CARGO_PKG_VERSION")
    );
}
