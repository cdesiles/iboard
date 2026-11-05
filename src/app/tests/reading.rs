//! The reader's page against its spec: reading sizes, the measure, live task
//! boxes, and the Editor's text starting where the Reader's does.

use egui::accesskit::Role;
use egui_kittest::kittest::Queryable;

use super::kit::{Kit, kittest};
use super::*;

const LONG: &str = "This paragraph runs on for long enough that it has to wrap at least \
    once on the page, however wide the page is, so that the distance between its lines \
    and how far it reaches across the sheet can both be measured from the outside.";

fn opened(tag: &str, text: &str) -> (Scratch, PathBuf, Kit) {
    let dir = scratch_dir(tag);
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let file = todo.join("01-demo.md");
    std::fs::write(&file, text).unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.open_issue(file.clone());
    let harness = kittest(app);
    (dir, file, harness)
}

fn rect_of(harness: &Kit, label: &str) -> egui::Rect {
    let found: Vec<_> = harness.get_all_by_label(label).collect();
    found.last().unwrap().rect()
}

#[test]
fn a_paragraph_wraps_at_68ch_on_22pt_lines() {
    let (_dir, _file, mut harness) = opened("reader-body", &format!("# Demo\n\n{LONG}\n"));
    harness.run_steps(2);
    let rect = rect_of(&harness, LONG);
    assert!(rect.height() >= 44.0, "{rect:?}");
    assert_eq!(rect.height() % 22.0, 0.0, "{rect:?}");
    let zero = harness
        .ctx
        .fonts_mut(|fonts| fonts.glyph_width(&theme::read_body(), '0'));
    assert!(rect.width() <= 68.0 * zero, "{rect:?}");
}

#[test]
fn blocks_keep_the_specified_gaps() {
    let text = "# Demo\n\nFirst.\n\n## Context\n\nSecond.\n\n- one\n- two\n";
    let (_dir, _file, mut harness) = opened("reader-gaps", text);
    harness.run_steps(2);
    let [title, first, context, second, one, two] =
        ["Demo", "First.", "Context", "Second.", "one", "two"]
            .map(|label| rect_of(&harness, label));
    assert_eq!(title.height(), 26.0);
    assert_eq!(context.height(), 20.0);
    assert_eq!(first.top() - title.bottom(), 12.0);
    assert_eq!(context.top() - first.bottom(), 16.0);
    assert_eq!(second.top() - context.bottom(), 4.0);
    assert_eq!(one.top() - second.bottom(), 8.0);
    assert_eq!(two.top() - one.bottom(), 8.0);
    assert_eq!(one.left() - second.left(), 20.0);
}

#[test]
fn a_task_box_stands_for_its_bullet_and_ticking_it_writes_the_file() {
    let text = "# Demo\n\nFirst.\n\n- [ ] one\n- [x] two\n";
    let (_dir, file, mut harness) = opened("reader-task", text);
    harness.run_steps(2);
    let first = rect_of(&harness, "First.");
    let task = harness.get_by_role_and_label(Role::CheckBox, "one");
    assert_eq!(task.rect().left(), first.left());
    task.hover();
    harness.run_steps(1);
    harness.get_by_role_and_label(Role::CheckBox, "one").click();
    harness.run_steps(2);
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "# Demo\n\nFirst.\n\n- [x] one\n- [x] two\n"
    );
}

#[test]
fn the_editor_starts_its_text_where_the_reader_does() {
    let (_dir, file, mut harness) = opened("reader-inset", "# Demo\n\nFirst.\n");
    harness.run_steps(2);
    let reader_left = rect_of(&harness, "First.").left();
    harness.state_mut().1.start_editing(&file);
    harness.run_steps(2);
    let field = harness
        .ctx
        .read_response(egui::Id::new(crate::app::editor::EDITOR_ID))
        .unwrap();
    assert_eq!(field.rect.left(), reader_left);
}
