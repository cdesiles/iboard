//! The form a build waits on: its fields on the card, answered in a modal
//! over the board. Submitting resumes the service's run.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use super::{
    ButtonKind, Duration, Frame, IboardApp, Instant, Job, Key, PathBuf, RichText, Vec2, file_name,
    job, theme, widgets,
};
use crate::agent::service::{
    Field, FieldKind, Permission, PermissionReply, Question, QuestionKind,
};

/// One question open for an answer.
pub(super) struct AnswerDraft {
    pub(super) path: PathBuf,
    question: Question,
    values: BTreeMap<String, AnswerValue>,
    error: Option<String>,
    reply: Option<Job<Result<(), String>>>,
}

#[derive(Clone, PartialEq, Eq)]
enum AnswerValue {
    Text(String),
    Boolean(bool),
    Multi(Vec<String>),
}

/// What the dialog's buttons asked for.
#[derive(Clone, Copy)]
enum Choice {
    Close,
    Send,
    Permission(PermissionReply),
}

impl AnswerDraft {
    fn new(path: PathBuf, question: Question) -> Self {
        let values = match &question.kind {
            QuestionKind::Form(fields) => fields
                .iter()
                .filter(|field| !field.hidden)
                .map(|field| (field.key.clone(), default_value(&field.kind)))
                .collect(),
            QuestionKind::Permission(_) => BTreeMap::new(),
        };
        Self {
            path,
            question,
            values,
            error: None,
            reply: None,
        }
    }

    /// The form's answers, as the service reads them.
    fn form_answer(&self) -> Result<Map<String, Value>, String> {
        let QuestionKind::Form(fields) = &self.question.kind else {
            return Ok(Map::new());
        };
        let mut answer = Map::new();
        for field in fields.iter().filter(|field| !field.hidden) {
            let value = self.values.get(&field.key);
            match (&field.kind, value) {
                (
                    FieldKind::Text {
                        options, custom, ..
                    },
                    Some(AnswerValue::Text(text)),
                ) => {
                    let text = text.trim();
                    if text.is_empty() {
                        if field.required {
                            return Err(missing(&field.title));
                        }
                        continue;
                    }
                    if !options.is_empty()
                        && !custom
                        && !options.iter().any(|option| option.value == text)
                    {
                        return Err(format!("Choose an option for {}.", field.title));
                    }
                    answer.insert(field.key.clone(), Value::String(text.to_string()));
                }
                (FieldKind::Number { integer, .. }, Some(AnswerValue::Text(text))) => {
                    let text = text.trim();
                    if text.is_empty() {
                        if field.required {
                            return Err(missing(&field.title));
                        }
                        continue;
                    }
                    answer.insert(field.key.clone(), number(text, *integer)?);
                }
                (FieldKind::Boolean { .. }, Some(AnswerValue::Boolean(checked))) => {
                    answer.insert(field.key.clone(), Value::Bool(*checked));
                }
                (
                    FieldKind::Multi {
                        options, custom, ..
                    },
                    Some(AnswerValue::Multi(values)),
                ) => {
                    if values.is_empty() {
                        if field.required {
                            return Err(missing(&field.title));
                        }
                        continue;
                    }
                    if !custom
                        && let Some(unknown) = values
                            .iter()
                            .find(|value| !options.iter().any(|option| &option.value == *value))
                    {
                        return Err(format!("Choose options for {}: {unknown}.", field.title));
                    }
                    let values = values.iter().cloned().map(Value::String).collect();
                    answer.insert(field.key.clone(), Value::Array(values));
                }
                _ => {}
            }
        }
        Ok(answer)
    }
}

fn default_value(kind: &FieldKind) -> AnswerValue {
    match kind {
        FieldKind::Text { default, .. } => AnswerValue::Text(default.clone()),
        FieldKind::Number { default, .. } => AnswerValue::Text(default.clone()),
        FieldKind::Boolean { default } => AnswerValue::Boolean(*default),
        FieldKind::Multi { default, .. } => AnswerValue::Multi(default.clone()),
        FieldKind::External { .. } => AnswerValue::Text(String::new()),
    }
}

fn missing(title: &str) -> String {
    format!("Answer {title} first.")
}

fn number(text: &str, integer: bool) -> Result<Value, String> {
    if integer {
        return text
            .parse::<i64>()
            .map(Value::from)
            .map_err(|_| format!("{text} is not a whole number."));
    }
    text.parse::<f64>()
        .ok()
        .and_then(serde_json::Number::from_f64)
        .map(Value::Number)
        .ok_or_else(|| format!("{text} is not a number."))
}

impl IboardApp {
    /// Opens the question the build on `path` waits on.
    pub(super) fn open_answer(&mut self, path: PathBuf) {
        if let Some(question) = self.question_for(&path) {
            self.session.answer = Some(AnswerDraft::new(path, question));
        }
    }

    /// Sends the open question's answer: a form's fields, or a tool approval's
    /// decision.
    pub(super) fn send_answer(&mut self, permission: Option<PermissionReply>) {
        let Some(draft) = self.session.answer.as_ref() else {
            return;
        };
        if draft.reply.is_some() {
            return;
        }
        let question = draft.question.clone();
        let Some(project) = self.question_project(&question) else {
            self.fail_answer("The build is no longer running.".to_string());
            return;
        };
        let answer = match &question.kind {
            QuestionKind::Form(_) => match draft.form_answer() {
                Ok(answer) => Some(answer),
                Err(error) => {
                    self.fail_answer(error);
                    return;
                }
            },
            QuestionKind::Permission(_) => None,
        };
        if matches!(&question.kind, QuestionKind::Permission(_)) && permission.is_none() {
            return;
        }
        let (session, id) = (question.session.clone(), question.id.clone());
        let result = match (answer, permission) {
            (Some(answer), _) => Job::spawn(move |_| {
                crate::agent::service::answer_form(&project, &session, &id, &answer)
            }),
            (None, Some(reply)) => Job::spawn(move |_| {
                crate::agent::service::answer_permission(&project, &session, &id, reply)
            }),
            (None, None) => return,
        };
        if let Some(draft) = self.session.answer.as_mut() {
            draft.error = None;
            draft.reply = Some(result);
        }
    }

    fn fail_answer(&mut self, error: String) {
        if let Some(draft) = self.session.answer.as_mut() {
            draft.error = Some(error);
        }
    }

    /// Takes the reply once the service has it: the question leaves its card,
    /// and the run carries on.
    pub(super) fn poll_answers(&mut self) -> Option<Duration> {
        let finished = self.session.answer.as_ref().and_then(|draft| {
            let result = draft.reply.as_ref()?.answer()?;
            Some((draft.question.clone(), result))
        });
        if let Some((question, result)) = finished {
            match result {
                Ok(()) => {
                    self.forget_question(&question);
                    self.session.answer = None;
                    let notice = format!("Answered {}", question.title);
                    self.session.notice = Some((notice, Instant::now()));
                }
                Err(error) => {
                    if let Some(draft) = self.session.answer.as_mut() {
                        draft.reply = None;
                        draft.error = Some(error);
                    }
                }
            }
        }
        let waiting = self
            .session
            .answer
            .as_ref()
            .is_some_and(|draft| draft.reply.is_some());
        waiting.then_some(job::POLL_EVERY)
    }

    /// Makes the open answer's reply land now, for a test without a service.
    #[cfg(test)]
    pub(super) fn finish_answer_for_test(&mut self) {
        if let Some(draft) = self.session.answer.as_mut() {
            draft.reply = Some(Job::done(Ok(())));
        }
    }

    /// The dialog's keys: `⌘↩` sends, `esc` closes.
    pub(super) fn answer_keys(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::Enter)) {
            let permission = self.session.answer.as_ref().and_then(|draft| {
                matches!(&draft.question.kind, QuestionKind::Permission(_))
                    .then_some(PermissionReply::Once)
            });
            self.send_answer(permission);
        } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
            self.session.answer = None;
        }
    }

    /// The question's modal: what the build asks, its fields or the tool it
    /// wants to run, and how to answer it.
    pub(super) fn answer_dialog(&mut self, ui: &egui::Ui) {
        let Some(draft) = self.session.answer.as_ref() else {
            return;
        };
        let title = draft.question.title.clone();
        let issue = self
            .board
            .find(&draft.path)
            .map_or_else(|| file_name(&draft.path), |issue| issue.title.clone());
        let size = theme::ModalSize::M;
        let width = theme::modal_size(size, ui.ctx().content_rect()).x;
        let Some(draft) = self.session.answer.as_mut() else {
            return;
        };
        let mut choice = None;
        widgets::modal(ui, "answer-build", size, &title).show(ui.ctx(), |ui| {
            ui.set_width(width);
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            if widgets::modal_header(ui, &title, |ui| {
                let p = theme::palette(ui);
                ui.label(
                    RichText::new(format!("on {issue}"))
                        .font(theme::meta())
                        .color(p.ink_muted),
                );
            }) {
                choice = Some(Choice::Close);
            }
            widgets::rule(ui);
            Frame::new()
                .inner_margin(theme::SPACE_5)
                .show(ui, |ui| answer_body(ui, draft, width));
            widgets::rule(ui);
            choice = answer_foot(ui, &draft.question, choice);
        });
        match choice {
            Some(Choice::Close) => self.session.answer = None,
            Some(Choice::Send) => self.send_answer(None),
            Some(Choice::Permission(reply)) => self.send_answer(Some(reply)),
            None => {}
        }
    }
}

fn answer_body(ui: &mut egui::Ui, draft: &mut AnswerDraft, width: f32) {
    let p = theme::palette(ui);
    let AnswerDraft {
        question,
        values,
        error,
        ..
    } = draft;
    if let Some(error) = error {
        widgets::help(ui, error, p.danger);
        ui.add_space(theme::SPACE_3);
    }
    match &question.kind {
        QuestionKind::Form(fields) => {
            let max = widgets::modal_body_max(ui.ctx().content_rect());
            egui::ScrollArea::vertical().max_height(max).show(ui, |ui| {
                ui.set_width(width - 2.0 * theme::SPACE_5);
                ui.spacing_mut().item_spacing.y = theme::SPACE_3;
                for field in fields.iter().filter(|field| !field.hidden) {
                    let value = values
                        .entry(field.key.clone())
                        .or_insert_with(|| default_value(&field.kind));
                    field_ui(ui, field, value);
                }
            });
        }
        QuestionKind::Permission(permission) => permission_body(ui, permission),
    }
}

fn field_ui(ui: &mut egui::Ui, field: &Field, value: &mut AnswerValue) {
    let p = theme::palette(ui);
    widgets::field_label(ui, &field.title);
    if !field.description.is_empty() {
        widgets::help(ui, &field.description, p.ink_muted);
        ui.add_space(theme::SPACE_2);
    }
    match (&field.kind, value) {
        (
            FieldKind::Text {
                options,
                placeholder,
                custom,
                ..
            },
            AnswerValue::Text(text),
        ) => {
            if options.is_empty() {
                widgets::text_field(ui, text, placeholder);
            } else {
                choices(ui, options, text);
                if *custom {
                    ui.add_space(theme::SPACE_2);
                    widgets::text_field(ui, text, "Other answer");
                }
            }
        }
        (FieldKind::Number { integer, .. }, AnswerValue::Text(text)) => {
            let hint = if *integer { "42" } else { "0" };
            widgets::text_field(ui, text, hint);
        }
        (FieldKind::Boolean { .. }, AnswerValue::Boolean(checked)) => {
            widgets::checkbox(ui, checked, "Yes");
        }
        (FieldKind::Multi { options, .. }, AnswerValue::Multi(chosen)) => {
            multi(ui, options, chosen);
        }
        (FieldKind::External { url }, _) if !url.is_empty() => {
            ui.hyperlink_to("Open", url);
        }
        _ => {}
    }
}

fn choices(ui: &mut egui::Ui, options: &[crate::agent::service::Choice], text: &mut String) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(theme::SPACE_2);
        for option in options {
            if widgets::chip(ui, &option.label, text == &option.value)
                .on_hover_text(&option.description)
                .clicked()
            {
                *text = option.value.clone();
            }
        }
    });
}

fn multi(ui: &mut egui::Ui, options: &[crate::agent::service::Choice], chosen: &mut Vec<String>) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(theme::SPACE_2);
        for option in options {
            let current = chosen.contains(&option.value);
            if widgets::chip(ui, &option.label, current)
                .on_hover_text(&option.description)
                .clicked()
            {
                match chosen.iter().position(|value| value == &option.value) {
                    Some(at) => {
                        chosen.remove(at);
                    }
                    None => chosen.push(option.value.clone()),
                }
            }
        }
    });
}

fn permission_body(ui: &mut egui::Ui, permission: &Permission) {
    let p = theme::palette(ui);
    widgets::field_label(ui, "The agent wants to run");
    for resource in &permission.resources {
        ui.label(
            RichText::new(resource)
                .font(theme::meta_mono())
                .color(p.ink),
        );
    }
    if let Some(message) = &permission.message {
        ui.add_space(theme::SPACE_3);
        widgets::help(ui, message, p.ink_muted);
    }
    ui.add_space(theme::SPACE_3);
    widgets::help(
        ui,
        &format!(
            "{} is asking from its build. Allow once to run it, always to let this agent use it, or reject to stop the tool.",
            permission.action
        ),
        p.ink_muted,
    );
}

fn answer_foot(
    ui: &mut egui::Ui,
    question: &Question,
    mut choice: Option<Choice>,
) -> Option<Choice> {
    match &question.kind {
        QuestionKind::Form(_) => match widgets::modal_footer(ui, "", "Send") {
            widgets::FooterClick::Primary => choice = Some(Choice::Send),
            widgets::FooterClick::Cancel => choice = Some(Choice::Close),
            widgets::FooterClick::None => {}
        },
        QuestionKind::Permission(_) => {
            widgets::footer_bar(ui, theme::SPACE_5, |ui| {
                if widgets::button(ui, ButtonKind::Primary, None, "Allow once", Some("⌘↩"))
                    .clicked()
                {
                    choice = Some(Choice::Permission(PermissionReply::Once));
                }
                if widgets::button(ui, ButtonKind::Secondary, None, "Always allow", None).clicked()
                {
                    choice = Some(Choice::Permission(PermissionReply::Always));
                }
                if widgets::button(ui, ButtonKind::Delete, None, "Reject", None).clicked() {
                    choice = Some(Choice::Permission(PermissionReply::Reject));
                }
            });
        }
    }
    choice
}
