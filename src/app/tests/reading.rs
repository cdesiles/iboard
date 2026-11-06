//! The reader's page against its spec: reading sizes, the measure, live task
//! boxes, and the Editor's text starting where the Reader's does.

use egui::accesskit::Role;
use egui_kittest::kittest::Queryable;

use super::kit::{Kit, kittest_with};
use super::*;

const LONG: &str = "This paragraph runs on for long enough that it has to wrap at least \
    once on the page, however wide the page is, so that the distance between its lines \
    and how far it reaches across the sheet can both be measured from the outside.";

/// The app in frames short enough for two clicks to make a double-click: the
/// harness gives every pointer event a frame of its own.
fn kit(app: IboardApp) -> Kit {
    let builder = egui_kittest::Harness::builder()
        .with_size(window().size())
        .with_step_dt(0.02);
    kittest_with(app, builder)
}

fn opened(tag: &str, text: &str) -> (Scratch, PathBuf, Kit) {
    let dir = scratch_dir(tag);
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    let file = todo.join("01-demo.md");
    std::fs::write(&file, text).unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.open_issue(file.clone());
    let harness = kit(app);
    (dir, file, harness)
}

fn rect_of(harness: &Kit, label: &str) -> egui::Rect {
    let found: Vec<_> = harness.get_all_by_label(label).collect();
    found.last().unwrap().rect()
}

#[test]
fn a_paragraph_wraps_at_the_measure_on_24pt_lines() {
    let (_dir, _file, mut harness) = opened("reader-body", &format!("# Demo\n\n{LONG}\n"));
    harness.run_steps(2);
    let rect = rect_of(&harness, LONG);
    assert!(rect.height() >= 48.0, "{rect:?}");
    assert_eq!(rect.height() % 24.0, 0.0, "{rect:?}");
    assert!(rect.width() <= theme::READ_MEASURE + 1.0, "{rect:?}");
    assert!(rect.width() > theme::READ_MEASURE - 60.0, "{rect:?}");
}

#[test]
fn blocks_keep_the_specified_gaps() {
    let text = "# Demo\n\nFirst.\n\n## Context\n\nSecond.\n\n### Notes\n\nThird.\n\n- one\n- two\n";
    let (_dir, _file, mut harness) = opened("reader-gaps", text);
    harness.run_steps(2);
    let labels = [
        "Demo", "First.", "Context", "Second.", "Notes", "Third.", "one", "two",
    ];
    let [title, first, context, second, notes, third, one, two] =
        labels.map(|label| rect_of(&harness, label));
    assert_eq!(title.height(), 26.0);
    assert_eq!(context.height(), 22.0);
    assert_eq!(notes.height(), 20.0);
    assert_eq!(first.top() - title.bottom(), 12.0);
    assert_eq!(context.top() - first.bottom(), 24.0);
    assert_eq!(second.top() - context.bottom(), 6.0);
    assert_eq!(notes.top() - second.bottom(), 16.0);
    assert_eq!(third.top() - notes.bottom(), 4.0);
    assert_eq!(one.top() - third.bottom(), 10.0);
    assert_eq!(two.top() - one.bottom(), 10.0);
    assert_eq!(one.left() - third.left(), 20.0);
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
    assert_eq!(field.rect.width(), theme::READ_MEASURE);
}

/// Two clicks at `pos`.
fn double_click(harness: &mut Kit, pos: egui::Pos2) {
    harness.event(moved(pos));
    harness.step();
    for _ in 0..2 {
        harness.event(button(pos, true));
        harness.event(button(pos, false));
    }
    harness.run_steps(3);
}

/// The line of the file the Editor's caret is on.
fn caret_line(harness: &Kit) -> usize {
    let id = egui::Id::new(crate::app::editor::EDITOR_ID);
    let caret = egui::TextEdit::load_state(&harness.ctx, id)
        .and_then(|state| state.cursor.char_range())
        .expect("the field has a caret")
        .primary
        .index
        .0;
    let editor = harness.state().1.session.editor.as_ref();
    let text = &editor.expect("the Editor is open").text;
    text.chars().take(caret).filter(|c| *c == '\n').count()
}

#[test]
fn a_double_click_on_the_text_edits_at_that_line() {
    let text = "# [bug] Demo\n\nFirst.\n\n## Context\n\n- one\n- two\n- three\n";
    let (_dir, _file, mut harness) = opened("reader-line", text);
    harness.run_steps(2);
    let two = rect_of(&harness, "two").center();
    double_click(&mut harness, two);
    assert_eq!(caret_line(&harness), 7, "on `- two`");
}

#[test]
fn a_double_click_below_the_text_edits_at_the_end() {
    let (_dir, _file, mut harness) = opened("reader-rest", "# Demo\n\nFirst.\n");
    harness.run_steps(2);
    let hint = "Double-click below the text to keep writing";
    let rest = harness.get_by_label(hint).rect().center();
    harness.event(moved(rest));
    harness.step();
    harness.event(button(rest, true));
    harness.event(button(rest, false));
    harness.run_steps(3);
    assert!(harness.state().1.session.editing().is_none(), "one click");
    double_click(&mut harness, rest);
    assert_eq!(caret_line(&harness), 3, "after the last line");
}

#[test]
fn the_editor_brings_the_clicked_line_into_view() {
    let text: String = (0..200).map(|line| format!("Line {line}\n\n")).collect();
    let (_dir, file, mut harness) = opened("reader-far-line", &text);
    harness.run_steps(2);
    harness.state_mut().1.edit_at_line(&file, Some(300));
    harness.run_steps(30);
    assert_eq!(caret_line(&harness), 300);
    let field = harness
        .ctx
        .read_response(egui::Id::new(crate::app::editor::EDITOR_ID))
        .unwrap();
    assert!(field.rect.top() < 0.0, "{:?}", field.rect);
}

/// The column is the breadcrumb's and the path, age and progress the status
/// bar's: none of them shows on the sheet.
#[test]
fn the_sheet_has_no_head_strip() {
    let (_dir, _file, mut harness) = opened("reader-head", "# Demo\n\n- [x] one\n- [ ] two\n");
    harness.run_steps(2);
    let page = harness.state().1.reader.preview_rect;
    for head in ["1/2 done", "modified ", "01-demo.md"] {
        let on_sheet = harness
            .query_all_by_label_contains(head)
            .filter(|node| node.rect().intersects(page))
            .count();
        assert_eq!(on_sheet, 0, "{head}");
    }
}

/// Doing and Done open read-only: no double-click to edit, no foot hint, but
/// ticking a box still writes the file.
#[test]
fn a_doing_issue_is_read_only_with_live_boxes() {
    let dir = scratch_dir("reader-doing");
    let doing = dir.join("issues/doing");
    std::fs::create_dir_all(&doing).unwrap();
    let file = doing.join("01-demo.md");
    std::fs::write(&file, "# Demo\n\nFirst.\n\n- [ ] one\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.open_issue(file.clone());
    let mut harness = kit(app);
    harness.run_steps(2);
    assert_eq!(
        harness.query_all_by_label_contains("keep writing").count(),
        0
    );
    let first = rect_of(&harness, "First.").center();
    double_click(&mut harness, first);
    assert!(harness.state().1.session.editing().is_none());
    harness.get_by_role_and_label(Role::CheckBox, "one").hover();
    harness.run_steps(1);
    harness.get_by_role_and_label(Role::CheckBox, "one").click();
    harness.run_steps(2);
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "# Demo\n\nFirst.\n\n- [x] one\n"
    );
}
