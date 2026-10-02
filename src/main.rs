//! roadmap-viewer: a fast, local kanban board + markdown reader for the
//! `roadmap/` directories used across projects.

mod app;
mod config;
mod discovery;
mod history;
#[cfg(target_os = "macos")]
mod menu;
mod theme;
mod ticket;
mod widgets;

use std::path::PathBuf;

use eframe::egui;

fn main() -> eframe::Result {
    let mut path: Option<PathBuf> = None;
    let mut list = false;

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
            "-l" | "--list" | "--print" => list = true,
            other if other.starts_with('-') => {
                eprintln!("roadmap-viewer: unknown option `{other}`\n");
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
    // Normalize so `roadmap .` and `roadmap ..` report a real project name.
    let start = std::fs::canonicalize(&start).unwrap_or(start);

    let located = match discovery::locate(&start) {
        Ok(located) => located,
        Err(err) => {
            eprintln!("roadmap-viewer: {err:#}");
            std::process::exit(1);
        }
    };

    if list {
        match discovery::build(&located) {
            Ok(roadmap) => {
                print_listing(&roadmap);
                return Ok(());
            }
            Err(err) => {
                eprintln!("roadmap-viewer: {err:#}");
                std::process::exit(1);
            }
        }
    }

    let project_name = located
        .project_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("project");
    let app_title = located
        .config
        .as_ref()
        .and_then(|config| config.title.clone())
        .unwrap_or_else(|| project_name.to_string());

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1280.0, 820.0])
        .with_min_inner_size([880.0, 480.0]);
    if app::UNIFIED_TITLEBAR {
        // macOS names the app menu after the title the window starts with
        // (given the Info.plist `build.rs` embeds), so start as "Roadmap
        // Viewer" and rename the window once it is up. The header bar is the
        // title bar: no native title, content under the traffic lights.
        viewport = viewport
            .with_title("Roadmap Viewer")
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
        "Roadmap",
        options,
        Box::new(move |cc| Ok(Box::new(app::RoadmapApp::new(cc, located, app_title)))),
    )
}

/// Text-mode dump of the board, useful for scripting and for checking discovery
/// without opening a window.
fn print_listing(roadmap: &discovery::Roadmap) {
    let title = roadmap
        .title
        .clone()
        .or_else(|| {
            roadmap
                .project_dir
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "Roadmap".to_string());

    println!("{title}  ({})", roadmap.root.display());
    for column in &roadmap.columns {
        println!("\n{} ({})", column.name, column.tickets.len());
        if column.tickets.is_empty() {
            println!("  (empty)");
            continue;
        }
        for ticket in &column.tickets {
            match ticket.done_at {
                Some(date) => println!("  • {}  {}", ticket.title, date.format("%Y-%m-%d")),
                None => println!("  • {}", ticket.title),
            }
            if !ticket.summary.is_empty() {
                println!("    {}", ticket.summary);
            }
        }
    }
    for warning in &roadmap.warnings {
        eprintln!("warning: {warning}");
    }
}

fn print_help() {
    println!(
        "{name} {version}\n\
         \n\
         A fast, local kanban board and markdown reader for `roadmap/` directories.\n\
         \n\
         USAGE:\n    {name} [OPTIONS] [PATH]\n\
         \n\
         PATH may be a project root, a `roadmap/` directory, or any folder inside\n\
         one. Defaults to the current directory; parent folders are searched too.\n\
         \n\
         OPTIONS:\n    -l, --list       Print the board to stdout and exit (no window)\n    -h, --help       Print this help\n    -V, --version    Print the version",
        name = env!("CARGO_BIN_NAME"),
        version = env!("CARGO_PKG_VERSION")
    );
}
