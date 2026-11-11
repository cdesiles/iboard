#![allow(clippy::unwrap_used)]

use super::*;

fn lane(project: &str, names: &[&str]) -> Vec<(String, PathBuf)> {
    names
        .iter()
        .map(|name| {
            let path = PathBuf::from(format!("/{project}/issues/ready/{name}"));
            (key(project, &path), path)
        })
        .collect()
}

fn names(merged: &[PathBuf]) -> Vec<String> {
    merged
        .iter()
        .map(|path| {
            let project = path.components().nth(1).unwrap().as_os_str();
            format!("{}/{}", project.to_string_lossy(), files::file_name(path))
        })
        .collect()
}

fn stored(keys: &[&str]) -> Vec<String> {
    keys.iter().map(|key| key.to_string()).collect()
}

#[test]
fn a_key_survives_renumbering() {
    let path = Path::new("/app/issues/ready/004-export.md");
    assert_eq!(key("app", path), "app/export.md");
}

#[test]
fn the_stored_order_interleaves_the_projects() {
    let lanes = [
        lane("bts", &["001-a.md", "002-b.md"]),
        lane("psy", &["001-c.md"]),
    ];
    let merged = merge(&lanes, &stored(&["bts/a.md", "psy/c.md", "bts/b.md"]));
    assert_eq!(
        names(&merged),
        ["bts/001-a.md", "psy/001-c.md", "bts/002-b.md"]
    );
}

#[test]
fn a_project_s_own_order_always_holds() {
    let lanes = [
        lane("bts", &["001-a.md", "002-b.md"]),
        lane("psy", &["001-c.md"]),
    ];
    let merged = merge(&lanes, &stored(&["bts/b.md", "psy/c.md", "bts/a.md"]));
    assert_eq!(
        names(&merged),
        ["bts/001-a.md", "bts/002-b.md", "psy/001-c.md"]
    );
}

#[test]
fn unknown_issues_follow_their_neighbour_and_gone_ones_are_ignored() {
    let lanes = [
        lane("bts", &["001-a.md", "002-new.md", "003-b.md"]),
        lane("psy", &["001-c.md", "002-late.md"]),
    ];
    let order = stored(&["psy/c.md", "bts/gone.md", "bts/a.md", "bts/b.md"]);
    assert_eq!(
        names(&merge(&lanes, &order)),
        [
            "psy/001-c.md",
            "psy/002-late.md",
            "bts/001-a.md",
            "bts/002-new.md",
            "bts/003-b.md"
        ]
    );
    assert_eq!(
        names(&merge(&lanes, &[])),
        [
            "bts/001-a.md",
            "bts/002-new.md",
            "bts/003-b.md",
            "psy/001-c.md",
            "psy/002-late.md"
        ],
        "with no order, project by project"
    );
}

#[test]
fn the_order_saves_and_loads() {
    let scratch = tempfile::tempdir().unwrap();
    let file = scratch.path().join("order.toml");
    let mut order = Order::default();
    order
        .columns
        .insert("ready".into(), stored(&["psy/c.md", "bts/a.md"]));
    order.save(&file).unwrap();
    assert_eq!(Order::load(Some(&file)), order);
    assert_eq!(
        Order::load(Some(&scratch.path().join("none"))),
        Order::default()
    );
}
