//! One source file's measures: its lines, its estimated cyclomatic complexity,
//! its tests and its TODOs, read from the text alone, without a parser.

/// How a language writes comments, and which words branch.
pub struct Language {
    pub name: &'static str,
    line: &'static [&'static str],
    block: Option<(&'static str, &'static str)>,
    /// `and`/`or` are words that branch (Python, Ruby).
    word_logic: bool,
    /// Rust: every `=>` arm of a `match` past the first branches.
    match_arms: bool,
}

const C_LIKE: (&[&str], Option<(&str, &str)>) = (&["//"], Some(("/*", "*/")));
const HASH: (&[&str], Option<(&str, &str)>) = (&["#"], None);

const fn language(
    name: &'static str,
    (line, block): (
        &'static [&'static str],
        Option<(&'static str, &'static str)>,
    ),
) -> Language {
    Language {
        name,
        line,
        block,
        word_logic: false,
        match_arms: false,
    }
}

const RUST: Language = Language {
    match_arms: true,
    ..language("Rust", C_LIKE)
};
const SWIFT: Language = language("Swift", C_LIKE);
const PYTHON: Language = Language {
    word_logic: true,
    ..language("Python", HASH)
};
const RUBY: Language = Language {
    word_logic: true,
    ..language("Ruby", HASH)
};
const JAVASCRIPT: Language = language("JavaScript", C_LIKE);
const TYPESCRIPT: Language = language("TypeScript", C_LIKE);
const GO: Language = language("Go", C_LIKE);
const JAVA: Language = language("Java", C_LIKE);
const KOTLIN: Language = language("Kotlin", C_LIKE);
const C: Language = language("C", C_LIKE);
const CPP: Language = language("C++", C_LIKE);
const OBJC: Language = language("Objective-C", C_LIKE);
const CSHARP: Language = language("C#", C_LIKE);
const PHP: Language = language("PHP", (&["//", "#"], Some(("/*", "*/"))));
const SCALA: Language = language("Scala", C_LIKE);
const DART: Language = language("Dart", C_LIKE);
const SHELL: Language = language("Shell", HASH);
const LUA: Language = language("Lua", (&["--"], Some(("--[[", "]]"))));
const SQL: Language = language("SQL", (&["--"], Some(("/*", "*/"))));
const ELIXIR: Language = language("Elixir", HASH);
const CSS: Language = language("CSS", (&[], Some(("/*", "*/"))));
const HTML: Language = language("HTML", (&[], Some(("<!--", "-->"))));

/// The language a file is written in, from its extension; `None` for
/// anything that is not source code (docs, data, images).
pub fn language_of(path: &str) -> Option<&'static Language> {
    let name = path.rsplit('/').next().unwrap_or(path);
    if name.ends_with(".min.js") || name.ends_with(".min.css") {
        return None;
    }
    let ext = name.rsplit_once('.')?.1.to_ascii_lowercase();
    Some(match ext.as_str() {
        "rs" => &RUST,
        "swift" => &SWIFT,
        "py" => &PYTHON,
        "rb" => &RUBY,
        "js" | "jsx" | "mjs" | "cjs" => &JAVASCRIPT,
        "ts" | "tsx" | "mts" | "cts" => &TYPESCRIPT,
        "go" => &GO,
        "java" => &JAVA,
        "kt" | "kts" => &KOTLIN,
        "c" | "h" => &C,
        "cc" | "cpp" | "cxx" | "hpp" | "hh" => &CPP,
        "m" | "mm" => &OBJC,
        "cs" => &CSHARP,
        "php" => &PHP,
        "scala" => &SCALA,
        "dart" => &DART,
        "sh" | "bash" | "zsh" => &SHELL,
        "lua" => &LUA,
        "sql" => &SQL,
        "ex" | "exs" => &ELIXIR,
        "css" | "scss" | "less" => &CSS,
        "html" | "htm" => &HTML,
        _ => return None,
    })
}

/// Folders whose files are someone else's code or build output.
const VENDORED: [&str; 8] = [
    "node_modules",
    "vendor",
    "target",
    "dist",
    "build",
    "third_party",
    ".build",
    "Pods",
];

/// Whether `path` is the project's own code, not a vendored or built copy.
pub fn is_own(path: &str) -> bool {
    !path.split('/').any(|part| VENDORED.contains(&part))
}

/// Whether the whole file is a test, from where it lives and how it is named.
pub fn is_test_file(path: &str) -> bool {
    let mut parts = path.split('/').collect::<Vec<_>>();
    let name = parts.pop().unwrap_or_default();
    let stem = name.split('.').next().unwrap_or(name);
    parts
        .iter()
        .any(|dir| matches!(*dir, "tests" | "test" | "spec" | "__tests__" | "Tests"))
        || stem.starts_with("test_")
        || stem.ends_with("_test")
        || stem.ends_with("_spec")
        || stem.ends_with("Tests")
        || stem.ends_with("Test")
        || name.contains(".test.")
        || name.contains(".spec.")
}

/// What one file holds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FileMetrics {
    pub code: usize,
    pub comments: usize,
    pub blank: usize,
    /// Code lines that test the rest: a test file, or Rust's `#[cfg(test)]`.
    pub test_code: usize,
    /// Test functions (`#[test]`, `def test_`, `it(`…).
    pub tests: usize,
    /// Branch points: `if`, loops, `case`s, `&&`, `||`, `match` arms…
    pub decisions: usize,
    /// Functions declared with a keyword (`fn`, `def`, `func`…).
    pub functions: usize,
    /// `TODO`, `FIXME` and `HACK` marks.
    pub todos: usize,
}

impl FileMetrics {
    /// Units the decisions are spread over: the functions, or the file
    /// itself when its language declares none with a keyword.
    pub fn units(&self) -> usize {
        self.functions.max(1)
    }

    /// McCabe's number summed over the file: one per unit, plus its branches.
    pub fn complexity(&self) -> usize {
        self.decisions + self.units()
    }
}

const DECISIONS: [&str; 13] = [
    "if", "for", "while", "case", "catch", "guard", "elif", "elsif", "unless", "until", "except",
    "when", "rescue",
];
const FUNCTIONS: [&str; 5] = ["fn", "func", "def", "function", "fun"];
const TODOS: [&str; 3] = ["TODO", "FIXME", "HACK"];

/// Measures `text`, a file at `path` in `language`.
pub fn measure(path: &str, language: &Language, text: &str) -> FileMetrics {
    let mut m = FileMetrics::default();
    let mut in_tests = is_test_file(path);
    let mut in_block = false;
    let mut matches = 0;
    for line in text.lines() {
        let trimmed = line.trim();
        if TODOS.iter().any(|mark| line.contains(mark)) {
            m.todos += 1;
        }
        if trimmed.is_empty() {
            m.blank += 1;
            continue;
        }
        if in_block {
            m.comments += 1;
            in_block = !language.block.is_some_and(|(_, end)| trimmed.contains(end));
            continue;
        }
        if language.line.iter().any(|start| trimmed.starts_with(start)) {
            m.comments += 1;
            continue;
        }
        if let Some((start, end)) = language.block
            && let Some(rest) = trimmed.strip_prefix(start)
        {
            m.comments += 1;
            in_block = !rest.contains(end);
            continue;
        }
        m.code += 1;
        if trimmed == "#[cfg(test)]" {
            in_tests = true;
        }
        if in_tests {
            m.test_code += 1;
        }
        if is_test_mark(trimmed) {
            m.tests += 1;
        }
        matches += branches(language, trimmed, &mut m);
    }
    if language.match_arms {
        m.decisions = m.decisions.saturating_sub(matches);
    }
    m
}

fn is_test_mark(line: &str) -> bool {
    line.starts_with("#[test]")
        || line.starts_with("#[tokio::test")
        || line.starts_with("@Test")
        || line.starts_with("def test")
        || line.starts_with("func test")
        || line.starts_with("func Test")
        || line.starts_with("it(")
        || line.starts_with("test(")
}

/// Counts the line's branches into `m`; returns its `match`es, whose first
/// arm does not branch.
fn branches(language: &Language, line: &str, m: &mut FileMetrics) -> usize {
    let mut matches = 0;
    for word in line.split(|c: char| !(c.is_alphanumeric() || c == '_')) {
        if DECISIONS.contains(&word) || (language.word_logic && matches!(word, "and" | "or")) {
            m.decisions += 1;
        } else if FUNCTIONS.contains(&word) {
            m.functions += 1;
        } else if word == "match" {
            matches += 1;
        }
    }
    m.decisions += line.matches("&&").count() + logical_ors(line);
    if language.match_arms {
        m.decisions += line.matches("=>").count();
    }
    matches
}

/// The `||` that join two conditions, not Rust's or JavaScript's closures
/// (`|| x`, `move || x`).
fn logical_ors(line: &str) -> usize {
    line.match_indices("||")
        .filter(|(at, _)| {
            let before = line[..*at].trim_end();
            !(before.is_empty()
                || before.ends_with("move")
                || before.ends_with(['(', ',', '=', '{', '|']))
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rust(text: &str) -> FileMetrics {
        measure("src/lib.rs", &RUST, text)
    }

    #[test]
    fn lines_split_into_code_comments_and_blanks() {
        let m = rust("// head\nfn a() {\n\n    /* one\n two */\n    let x = 1; // TODO\n}\n");
        assert_eq!((m.code, m.comments, m.blank, m.todos), (3, 3, 1, 1));
    }

    #[test]
    fn branches_add_to_the_complexity() {
        let m = rust("fn a(x: u8) {\n    if x > 1 && x < 3 {}\n    for _ in 0..x {}\n}\n");
        assert_eq!((m.functions, m.decisions, m.complexity()), (1, 3, 4));
        let arms =
            rust("fn b(x: u8) {\n    match x {\n        1 => (),\n        _ => (),\n    }\n}\n");
        assert_eq!(arms.complexity(), 2);
        let closures = rust("fn c() {\n    a.or_else(|| b).map(move || c);\n    if d || e {}\n}\n");
        assert_eq!(closures.decisions, 2);
    }

    #[test]
    fn rust_tests_are_counted_after_cfg_test() {
        let m = rust("fn a() {}\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() {}\n}\n");
        assert_eq!((m.code, m.test_code, m.tests), (6, 5, 1));
    }

    #[test]
    fn test_files_are_known_by_place_and_name() {
        assert!(is_test_file("tests/layout.rs"));
        assert!(is_test_file("web/app.spec.ts"));
        assert!(is_test_file("pkg/x_test.go"));
        assert!(!is_test_file("src/latest.rs"));
        assert!(!is_own("web/node_modules/x.js"));
    }

    #[test]
    fn languages_come_from_the_extension() {
        assert_eq!(language_of("src/a.RS").map(|l| l.name), Some("Rust"));
        assert_eq!(language_of("README.md").map(|l| l.name), None);
        assert_eq!(language_of("a.min.js").map(|l| l.name), None);
        let py = measure(
            "a.py",
            language_of("a.py").unwrap(),
            "def f(a):\n    if a and b:\n        pass\n",
        );
        assert_eq!(py.complexity(), 3);
    }
}
