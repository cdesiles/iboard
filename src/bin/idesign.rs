//! `idesign`: create, browse, import and render a project's design system.
//!
//! It links the same `iboard` library the board does, so the two can never
//! disagree about the `design/` format. Reading, scaffolding a component and
//! importing an Open Design package are all here; so is the preview renderer,
//! which runs the same out-of-process helper the board uses.
//!
//! ```text
//! idesign show                          # the default summary
//! idesign list                          # one line per component
//! idesign tokens                        # every token, per section
//! idesign validate                      # fatal errors and warnings
//! idesign add-component NAME [PATH]     # scaffold components/NAME
//! idesign import PACKAGE [PATH]         # Open Design package -> design/
//! idesign preview NAME [PATH]           # render a component preview to a PNG
//! idesign render HTML PNG               # render any HTML file to a PNG
//! ```

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use iboard::design::{self, DesignSystem, Token, TokenValue};

const DEFAULT_WIDTH: u32 = 1200;
const DEFAULT_HEIGHT: u32 = 900;

#[derive(Default)]
struct Flags {
    group: Option<String>,
    intent: Option<String>,
    into: Option<String>,
    out: Option<String>,
    theme: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
}

fn main() -> ExitCode {
    let mut positionals: Vec<String> = Vec::new();
    let mut flags = Flags::default();

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_help();
                return ExitCode::SUCCESS;
            }
            "-V" | "--version" => {
                println!("{} {}", env!("CARGO_BIN_NAME"), env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            "--group" => match take(&mut args, "--group") {
                Ok(value) => flags.group = Some(value),
                Err(code) => return code,
            },
            "--intent" => match take(&mut args, "--intent") {
                Ok(value) => flags.intent = Some(value),
                Err(code) => return code,
            },
            "--into" => match take(&mut args, "--into") {
                Ok(value) => flags.into = Some(value),
                Err(code) => return code,
            },
            "--out" => match take(&mut args, "--out") {
                Ok(value) => flags.out = Some(value),
                Err(code) => return code,
            },
            "--theme" => match take(&mut args, "--theme") {
                Ok(value) => flags.theme = Some(value),
                Err(code) => return code,
            },
            "--width" => match take_number(&mut args, "--width") {
                Ok(value) => flags.width = Some(value),
                Err(code) => return code,
            },
            "--height" => match take_number(&mut args, "--height") {
                Ok(value) => flags.height = Some(value),
                Err(code) => return code,
            },
            other if other.starts_with('-') => {
                eprintln!("idesign: unknown option `{other}`\n");
                print_help();
                return ExitCode::from(2);
            }
            other => positionals.push(other.to_string()),
        }
    }

    let command = positionals
        .first()
        .map(String::as_str)
        .unwrap_or("show")
        .to_string();
    // No command is the default `show`; the rest is empty, not a panic.
    let rest: &[String] = positionals.get(1..).unwrap_or(&[]);

    match command.as_str() {
        "render" => {
            let (Some(html), Some(png)) = (rest.first(), rest.get(1)) else {
                eprintln!("idesign: render needs an HTML file and an output PNG");
                return ExitCode::from(2);
            };
            return render_file(Path::new(html), Path::new(png), &flags);
        }
        "import" => {
            let Some(package) = rest.first() else {
                eprintln!("idesign: import needs a package folder");
                return ExitCode::from(2);
            };
            return import(package, rest.get(1), &flags);
        }
        _ => {}
    }

    // Everything else reads the design system first.
    let (name, path) = match command.as_str() {
        "add-component" | "preview" => (rest.first().cloned(), rest.get(1).cloned()),
        _ => (None, rest.first().cloned()),
    };
    let start = resolve(path.as_ref());

    let mut system = match DesignSystem::discover(&start) {
        Ok(system) => system,
        Err(err) => {
            eprintln!("idesign: {err}");
            return ExitCode::FAILURE;
        }
    };

    match command.as_str() {
        "show" => print!("{}", system.describe()),
        "list" => list(&system),
        "tokens" => tokens(&system),
        "validate" => return validate(&system),
        "add-component" => {
            let Some(name) = name else {
                eprintln!("idesign: add-component needs a NAME");
                return ExitCode::from(2);
            };
            let group = flags.group.as_deref().unwrap_or("Components");
            match system.add_component(&name, group, flags.intent.as_deref()) {
                Ok(dir) => println!("created {}", dir.display()),
                Err(err) => {
                    eprintln!("idesign: {err}");
                    return ExitCode::FAILURE;
                }
            }
        }
        "preview" => {
            let Some(name) = name else {
                eprintln!("idesign: preview needs a component NAME");
                return ExitCode::from(2);
            };
            return preview(&system, &name, &flags);
        }
        other => {
            eprintln!("idesign: unknown command `{other}`\n");
            print_help();
            return ExitCode::from(2);
        }
    }
    ExitCode::SUCCESS
}

fn take(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, ExitCode> {
    args.next().ok_or_else(|| {
        eprintln!("idesign: {flag} needs a value");
        ExitCode::from(2)
    })
}

fn take_number(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<u32, ExitCode> {
    let value = take(args, flag)?;
    value.parse::<u32>().map_err(|_| {
        eprintln!("idesign: {flag} needs a number, got `{value}`");
        ExitCode::from(2)
    })
}

fn resolve(path: Option<&String>) -> PathBuf {
    path.map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

fn render_file(html: &Path, png: &Path, flags: &Flags) -> ExitCode {
    let width = flags.width.unwrap_or(DEFAULT_WIDTH);
    let height = flags.height.unwrap_or(DEFAULT_HEIGHT);
    match design::render::snapshot(html, png, width, height) {
        Ok(path) => {
            println!("{}", path.display());
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("idesign: {err}");
            ExitCode::FAILURE
        }
    }
}

fn preview(system: &DesignSystem, name: &str, flags: &Flags) -> ExitCode {
    let Some(component) = system.components.iter().find(|c| c.name == name) else {
        eprintln!("idesign: no component named `{name}`");
        return ExitCode::FAILURE;
    };
    let theme = flags.theme.as_deref().unwrap_or("light");
    let html = match design::css::preview_html(system, component, theme) {
        Ok(html) => html,
        Err(err) => {
            eprintln!("idesign: {err}");
            return ExitCode::FAILURE;
        }
    };
    let html_path = std::env::temp_dir().join(format!("iboard-preview-{name}.html"));
    if let Err(err) = std::fs::write(&html_path, &html) {
        eprintln!("idesign: couldn't write {}: {err}", html_path.display());
        return ExitCode::FAILURE;
    }
    let out = flags
        .out
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join(format!("iboard-preview-{name}.png")));
    let width = flags.width.unwrap_or(DEFAULT_WIDTH);
    let height = flags.height.unwrap_or(DEFAULT_HEIGHT);
    match design::render::snapshot(&html_path, &out, width, height) {
        Ok(path) => {
            println!("{}", path.display());
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("idesign: {err}");
            ExitCode::FAILURE
        }
    }
}

fn import(package: &str, path: Option<&String>, flags: &Flags) -> ExitCode {
    let package_dir = Path::new(package);
    let read = match design::open_design::read(package_dir) {
        Ok(read) => read,
        Err(err) => {
            eprintln!("idesign: {err}");
            return ExitCode::FAILURE;
        }
    };
    for warning in &read.warnings {
        println!("note: {warning}");
    }
    let design_dir = match &flags.into {
        Some(dir) => PathBuf::from(dir),
        None => resolve(path).join("design"),
    };
    if design_dir.join("design-system.json").is_file() {
        eprintln!(
            "idesign: {} already holds a design system; pass --into to choose another folder",
            design_dir.display()
        );
        return ExitCode::FAILURE;
    }
    match read.write(&design_dir) {
        Ok(system) => {
            println!(
                "imported {} ({}) into {}",
                system.meta.title,
                system.meta.namespace,
                design_dir.display()
            );
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("idesign: {err}");
            ExitCode::FAILURE
        }
    }
}

/// One line per component, naming which files it ships.
fn list(system: &DesignSystem) {
    println!(
        "{} — {} components",
        system.meta.title,
        system.components.len()
    );
    for component in &system.components {
        let doc = if component.readme.is_some() {
            "doc"
        } else {
            "-"
        };
        let preview = if component.has_preview {
            "preview"
        } else {
            "-"
        };
        println!("  {:<16} {doc:<4} {preview}", component.name);
    }
}

/// Every token, section by section, with themed values inline.
fn tokens(system: &DesignSystem) {
    let themes: Vec<&str> = system
        .tokens
        .themes
        .iter()
        .map(|theme| theme.id.as_str())
        .collect();

    let sections: [(&str, &[Token]); 5] = [
        ("color", &system.tokens.color),
        ("spacing", &system.tokens.spacing),
        ("radius", &system.tokens.radius),
        ("size", &system.tokens.size),
        ("shadow", &system.tokens.shadow),
    ];
    for (label, list) in sections {
        println!("{label} ({})", list.len());
        for token in list {
            println!(
                "  {:<16} {}",
                token.name,
                format_value(&token.value, &themes)
            );
        }
    }

    println!("type ({})", system.tokens.type_style_count());
    for group in &system.tokens.type_groups {
        println!("  {} ({})", group.name, group.family);
        for style in &group.styles {
            let size = style.font_size.as_deref().unwrap_or("");
            let weight = style.weight.map(|w| w.to_string()).unwrap_or_default();
            println!("    {:<12} {size} {weight}", style.name);
        }
    }
}

fn format_value(value: &TokenValue, themes: &[&str]) -> String {
    match value {
        TokenValue::Plain(value) => value.clone(),
        TokenValue::Themed(values) => themes
            .iter()
            .filter_map(|id| values.get(*id).map(|value| format!("{id}={value}")))
            .collect::<Vec<_>>()
            .join("  "),
    }
}

/// Fatal errors first, then warnings; exit 1 when anything is fatal.
fn validate(system: &DesignSystem) -> ExitCode {
    let errors = system.errors();
    for warning in &system.warnings {
        println!("warning: {warning}");
    }
    for error in &errors {
        println!("error: {error}");
    }
    if errors.is_empty() {
        println!(
            "{}: ok ({} components, {} warnings)",
            system.meta.title,
            system.components.len(),
            system.warnings.len()
        );
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn print_help() {
    println!(
        "{name} {version}\n\
         \n\
         Create, browse, import and render a design system at design/.\n\
         \n\
         USAGE:\n    {name} [COMMAND] [PATH]\n    {name} add-component NAME [PATH] [--group G] [--intent \"...\"]\n    {name} import PACKAGE [PATH] [--into DIR]\n    {name} preview NAME [PATH] [--out FILE] [--theme light|dark] [--width N] [--height N]\n    {name} render INPUT.html OUTPUT.png [--width N] [--height N]\n\
         \n\
         PATH defaults to the current directory; parent folders are searched for\n\
         a design/ or design-system.json. COMMAND defaults to show.\n\
         \n\
         COMMANDS:\n    show             Summary of the system (default)\n    list             One line per component\n    tokens           Every token, per section\n    validate         Fatal errors and warnings (exit 1 on error)\n    add-component    Scaffold components/NAME with a README and a preview\n    import           Turn an Open Design package into design/\n    preview          Render a component preview to a PNG\n    render           Render any HTML file to a PNG\n\
         \n\
         OPTIONS:\n    --group G        The component group for add-component (default Components)\n    --intent TEXT    One line of intent for the README\n    --into DIR       Where import writes the design system (default PATH/design)\n    --out FILE       Where preview writes the PNG\n    --theme NAME     The token theme to apply (default light)\n    --width N        Render width in points (default {width})\n    --height N       Render height in points (default {height})\n    -h, --help       Print this help\n    -V, --version    Print the version",
        name = env!("CARGO_BIN_NAME"),
        version = env!("CARGO_PKG_VERSION"),
        width = DEFAULT_WIDTH,
        height = DEFAULT_HEIGHT
    );
}
