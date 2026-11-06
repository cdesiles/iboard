//! The quiet board: its column heads, its cards at rest and under the strip,
//! the DropZones and the status bar.

use egui_kittest::kittest::Queryable;

use super::kit::{Kit, kittest};
use super::*;
use crate::app::dock::IssueDrag;

const LONG_TITLE: &str = "A title long enough to need far more than the two lines a card allows, \
    as it goes on and on about what the issue is really about, and then some more, until nobody \
    would read it on a card anyway";
const LONG_SUMMARY: &str = "A summary long enough to need far more than the two lines a card \
    allows, as it goes on and on about what the issue is really about, and then some more, until \
    nobody would read it on a card anyway, not even at the widest column there is";

fn project(tag: &str, files: &[(&str, &str)]) -> (Scratch, IboardApp) {
    let dir = scratch_dir(tag);
    for column in ["todo", "ready", "doing", "done"] {
        std::fs::create_dir_all(dir.join("issues").join(column)).unwrap();
    }
    for (file, text) in files {
        std::fs::write(dir.join("issues").join(file), text).unwrap();
    }
    let app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    (dir, app)
}

fn card_rect(harness: &Kit, path: &Path) -> egui::Rect {
    harness
        .ctx
        .read_response(egui::Id::new(("issue_card", path)))
        .expect("the card is drawn")
        .rect
}

fn hover(harness: &mut Kit, at: egui::Pos2) {
    harness.event(egui::Event::PointerMoved(at));
    harness.run_steps(2);
}

/// Where the status bar starts, well enough: the harness insets the app.
fn status_bar_top() -> f32 {
    window().max.y - 2.0 * theme::STATUSBAR_H
}

/// The widgets labelled `label` in the status bar's band.
fn in_status_bar(harness: &Kit, label: &str) -> Vec<egui::Rect> {
    harness
        .query_all_by_label(label)
        .map(|node| node.rect())
        .filter(|rect| rect.min.y >= status_bar_top())
        .collect()
}

/// However a card is shown, hovered, selected or at rest, its height does
/// not change, and nothing below it moves.
#[test]
fn the_strip_never_changes_a_cards_height() {
    let long = format!("# {LONG_TITLE}\n\n{LONG_SUMMARY}\n");
    let cases = [
        ("todo/a.md", "# Title only\n"),
        ("ready/b.md", "# One line\n\nA short summary.\n"),
        ("doing/c.md", long.as_str()),
        ("done/d.md", "# Done, no summary\n"),
        ("done/e.md", "# Done\n\nWith a summary.\n"),
    ];
    let (dir, app) = project("strip-height", &cases);
    let below = dir.join("issues/todo/z.md");
    std::fs::write(&below, "# Below\n").unwrap();
    let mut harness = kittest(IboardApp::from_located(discovery::locate(&dir).unwrap()));
    drop(app);
    let bottom = card_rect(&harness, &below);
    for (file, _) in cases {
        let path = dir.join("issues").join(file);
        hover(&mut harness, window().center() + egui::vec2(0.0, 390.0));
        let rest = card_rect(&harness, &path);

        hover(&mut harness, rest.center());
        assert_eq!(card_rect(&harness, &path), rest, "{file} hovered");
        assert_eq!(card_rect(&harness, &below), bottom, "{file} hovered");

        harness.state_mut().1.select(path.clone());
        harness.run_steps(2);
        assert_eq!(card_rect(&harness, &path), rest, "{file} selected");
        assert_eq!(card_rect(&harness, &below), bottom, "{file} selected");

        harness.state_mut().1.session.selected = None;
        harness.run_steps(2);
    }
}

/// At rest no file name and no actions; hovered, the strip brings both, and
/// takes them away again.
#[test]
fn a_card_at_rest_shows_no_file_name_and_no_actions() {
    let (dir, app) = project("strip-rest", &[("todo/a.md", "# Title\n\nSummary.\n")]);
    let path = dir.join("issues/todo/a.md");
    let mut harness = kittest(app);
    assert!(harness.query_by_label("a.md").is_none());
    assert!(harness.query_by_label("Delete").is_none());
    assert!(harness.query_by_label("Expand").is_none());

    let card = card_rect(&harness, &path);
    hover(&mut harness, card.center());
    harness.get_by_label("a.md");
    harness.get_by_label("Delete");
    harness.get_by_label("Expand");

    hover(&mut harness, window().center() + egui::vec2(0.0, 390.0));
    assert!(harness.query_by_label("a.md").is_none());
}

/// The strip is the card's bottom 28pt: a card with nothing under its title
/// keeps that for it, and it leaves the title alone.
#[test]
fn a_title_alone_keeps_28pt_under_it() {
    let (dir, app) = project("strip-room", &[("todo/a.md", "# Just a title\n")]);
    let path = dir.join("issues/todo/a.md");
    let mut harness = kittest(app);
    let title = harness.get_by_label("Just a title").rect();
    let card = card_rect(&harness, &path);
    assert_eq!(card.max.y - 1.0 - title.max.y, 28.0);

    hover(&mut harness, card.center());
    let delete = harness.get_by_label("Delete").rect();
    assert!(delete.min.y >= title.max.y, "{delete:?} {title:?}");
    assert!(delete.max.y <= card.max.y);
    let file = harness.get_by_label("a.md").rect();
    assert_eq!(
        file.min.x, title.min.x,
        "the file name lines up with the title"
    );
}

/// Title and summary stop at two lines with an ellipsis, and VoiceOver
/// still reads them whole.
#[test]
fn the_title_and_summary_clamp_to_two_lines() {
    let text = format!("# {LONG_TITLE}\n\n{LONG_SUMMARY}\n");
    let (_dir, app) = project("clamp", &[("todo/a.md", &text)]);
    let harness = kittest(app);
    let title = harness.get_by_label(LONG_TITLE).rect();
    assert_eq!(title.height(), 2.0 * 18.0);
    let summary = harness.state().1.board.columns[0].issues[0].summary.clone();
    let summary = harness.get_by_label(&summary).rect();
    assert_eq!(summary.height(), 2.0 * 17.0);
    assert_eq!(
        summary.min.y - title.max.y,
        theme::SPACE_3,
        "rows stand 8pt apart"
    );
}

/// Under the strip the date is said once: the strip's, in the place of the
/// row it covers.
#[test]
fn a_done_card_says_its_date_once() {
    let (dir, app) = project("done-date", &[("done/d.md", "# Done\n\nSummary.\n")]);
    let path = dir.join("issues/done/d.md");
    let mut harness = kittest(app);
    let date = harness.state().1.board.columns[3].issues[0]
        .done_at
        .expect("a Done issue has a date")
        .format("%Y-%m-%d")
        .to_string();
    assert_eq!(harness.query_all_by_label(&date).count(), 1, "at rest");
    let row = harness.get_by_label(&date).rect();
    let card = card_rect(&harness, &path);
    assert_eq!(row.max.y + 11.0, card.max.y, "its last row");

    hover(&mut harness, card.center());
    assert_eq!(harness.query_all_by_label(&date).count(), 1, "hovered");
    harness.get_by_label("Archive");
    assert!(harness.query_by_label("Delete").is_none());
}

/// The keyboard on a card brings its strip out, and the Expand beside it.
#[test]
fn keyboard_focus_shows_the_strip() {
    let (dir, app) = project("strip-focus", &[("todo/a.md", "# Title\n")]);
    let path = dir.join("issues/todo/a.md");
    let mut harness = kittest(app);
    assert!(harness.query_by_label("Add label").is_none());
    harness.ctx.memory_mut(|memory| {
        memory.request_focus(egui::Id::new(("issue_card", &path)));
    });
    harness.run_steps(2);
    harness.get_by_label("Add label");
    harness.get_by_label("Expand");
}

/// Column heads: the title in title case and no coloured rule.
#[test]
fn a_column_head_says_its_name_in_title_case_with_no_rule() {
    let (_dir, mut app) = project("column-head", &[("todo/a.md", "# Title\n")]);
    let ctx = themed_ctx();
    draw(&ctx, &mut app, vec![]);
    let (p, shapes) = painted(&ctx, &mut app, vec![]);
    let rules = shapes.iter().filter(|shape| {
        matches!(shape, egui::Shape::LineSegment { stroke, .. }
            if stroke.width == 2.0
                && [p.status_todo, p.status_ready, p.status_doing, p.status_done]
                    .contains(&stroke.color))
    });
    assert_eq!(rules.count(), 0);

    let (_dir, app) = project("column-title", &[("todo/a.md", "# Title\n")]);
    let harness = kittest(app);
    harness.get_by_label("Todo");
    harness.get_by_label("Done");
    assert!(harness.query_by_label("TODO").is_none());
}

fn filled(shapes: &[egui::Shape], color: egui::Color32) -> usize {
    shapes
        .iter()
        .filter(|shape| matches!(shape, egui::Shape::Rect(rect) if rect.fill == color))
        .count()
}

/// An empty column is a dashed well with no fill, until a card is over it.
#[test]
fn the_empty_well_is_bare_until_a_card_is_over_it() {
    let (dir, mut app) = project("well", &[("todo/a.md", "# Title\n")]);
    let ctx = themed_ctx();
    draw(&ctx, &mut app, vec![]);
    let (p, shapes) = painted(&ctx, &mut app, vec![]);
    assert_eq!(filled(&shapes, p.surface_sunken), 0, "no fill at rest");
    assert_eq!(filled(&shapes, p.accent_soft), 0);

    let start = ctx
        .read_response(egui::Id::new(("issue_card", dir.join("issues/todo/a.md"))))
        .unwrap()
        .rect
        .center();
    let doing = ctx
        .read_response(egui::Id::new(("column_drop", 2)))
        .unwrap()
        .rect;
    let over = doing.min + egui::vec2(40.0, theme::CONTROL_H + theme::SPACE_3 + 12.0);
    draw(&ctx, &mut app, vec![moved(start)]);
    draw(&ctx, &mut app, vec![moved(start), button(start, true)]);
    draw(&ctx, &mut app, vec![moved(start + egui::vec2(30.0, 0.0))]);
    draw(&ctx, &mut app, vec![moved(over)]);
    assert!(egui::DragAndDrop::has_payload_of_type::<IssueDrag>(&ctx));
    let (p, shapes) = painted(&ctx, &mut app, vec![moved(over)]);
    let well: Vec<_> = shapes
        .iter()
        .filter_map(|shape| match shape {
            egui::Shape::Rect(rect) if rect.fill == p.accent_soft => Some(rect.rect),
            _ => None,
        })
        .collect();
    assert_eq!(well.len(), 1, "the well lights, not the whole column");
    assert!(well[0].height() < doing.height() / 2.0, "{well:?}");
}

/// Both zones wait in neutral: `danger` shows only with the card over Delete.
#[test]
fn delete_stays_neutral_until_the_pointer_is_over_it() {
    let (dir, mut app) = project("neutral-delete", &[("todo/a.md", "# Title\n")]);
    let ctx = themed_ctx();
    draw(&ctx, &mut app, vec![]);
    let start = ctx
        .read_response(egui::Id::new(("issue_card", dir.join("issues/todo/a.md"))))
        .unwrap()
        .rect
        .center();
    draw(&ctx, &mut app, vec![moved(start)]);
    draw(&ctx, &mut app, vec![moved(start), button(start, true)]);
    let carried = start + egui::vec2(30.0, 30.0);
    for _ in 0..20 {
        draw(&ctx, &mut app, vec![moved(carried)]);
    }
    let reddish = |shapes: &[egui::Shape], p: &Palette| {
        shapes.iter().any(|shape| match shape {
            egui::Shape::Rect(rect) => rect.fill == p.danger_soft || rect.stroke.color == p.danger,
            egui::Shape::Path(path) => {
                path.stroke.color == egui::epaint::ColorMode::Solid(p.danger)
            }
            egui::Shape::LineSegment { stroke, .. } => stroke.color == p.danger,
            _ => false,
        })
    };
    let (p, shapes) = painted(&ctx, &mut app, vec![moved(carried)]);
    assert!(!reddish(&shapes, p), "no red at rest");
    assert_eq!(filled(&shapes, p.surface_sunken), 0);

    let zone = ctx
        .read_response(egui::Id::new("drop_zone_delete"))
        .unwrap()
        .rect
        .center();
    draw(&ctx, &mut app, vec![moved(zone)]);
    let (p, shapes) = painted(&ctx, &mut app, vec![moved(zone)]);
    assert!(reddish(&shapes, p), "red under the pointer");
}

/// The board's status bar: the issues folder and no total, with what the
/// filter matches beside it; the selected card's path stays on its strip.
#[test]
fn the_board_status_says_the_folder_and_no_total() {
    let (dir, app) = project(
        "status-board",
        &[
            ("todo/a.md", "# [bug] First\n"),
            ("todo/b.md", "# Second\n"),
        ],
    );
    let mut harness = kittest(app);
    let folder = display_path(&harness.state().1.board.root);
    assert_eq!(in_status_bar(&harness, &folder).len(), 1);
    assert!(harness.query_by_label("2 issues").is_none());

    let path = dir.join("issues/todo/a.md");
    harness.state_mut().1.select(path);
    harness.run_steps(2);
    assert_eq!(in_status_bar(&harness, &folder).len(), 1, "selected");
    assert!(harness.query_by_label("todo/a.md").is_none());
    harness.state_mut().1.session.selected = None;

    harness.state_mut().1.filter_bar.filter.pick("bug");
    harness.run_steps(2);
    assert_eq!(in_status_bar(&harness, &folder).len(), 1, "filtered");
    let matches = "1 of 2 issues matches “bug”";
    assert_eq!(in_status_bar(&harness, matches).len(), 1);
    assert!(
        harness.query_by_label("1 of 2").is_none(),
        "the filter field does not repeat it"
    );
}

/// The Reader's status bar: its path, how old it is, then its checklist or
/// its words.
#[test]
fn the_reader_status_says_the_path_age_and_progress() {
    let (dir, app) = project(
        "status-reader",
        &[
            (
                "todo/a.md",
                "# Tasks\n\n- [x] one\n- [ ] two\n- [ ] three\n",
            ),
            ("todo/b.md", "# Prose\n\nThree more words\n"),
        ],
    );
    let mut harness = kittest(app);
    harness
        .state_mut()
        .1
        .open_issue(dir.join("issues/todo/a.md"));
    harness.run_steps(2);
    assert_eq!(in_status_bar(&harness, "todo/a.md").len(), 1);
    assert_eq!(in_status_bar(&harness, "1/3 done").len(), 1);
    let ago = harness
        .query_all_by_label_contains("modified ")
        .map(|node| node.rect().min.y)
        .filter(|y| *y >= status_bar_top())
        .count();
    assert_eq!(ago, 1);

    harness
        .state_mut()
        .1
        .open_issue(dir.join("issues/todo/b.md"));
    harness.run_steps(2);
    assert_eq!(in_status_bar(&harness, "todo/b.md").len(), 1);
    let words = harness
        .query_all_by_label_contains(" words")
        .map(|node| node.rect().min.y)
        .filter(|y| *y >= status_bar_top())
        .count();
    assert_eq!(words, 1);
}

/// The Editor's save state sits at the bar's far end, with the path on the
/// left; ⌘ brings the keys in beside it without moving it.
#[test]
fn the_editor_status_puts_the_save_state_on_the_right() {
    let (dir, app) = project("status-editor", &[("todo/a.md", "# Title\n")]);
    let path = dir.join("issues/todo/a.md");
    let mut harness = kittest(app);
    harness.state_mut().1.open_issue(path.clone());
    harness.state_mut().1.start_editing(&path);
    harness.run_steps(2);
    let [file] = in_status_bar(&harness, "todo/a.md")[..] else {
        panic!("the path shows once");
    };
    assert!(file.max.x < window().center().x);
    let pill = ["Saved", "Unsaved"]
        .into_iter()
        .find_map(|word| in_status_bar(&harness, word).first().copied())
        .expect("the save state shows");
    assert!(pill.min.x > window().center().x, "{pill:?}");
    let shown = harness.query_all_by_label("Saved").count();
    assert_eq!(shown, 1, "the sheet does not repeat it");
}

/// A head with nothing to offer keeps its count under the pointer; Todo's `+`
/// takes its place.
#[test]
fn a_head_with_no_actions_keeps_its_count_under_the_pointer() {
    let (_dir, app) = project("head-count", &[]);
    let mut harness = kittest(app);
    let counts = |harness: &Kit| harness.query_all_by_label("0").count();
    assert_eq!(counts(&harness), 4);
    for head in ["Done", "Ready"] {
        let at = harness.get_by_label(head).rect();
        hover(
            &mut harness,
            egui::Pos2::new(at.center().x as f32, at.center().y as f32),
        );
        assert_eq!(counts(&harness), 4, "{head}");
    }
    let at = harness.get_by_label("Todo").rect();
    hover(
        &mut harness,
        egui::Pos2::new(at.center().x as f32, at.center().y as f32),
    );
    assert_eq!(counts(&harness), 3, "Todo offers its +");
}
