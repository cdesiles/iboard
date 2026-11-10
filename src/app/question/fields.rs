//! The Question card's fields, option rows and permission scope.

use super::{
    AnswerDraft, AnswerValue, Choice, CornerRadius, Field, FieldKind, Focus, Frame, Icon, Key,
    Path, PathBuf, Permission, Pos2, Question, QuestionKind, RichText, Sense, Stroke, StrokeKind,
    Vec2, default_value, file_name, theme, widgets,
};

pub(super) fn answer_body(
    ui: &mut egui::Ui,
    draft: &mut AnswerDraft,
    question: &Question,
    heading: &str,
    outside: bool,
    project: Option<&Path>,
) {
    let AnswerDraft {
        values,
        field_error,
        focus,
        ..
    } = draft;
    match &question.kind {
        QuestionKind::Form(fields) => {
            ui.spacing_mut().item_spacing.y = theme::SPACE_4;
            for field in fields.iter().filter(|field| !field.hidden) {
                let value = values
                    .entry(field.key.clone())
                    .or_insert_with(|| default_value(&field.kind));
                let error = field_error
                    .as_ref()
                    .filter(|error| error.key == field.key)
                    .map(|error| error.message.as_str());
                field_ui(ui, question, field, value, heading, error, focus);
            }
        }
        QuestionKind::Permission(permission) => permission_body(ui, permission, project, outside),
    }
    *focus = None;
}

fn field_ui(
    ui: &mut egui::Ui,
    question: &Question,
    field: &Field,
    value: &mut AnswerValue,
    heading: &str,
    error: Option<&str>,
    focus: &mut Option<Focus>,
) {
    let p = theme::palette(ui);
    let show_label = !field.title.trim().is_empty() && field.title.trim() != heading.trim();
    if show_label {
        field_label(ui, field);
    }
    if !field.description.is_empty() {
        widgets::help(ui, &field.description, p.ink_muted);
        ui.add_space(theme::SPACE_2);
    }
    let wants_field = focus
        .as_ref()
        .is_some_and(|focus| matches!(focus, Focus::Field(key) if key == &field.key));
    if wants_field {
        *focus = None;
    }
    match (&field.kind, value) {
        (
            FieldKind::Text {
                options,
                placeholder,
                ..
            },
            AnswerValue::Text { text, other },
        ) => {
            if options.is_empty() {
                let id = field_id(&question.id, &field.key, "text");
                let response = text_area(ui, id, text, placeholder, 3);
                if wants_field {
                    response.request_focus();
                }
            } else {
                text_options(ui, question, field, options, text, other, wants_field);
            }
        }
        (FieldKind::Number { integer, .. }, AnswerValue::Text { text, .. }) => {
            let id = field_id(&question.id, &field.key, "number");
            let hint = if *integer { "42" } else { "0" };
            let response = line_field(ui, id, text, hint, Some(120.0));
            if wants_field {
                response.request_focus();
            }
        }
        (FieldKind::Boolean { .. }, AnswerValue::Boolean(checked)) => {
            boolean(ui, question, field, checked, wants_field);
        }
        (
            FieldKind::Multi {
                options, custom, ..
            },
            AnswerValue::Multi {
                chosen,
                other,
                custom: text,
            },
        ) => {
            multi(
                ui,
                question,
                field,
                options,
                chosen,
                *custom,
                other,
                text,
                wants_field,
            );
        }
        (FieldKind::External { url }, _) if !url.is_empty() => {
            ui.hyperlink_to("Open form", url);
        }
        _ => {}
    }
    if let Some(error) = error {
        ui.add_space(theme::SPACE_2);
        error_line(ui, error);
    }
}

fn field_label(ui: &mut egui::Ui, field: &Field) {
    let p = theme::palette(ui);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_2;
        ui.label(
            RichText::new(&field.title)
                .font(theme::ui_strong())
                .color(p.ink),
        );
        if !field.required {
            ui.label(
                RichText::new("Optional")
                    .font(theme::meta())
                    .color(p.ink_faint),
            );
        }
    });
    ui.add_space(theme::SPACE_2);
}

fn text_area(
    ui: &mut egui::Ui,
    id: egui::Id,
    text: &mut String,
    hint: &str,
    rows: usize,
) -> egui::Response {
    let p = theme::palette(ui);
    let frame = Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.line_strong))
        .corner_radius(CornerRadius::same(theme::RADIUS_SM))
        .inner_margin(Vec2::new(theme::SPACE_3, theme::SPACE_2))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add(
                egui::TextEdit::multiline(text)
                    .id(id)
                    .font(theme::ui_body())
                    .hint_text(hint)
                    .desired_rows(rows)
                    .desired_width(f32::INFINITY)
                    .lock_focus(true)
                    .frame(egui::Frame::NONE)
                    .margin(Vec2::ZERO),
            )
        });
    let rect = frame.response.rect;
    let response = frame.inner;
    if response.has_focus() {
        widgets::paint_focus_halo(ui, rect, theme::RADIUS_SM);
    }
    response
}

fn line_field(
    ui: &mut egui::Ui,
    id: egui::Id,
    text: &mut String,
    hint: &str,
    width: Option<f32>,
) -> egui::Response {
    let p = theme::palette(ui);
    let frame = Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.line_strong))
        .corner_radius(CornerRadius::same(theme::RADIUS_SM))
        .inner_margin(Vec2::new(theme::SPACE_3, 4.0))
        .show(ui, |ui| {
            if let Some(width) = width {
                ui.set_width(width);
            }
            ui.add(
                egui::TextEdit::singleline(text)
                    .id(id)
                    .font(theme::ui_body())
                    .hint_text(hint)
                    .desired_width(f32::INFINITY)
                    .frame(egui::Frame::NONE)
                    .margin(Vec2::ZERO),
            )
        });
    let rect = frame.response.rect;
    let response = frame.inner;
    if response.has_focus() {
        widgets::paint_focus_halo(ui, rect, theme::RADIUS_SM);
    }
    response
}

fn text_options(
    ui: &mut egui::Ui,
    question: &Question,
    field: &Field,
    options: &[Choice],
    text: &mut String,
    other: &mut bool,
    wants_field: bool,
) {
    let FieldKind::Text { custom, .. } = &field.kind else {
        return;
    };
    let other_index = custom.then_some(options.len());
    let mut focus_other = false;
    let mut ids = Vec::new();
    let mut picked = None;
    for (index, option) in options.iter().enumerate() {
        ids.push(option_id(&question.id, &field.key, index));
        let selected = !*other && text == &option.value;
        let response = option_row(
            ui,
            ids[index],
            OptionRow {
                title: &option.label,
                description: &option.description,
                number: index + 1,
                selected,
                role: egui::accesskit::Role::RadioButton,
            },
        );
        if response.clicked() {
            picked = Some(index);
        }
    }
    if *custom {
        let index = options.len();
        ids.push(option_id(&question.id, &field.key, index));
        let response = option_row(
            ui,
            ids[index],
            OptionRow {
                title: "Other…",
                description: "Describe your own answer.",
                number: index + 1,
                selected: *other,
                role: egui::accesskit::Role::RadioButton,
            },
        );
        if response.clicked() {
            picked = Some(index);
        }
    }
    if let Some(at) = picked.or_else(|| row_keys(ui, &ids)) {
        focus_other = Some(at) == other_index && !*other;
        pick_text(options, at, other_index, text, other);
    }
    if *other {
        ui.add_space(theme::SPACE_2);
        let id = field_id(&question.id, &field.key, "other");
        let response = line_field(ui, id, text, "Your answer", None);
        if focus_other || wants_field {
            response.request_focus();
        }
    } else if wants_field && !ids.is_empty() {
        let at = options
            .iter()
            .position(|option| option.value == *text)
            .unwrap_or(0);
        ui.memory_mut(|memory| memory.request_focus(ids[at]));
    }
}

fn pick_text(
    options: &[Choice],
    at: usize,
    other: Option<usize>,
    text: &mut String,
    other_on: &mut bool,
) {
    if Some(at) == other {
        if !*other_on {
            text.clear();
        }
        *other_on = true;
        return;
    }
    if let Some(option) = options.get(at) {
        *text = option.value.clone();
        *other_on = false;
    }
}

fn boolean(
    ui: &mut egui::Ui,
    question: &Question,
    field: &Field,
    checked: &mut Option<bool>,
    wants_field: bool,
) {
    let mut ids = Vec::new();
    let mut picked = None;
    for (index, (title, value)) in [("Yes", true), ("No", false)].into_iter().enumerate() {
        ids.push(option_id(&question.id, &field.key, index));
        let response = option_row(
            ui,
            ids[index],
            OptionRow {
                title,
                description: "",
                number: index + 1,
                selected: *checked == Some(value),
                role: egui::accesskit::Role::RadioButton,
            },
        );
        if response.clicked() {
            picked = Some(value);
        }
    }
    if let Some(at) = row_keys(ui, &ids) {
        picked = Some(at == 0);
    }
    if let Some(value) = picked {
        *checked = Some(value);
    }
    if wants_field && !ids.is_empty() {
        let at = usize::from(*checked == Some(false));
        ui.memory_mut(|memory| memory.request_focus(ids[at]));
    }
}

#[allow(clippy::too_many_arguments)]
fn multi(
    ui: &mut egui::Ui,
    question: &Question,
    field: &Field,
    options: &[Choice],
    chosen: &mut Vec<String>,
    custom: bool,
    other: &mut bool,
    text: &mut String,
    wants_field: bool,
) {
    let mut ids = Vec::new();
    let mut toggled = None;
    let mut focus_other = false;
    for (index, option) in options.iter().enumerate() {
        ids.push(option_id(&question.id, &field.key, index));
        let selected = chosen.contains(&option.value);
        let response = option_row(
            ui,
            ids[index],
            OptionRow {
                title: &option.label,
                description: &option.description,
                number: index + 1,
                selected,
                role: egui::accesskit::Role::CheckBox,
            },
        );
        if response.clicked() {
            toggled = Some(index);
        }
    }
    if custom {
        let index = options.len();
        ids.push(option_id(&question.id, &field.key, index));
        let response = option_row(
            ui,
            ids[index],
            OptionRow {
                title: "Other…",
                description: "Add a value of your own.",
                number: index + 1,
                selected: *other,
                role: egui::accesskit::Role::CheckBox,
            },
        );
        if response.clicked() {
            toggled = Some(index);
        }
    }
    if let Some(at) = toggled.or_else(|| row_keys(ui, &ids)) {
        if custom && at == options.len() {
            focus_other = !*other;
            *other = !*other;
        } else if let Some(option) = options.get(at) {
            match chosen.iter().position(|value| value == &option.value) {
                Some(at) => {
                    chosen.remove(at);
                }
                None => chosen.push(option.value.clone()),
            }
        }
    }
    if *other {
        ui.add_space(theme::SPACE_2);
        let id = field_id(&question.id, &field.key, "other");
        let response = line_field(ui, id, text, "Your own value", None);
        if focus_other || wants_field {
            response.request_focus();
        }
    } else if wants_field && !ids.is_empty() {
        ui.memory_mut(|memory| memory.request_focus(ids[0]));
    }
}

struct OptionRow<'a> {
    title: &'a str,
    description: &'a str,
    number: usize,
    selected: bool,
    role: egui::accesskit::Role,
}

fn option_row(ui: &mut egui::Ui, id: egui::Id, row: OptionRow<'_>) -> egui::Response {
    let p = theme::palette(ui);
    let pad = Vec2::new(theme::SPACE_3, theme::SPACE_3);
    let key_width = 18.0;
    let mark = 14.0;
    let gap = theme::SPACE_3;
    let width = ui.available_width();
    let text_width = (width - 2.0 * pad.x - key_width - mark - 2.0 * gap).max(48.0);
    let painter = ui.painter();
    let title = painter.layout(row.title.to_string(), theme::ui_strong(), p.ink, text_width);
    let description = (!row.description.is_empty()).then(|| {
        painter.layout(
            row.description.to_string(),
            theme::ui_small(),
            p.ink_muted,
            text_width,
        )
    });
    let text_height = title.size().y
        + description
            .as_ref()
            .map_or(0.0, |description| 2.0 + description.size().y);
    let height = (text_height + 2.0 * pad.y).max(36.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let response = ui.interact(rect, id, Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    response.widget_info(|| {
        egui::WidgetInfo::selected(row.role, ui.is_enabled(), row.selected, row.title)
    });
    if !row.description.is_empty() {
        ui.ctx().accesskit_node_builder(id, |node| {
            node.set_description(row.description);
        });
    }
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let (fill, edge, key_ink) = if row.selected {
        (p.accent_soft, Stroke::new(1.0, p.accent), p.accent)
    } else if response.hovered() {
        (
            p.surface_hover,
            Stroke::new(1.0, p.line_strong),
            p.ink_muted,
        )
    } else {
        (p.surface, Stroke::new(1.0, p.line_soft), p.ink_muted)
    };
    let painter = ui.painter();
    painter.rect(
        rect,
        CornerRadius::same(theme::RADIUS_SM),
        fill,
        edge,
        StrokeKind::Inside,
    );
    let top = rect.min.y + pad.y;
    let first_line = title
        .rows
        .first()
        .map_or(title.size().y, |row| row.rect().height());
    let line_center = top + first_line / 2.0;
    widgets::paint_kbd_in(
        ui,
        &row.number.to_string(),
        Pos2::new(rect.min.x + pad.x, line_center),
        key_ink,
    );
    let text_x = rect.min.x + pad.x + key_width + gap;
    painter.galley(Pos2::new(text_x, top), title, p.ink);
    if let Some(description) = description {
        painter.galley(
            Pos2::new(text_x, top + first_line + 2.0),
            description,
            p.ink_muted,
        );
    }
    let mark_center = Pos2::new(rect.max.x - pad.x - mark / 2.0, line_center);
    if row.role == egui::accesskit::Role::CheckBox {
        paint_checkbox(painter, mark_center, mark, row.selected, p);
    } else {
        paint_radio(painter, mark_center, row.selected, p);
    }
    widgets::focus_ring(ui, &response, theme::RADIUS_SM);
    response
}

fn paint_radio(painter: &egui::Painter, center: Pos2, selected: bool, p: &theme::Palette) {
    if selected {
        painter.circle_stroke(center, 6.0, Stroke::new(2.0, p.accent));
        painter.circle_filled(center, 3.0, p.accent);
    } else {
        painter.circle_stroke(center, 6.0, Stroke::new(1.0, p.line_strong));
    }
}

fn paint_checkbox(
    painter: &egui::Painter,
    center: Pos2,
    size: f32,
    checked: bool,
    p: &theme::Palette,
) {
    let rect = egui::Rect::from_center_size(center, Vec2::splat(size));
    if checked {
        painter.rect_filled(rect, CornerRadius::same(3), p.accent);
        let at = |x: f32, y: f32| rect.min + Vec2::new(x, y);
        painter.line(
            vec![at(3.5, 7.0), at(6.0, 9.5), at(10.5, 4.5)],
            Stroke::new(2.0, p.on_accent),
        );
    } else {
        painter.rect(
            rect,
            CornerRadius::same(3),
            p.surface,
            Stroke::new(1.0, p.line_strong),
            StrokeKind::Inside,
        );
    }
}

fn permission_body(
    ui: &mut egui::Ui,
    permission: &Permission,
    project: Option<&Path>,
    outside: bool,
) {
    let p = theme::palette(ui);
    egui::ScrollArea::vertical()
        .id_salt(("permission_resources", &permission.action))
        .max_height(96.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = theme::SPACE_2;
            for resource in &permission.resources {
                Frame::new()
                    .fill(p.surface_sunken)
                    .corner_radius(CornerRadius::same(theme::RADIUS_SM))
                    .inner_margin(Vec2::new(theme::SPACE_3, theme::SPACE_2))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.add(
                            egui::Label::new(
                                RichText::new(resource)
                                    .font(theme::edit_body())
                                    .color(p.ink),
                            )
                            .wrap(),
                        );
                    });
            }
        });
    ui.add_space(theme::SPACE_3);
    if outside {
        Frame::new()
            .fill(p.danger_soft)
            .corner_radius(CornerRadius::same(theme::RADIUS_SM))
            .inner_margin(Vec2::new(theme::SPACE_3, theme::SPACE_2))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                error_line(
                    ui,
                    &format!(
                        "Outside {}. The builder should only work in its own folder.",
                        project_label(project)
                    ),
                );
            });
    } else {
        ui.label(
            RichText::new(format!("Inside {}", project_label(project)))
                .font(theme::meta())
                .color(p.ink_muted),
        );
    }
    if let Some(message) = &permission.message {
        ui.add_space(theme::SPACE_2);
        widgets::help(ui, message, p.ink_muted);
    }
}

pub(super) fn error_line(ui: &mut egui::Ui, text: &str) {
    let p = theme::palette(ui);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_2;
        let (icon, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
        theme::paint_icon(ui.painter(), Icon::Warning, icon, p.danger);
        ui.add(
            egui::Label::new(RichText::new(text).font(theme::ui_small()).color(p.danger)).wrap(),
        );
    });
}

pub(super) fn project_label(project: Option<&Path>) -> String {
    project
        .map(|project| format!("{}/", file_name(project)))
        .unwrap_or_else(|| "the project".to_string())
}

pub(super) fn permission_outside(permission: &Permission, project: Option<&Path>) -> bool {
    let Some(project) = project else {
        return true;
    };
    permission
        .resources
        .iter()
        .any(|resource| resource_outside(resource, project))
}

fn resource_outside(resource: &str, project: &Path) -> bool {
    let home = std::env::var("HOME").ok().map(PathBuf::from);
    resource.split_whitespace().any(|token| {
        let token = token.trim_matches(|c: char| {
            matches!(
                c,
                '"' | '\'' | '`' | ',' | ';' | '(' | ')' | '[' | ']' | '{' | '}' | ':'
            )
        });
        if let Some(rest) = token.strip_prefix("~/") {
            let Some(home) = &home else {
                return true;
            };
            return !within(&home.join(rest), project);
        }
        if let Some(rest) = token.strip_prefix('~') {
            let Some(home) = &home else {
                return true;
            };
            return !within(&home.join(rest.trim_start_matches('/')), project);
        }
        let path = if token.starts_with('/') {
            PathBuf::from(token)
        } else if token == ".." || token.starts_with("../") || token.starts_with("./") {
            project.join(token)
        } else {
            return false;
        };
        !within(&path, project)
    })
}

fn within(path: &Path, root: &Path) -> bool {
    normalize(path).starts_with(normalize(root))
}

fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            component => out.push(component.as_os_str()),
        }
    }
    out
}

fn field_id(question: &str, key: &str, part: &str) -> egui::Id {
    egui::Id::unique(("answer_field", question, key, part))
}

fn option_id(question: &str, key: &str, index: usize) -> egui::Id {
    egui::Id::unique(("answer_option", question, key, index))
}

const NUMBERS: [Key; 9] = [
    Key::Num1,
    Key::Num2,
    Key::Num3,
    Key::Num4,
    Key::Num5,
    Key::Num6,
    Key::Num7,
    Key::Num8,
    Key::Num9,
];

fn row_keys(ui: &mut egui::Ui, ids: &[egui::Id]) -> Option<usize> {
    let focused = ui.memory(|memory| memory.focused())?;
    let at = ids.iter().position(|id| *id == focused)?;
    let mut picked = None;
    let mut next = None;
    ui.input_mut(|input| {
        if ids.len() > 1 {
            if input.consume_key(egui::Modifiers::NONE, Key::ArrowDown) {
                next = Some((at + 1) % ids.len());
            }
            if input.consume_key(egui::Modifiers::NONE, Key::ArrowUp) {
                next = Some((at + ids.len() - 1) % ids.len());
            }
        }
        if input.consume_key(egui::Modifiers::NONE, Key::Space) {
            picked = Some(at);
        }
        if let Some(index) = NUMBERS[..ids.len().min(NUMBERS.len())]
            .iter()
            .position(|key| input.consume_key(egui::Modifiers::NONE, *key))
        {
            picked = Some(index);
        }
    });
    if let Some(next) = next {
        ui.memory_mut(|memory| memory.request_focus(ids[next]));
    }
    picked
}
