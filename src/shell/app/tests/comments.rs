//! Margin comments: written in the reader beside a block, kept beside the
//! issue, and handed to the next review.

use egui_kittest::kittest::Queryable;

use super::*;

/// The note's composer is one field, outlined in `review`: its text has no
/// frame of its own inside that outline.
#[test]
fn a_margin_comments_field_has_one_edge() {
    let (dir, mut app) = project(
        "margin-field",
        &[("todo/demo.md", "# Demo\n\nA paragraph.\n")],
    );
    app.open_issue(dir.join("issues/todo/demo.md"));
    let mut harness = kittest(app);
    harness
        .get_all_by_label("Add a comment")
        .nth(1)
        .unwrap()
        .click();
    harness.run_steps(2);

    let app = &mut harness.state_mut().1;
    let ctx = themed_ctx();
    draw(&ctx, app, vec![]);
    let (p, shapes) = painted(&ctx, app, vec![]);
    let edged: Vec<_> = shapes
        .iter()
        .filter_map(|shape| match shape {
            egui::Shape::Rect(rect) if !rect.stroke.is_empty() => Some(rect),
            _ => None,
        })
        .collect();
    let outline = edged
        .iter()
        .find(|rect| rect.stroke.color == p.review)
        .expect("the outline");
    let inside = edged.iter().filter(|rect| {
        rect.rect != outline.rect
            && outline.rect.contains_rect(rect.rect)
            && rect.rect.height() > theme::CONTROL_H
    });
    assert_eq!(inside.count(), 0, "no second frame around the text");
}

/// A comment written in the reader's margin is kept beside the issue and
/// reaches the next review, which marks it seen.
#[test]
fn a_margin_comment_reaches_the_review() {
    let (dir, mut app) = project(
        "margin-comment",
        &[("todo/demo.md", "# Demo\n\nThree files over 1,100 lines.\n")],
    );
    let file = dir.join("issues/todo/demo.md");
    app.prefs.agent = fake_agent("grep -q 'Only design.rs matters' {prompt} && printf noted");
    app.open_issue(file.clone());
    let mut harness = kittest(app);

    let buttons: Vec<_> = harness.get_all_by_label("Add a comment").collect();
    assert_eq!(
        buttons.len(),
        2,
        "one per block: the title and the paragraph"
    );
    buttons[1].click();
    harness.run_steps(2);
    harness
        .get_by_role(egui::accesskit::Role::MultilineTextInput)
        .type_text("Only design.rs matters now.");
    harness.run_steps(1);
    harness.get_by_label("Comment").click();
    harness.run_steps(2);

    let sidecar = reviews::load(&dir.join("issues"), &file);
    assert_eq!(sidecar.comments.len(), 1);
    assert_eq!(sidecar.comments[0].anchor, "Three files over 1,100 lines.");
    assert!(!sidecar.comments[0].sent);

    let app = &mut harness.state_mut().1;
    app.start_review(&file, false);
    landed(app, &file);
    assert_eq!(
        app.past_reviews[&file].raw, "noted",
        "the prompt quoted the note"
    );
    assert!(reviews::load(&dir.join("issues"), &file).comments[0].sent);
}
