//! The form a build waits on: its fields on the card, answered in a modal
//! over the board. Submitting resumes the service's run.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use super::{
    ButtonKind, CornerRadius, Duration, Frame, IboardApp, Instant, Job, Key, Path, PathBuf,
    RichText, Stroke, Vec2, job, theme, widgets,
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
    /// Opens the ticket whose build waits on an answer. The question sits
    /// above the agent log, in the ticket, not in a dialog over it.
    pub(super) fn open_answer(&mut self, path: PathBuf) {
        self.open_issue(path.clone());
        if let Some(question) = self.question_for(&path) {
            self.session.answer = Some(AnswerDraft::new(path, question));
        }
    }

    /// The question the build on `path` waits on, drawn above the log it
    /// belongs to. Answering it resumes the run; it never leaves the ticket.
    pub(super) fn question_panel(&mut self, ui: &mut egui::Ui, path: &Path, question: &Question) {
        if self
            .session
            .answer
            .as_ref()
            .is_none_or(|draft| draft.question.id != question.id || draft.path != path)
        {
            self.session.answer = Some(AnswerDraft::new(path.to_path_buf(), question.clone()));
        }
        let Some(draft) = self.session.answer.as_mut() else {
            return;
        };
        let p = theme::palette(ui);
        let width = ui.available_width();
        let mut send = false;
        let mut permission = None;
        Frame::new()
            .fill(p.surface_sunken)
            .stroke(Stroke::new(1.0, p.status_doing.gamma_multiply(0.6)))
            .corner_radius(CornerRadius::same(theme::RADIUS_SM))
            .inner_margin(Vec2::new(theme::SPACE_3, theme::SPACE_3))
            .show(ui, |ui| {
                ui.set_width(width - 2.0 * theme::SPACE_3);
                ui.label(
                    RichText::new("Question")
                        .font(theme::meta())
                        .color(p.status_doing)
                        .strong(),
                );
                ui.add(
                    egui::Label::new(
                        RichText::new(&question.title)
                            .font(theme::ui_strong())
                            .color(p.ink),
                    )
                    .wrap(),
                );
                ui.add_space(theme::SPACE_3);
                answer_body(ui, draft, 260.0);
                ui.add_space(theme::SPACE_3);
                match &question.kind {
                    QuestionKind::Form(_) => {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if widgets::button(ui, ButtonKind::Primary, None, "Send", Some("⌘↩"))
                                .clicked()
                            {
                                send = true;
                            }
                        });
                    }
                    QuestionKind::Permission(_) => {
                        widgets::footer_bar(ui, 0.0, |ui| {
                            if widgets::button(
                                ui,
                                ButtonKind::Primary,
                                None,
                                "Allow once",
                                Some("⌘↩"),
                            )
                            .clicked()
                            {
                                permission = Some(PermissionReply::Once);
                            }
                            if widgets::button(
                                ui,
                                ButtonKind::Secondary,
                                None,
                                "Always allow",
                                None,
                            )
                            .clicked()
                            {
                                permission = Some(PermissionReply::Always);
                            }
                            if widgets::button(ui, ButtonKind::Delete, None, "Reject", None)
                                .clicked()
                            {
                                permission = Some(PermissionReply::Reject);
                            }
                        });
                    }
                }
            });
        if send {
            self.send_answer(None);
        } else if let Some(reply) = permission {
            self.send_answer(Some(reply));
        } else if ui
            .ctx()
            .input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::Enter))
        {
            let reply = matches!(&question.kind, QuestionKind::Permission(_))
                .then_some(PermissionReply::Once);
            self.send_answer(reply);
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
}

fn answer_body(ui: &mut egui::Ui, draft: &mut AnswerDraft, max_height: f32) {
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
            egui::ScrollArea::vertical()
                .max_height(max_height)
                .show(ui, |ui| {
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
