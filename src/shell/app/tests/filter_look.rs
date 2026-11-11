//! The header's filter: its placeholder sits on the field's middle line.

use super::*;
use crate::shell::app::filter_bar::filter_id;

/// Where the "Filter issues" placeholder is drawn, its middle.
fn placeholder_middle(shapes: &[egui::Shape]) -> Option<f32> {
    shapes.iter().find_map(|shape| match shape {
        egui::Shape::Vec(inner) => placeholder_middle(inner),
        egui::Shape::Text(text) if text.galley.text() == "Filter issues" => {
            let row = text.galley.rows.first()?;
            Some(text.pos.y + row.pos.y + row.size.y / 2.0)
        }
        _ => None,
    })
}

/// The placeholder sits on the field's middle line, as its icon and the
/// header's buttons do.
#[test]
fn the_filter_placeholder_sits_on_the_fields_middle_line() {
    let (_dir, mut app) = project("filter-placeholder", &[]);
    let ctx = themed_ctx();
    draw(&ctx, &mut app, vec![]);
    let (_, shapes) = painted(&ctx, &mut app, vec![]);
    let field = ctx.read_response(filter_id()).unwrap().rect;
    let middle = placeholder_middle(&shapes).expect("the placeholder is drawn");
    assert!(
        (middle - field.center().y).abs() < 0.75,
        "the placeholder's middle {middle} on the field's {}",
        field.center().y
    );
}
