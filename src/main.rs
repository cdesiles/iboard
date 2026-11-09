//! iboard: a fast, local kanban board + markdown reader for the
//! `issues/` directories used across projects.

mod app;
mod dialog;
mod groups_ui;
mod instance;
mod label_picker;
mod project_menu;
mod recents;
#[cfg(test)]
mod scratch_git;
mod settings_ui;

// The shared modules live in the `iboard` library. Re-export them at this crate
// root so the view modules keep addressing them as `crate::…`.
use iboard::{
    agent, assets, build_log, commits, design_ui, discovery, domains, files, filter, format,
    general, issue, job, kernel, labels, plan, priority, process, reviews, settings, sheet, theme,
    titles, trash, widgets, workspace,
};

use std::path::PathBuf;

use eframe::egui;
use instance::Request;

fn main() -> eframe::Result {
    let mut path: Option<PathBuf> = None;
    let mut list = false;
    let mut init = false;
    let mut all = false;

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
            "-a" | "--all" => all = true,
            other if other.starts_with('-') => {
                eprintln!("iboard: unknown option `{other}`\n");
                print_help();
                std::process::exit(2);
            }
            other => path = Some(PathBuf::from(other)),
        }
    }

    if all && list {
        print_groups(&workspace_or_exit());
        return Ok(());
    }

    // A bare `iboard` brings the running one forward, or opens the latest project.
    let bare = path.is_none() && !(init || list || all);
    if bare {
        if handed_over(&Request::Focus) {
            return Ok(());
        }
        path = recents::cache_file().and_then(|file| recents::latest(&file));
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
                eprintln!("iboard: {err}");
                std::process::exit(1);
            }
        }
    }

    // Outside any project, a workspace or the welcome screen opens instead of
    // failing, unless a path or a command asked for a project.
    let located = match discovery::locate(&start) {
        _ if all => None,
        Ok(located) => Some(located),
        Err(_) if !init && has_workspace() => None,
        Err(_) if bare => return open_window(None, "iboard".to_string()),
        Err(err) => {
            eprintln!("iboard: {err}");
            std::process::exit(1);
        }
    };
    let Some(located) = located else {
        let workspace = workspace_or_exit();
        if list {
            print_groups(&workspace);
            return Ok(());
        }
        let first = workspace.groups.first().map(|group| group.name.clone());
        return run_app(None, first.unwrap_or_default());
    };

    if list {
        match discovery::build(&located) {
            Ok(board) => {
                print_listing(&board);
                return Ok(());
            }
            Err(err) => {
                eprintln!("iboard: {err}");
                std::process::exit(1);
            }
        }
    }

    let app_title = discovery::window_title(&located);
    run_app(Some(located), app_title)
}

/// [`open_window`], unless an iboard already running takes the board.
fn run_app(located: Option<discovery::Located>, app_title: String) -> eframe::Result {
    let request = match &located {
        Some(located) => Request::Project(located.project_dir.clone()),
        None => Request::Group(app_title.clone()),
    };
    if handed_over(&request) {
        return Ok(());
    }
    open_window(located, app_title)
}

/// Opens the window on `located`, or with none on the first group's board, or
/// the welcome screen when there is no group.
fn open_window(located: Option<discovery::Located>, app_title: String) -> eframe::Result {
    app::on_signals();
    let listener = instance::socket().and_then(|socket| instance::listen(&socket));
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1280.0, 820.0])
        .with_min_inner_size([880.0, 480.0]);
    if let Some(icon) = app_icon() {
        viewport = viewport.with_icon(icon);
    }
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
        Box::new(move |cc| {
            Ok(Box::new(app::IboardApp::new(
                cc, located, app_title, listener,
            )))
        }),
    )
}

/// Whether an iboard already running took `request`: this one has nothing left
/// to do.
fn handed_over(request: &Request) -> bool {
    instance::socket().is_some_and(|socket| instance::hand_over(&socket, request))
}

/// The Logo's app icon, for the Dock: the "i" whose dot is a card, on its
/// dark tile (see `design/components/Logo`).
fn app_icon() -> Option<egui::IconData> {
    let png = include_bytes!("../assets/iboard-256.png");
    let image = image::load_from_memory(png).ok()?.into_rgba8();
    Some(egui::IconData {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    })
}

/// Whether the user configuration lists `[[workspace.group]]` projects.
fn has_workspace() -> bool {
    let file = settings::config_file();
    workspace::load(&mut settings::UserConfig::load(file.as_deref())).is_some()
}

/// Each group's board, one after the other.
fn print_groups(workspace: &workspace::Workspace) {
    let dir = workspace::order::dir();
    for group in &workspace.groups {
        let file = dir
            .as_deref()
            .map(|dir| workspace::order::file(dir, &group.name));
        let order = workspace::Order::load(file.as_deref());
        let root = workspace::virtual_root(&group.name);
        let board = workspace::build(group, &root, &order);
        let repos = match board.repos.len() {
            1 => "1 repo".to_string(),
            n => format!("{n} repos"),
        };
        println!("{}  (group, {repos})", board.name());
        print_columns(&board);
        println!();
    }
}

/// The user's `[[workspace.group]]` projects, or exits when there is none.
fn workspace_or_exit() -> workspace::Workspace {
    let file = settings::config_file();
    let mut config = settings::UserConfig::load(file.as_deref());
    let Some(workspace) = workspace::load(&mut config) else {
        let file = file.map_or("the user configuration".to_string(), |file| {
            file.display().to_string()
        });
        eprintln!("iboard: no [[workspace.group]] in {file}");
        std::process::exit(1);
    };
    workspace
}

/// Text-mode dump of the board, useful for scripting and for checking discovery
/// without opening a window.
fn print_listing(board: &discovery::Board) {
    println!("{}  ({})", board.name(), board.root.display());
    print_columns(board);
}

/// The columns of `board` and its warnings, under its header line.
fn print_columns(board: &discovery::Board) {
    for column in &board.columns {
        println!("\n{} ({})", column.name, column.issues.len());
        if column.issues.is_empty() {
            println!("  (empty)");
            continue;
        }
        for issue in &column.issues {
            let title = match board.repo_of(&issue.path) {
                Some(repo) => format!("[{}] {}", repo.name, issue.title),
                None => issue.title.clone(),
            };
            match issue.done_at {
                Some(date) => println!("  • {title}  {}", date.format("%Y-%m-%d")),
                None => println!("  • {title}"),
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
         one; parent folders are searched too. It defaults to the latest project,\n\
         or to the current directory for init and --list.\n\
         \n\
         One iboard runs at a time: when one is open, {name} brings it forward,\n\
         on PATH when given, instead of opening a second window.\n\
         \n\
         COMMANDS:\n    init             Create issues/ with todo, ready, doing and done,\n                     then open the board on it\n\
         \n\
         OPTIONS:\n    -l, --list       Print the board to stdout and exit (no window)\n    -a, --all        The board of the first [[workspace.group]] (with --list, of each)\n    -h, --help       Print this help\n    -V, --version    Print the version",
        name = env!("CARGO_BIN_NAME"),
        version = env!("CARGO_PKG_VERSION")
    );
}
