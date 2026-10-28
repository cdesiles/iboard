#![allow(clippy::unwrap_used)]

use super::*;

fn repo(parent: &Path, name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = parent.join(name);
    for (file, text) in files {
        let path = dir.join("issues").join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    std::fs::canonicalize(&dir).unwrap_or(dir)
}

fn titles(column: &Column) -> Vec<&str> {
    column.issues.iter().map(|i| i.title.as_str()).collect()
}

#[test]
fn projects_merge_by_column_and_tag_their_issues() {
    let scratch = tempfile::tempdir().unwrap();
    let parent = std::fs::canonicalize(scratch.path()).unwrap();
    let app = repo(
        &parent,
        "app",
        &[("ready/002-x.md", "# X\n"), ("wip/y.md", "# Y\n")],
    );
    let site = repo(
        &parent,
        "site",
        &[("ready/001-z.md", "# Z\n"), ("doing/w.md", "# W\n")],
    );
    let bare = parent.join("bare");
    std::fs::create_dir_all(&bare).unwrap();
    let workspace = Workspace {
        groups: vec![
            Group {
                name: "Dev".into(),
                repos: vec![app.clone()],
            },
            Group {
                name: "Client side".into(),
                repos: vec![site.clone(), bare, app.clone()],
            },
        ],
    };
    let board = build(&workspace, &parent.join("virtual"), &Order::default());

    let names: Vec<&str> = board.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["Ready", "Doing"], "wip/ joins Doing");
    assert_eq!(titles(&board.columns[0]), ["Z", "X"], "by priority");
    let mut doing = titles(&board.columns[1]);
    doing.sort();
    assert_eq!(doing, ["W", "Y"]);
    assert!(discovery::is_ready_column(&board.columns[0].dir));
    assert!(!board.columns[0].dir.exists(), "nothing is written there");

    let z = &board.columns[0].issues[0];
    assert!(
        z.labels.contains(&"repo:site".to_string()),
        "{:?}",
        z.labels
    );
    assert!(z.labels.contains(&"group:client-side".to_string()));
    assert_eq!(board.project_of(&z.path), site);
    assert_eq!(board.root_of(&z.path), site.join("issues"));
    assert_eq!(board.repos.len(), 2, "a project listed twice counts once");
    assert_eq!(board.name(), TITLE);
    assert_eq!(board.warnings.len(), 1, "{:?}", board.warnings);
    assert!(board.warnings[0].contains("no issues/ folder, skipped"));
}

#[test]
fn groups_load_in_order_with_their_home_folders() {
    let text = "[[workspace.group]]\nname = \"B\"\nrepos = [\"~/b\", \"/abs/c\"]\n\n\
                [[workspace.group]]\nname = \"A\"\nrepos = []\n";
    let mut config = UserConfig::parse(text, Path::new("iboard.toml"));
    let workspace = load(&mut config).unwrap();
    let names: Vec<&str> = workspace.groups.iter().map(|g| g.name.as_str()).collect();
    assert_eq!(names, ["B", "A"]);
    let home = settings::home().unwrap();
    assert_eq!(
        workspace.groups[0].repos,
        [home.join("b"), PathBuf::from("/abs/c")]
    );
    assert!(config.problems.is_empty());
    assert_eq!(load(&mut UserConfig::default()), None);
}
