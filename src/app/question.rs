//! The form a build waits on: its fields on the card, answered in a modal
//! over the board. Submitting resumes the service's run.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use super::{
    Align, ButtonKind, CornerRadius, Duration, Frame, IboardApp, Icon, Instant, Job, Key, Layout,
    Path, PathBuf, Pos2, RichText, Sense, Stroke, StrokeKind, Vec2, file_name, job, theme, widgets,
};
use crate::agent::service::{
    Choice, Field, FieldKind, Permission, PermissionReply, Question, QuestionKind,
};

mod card;
mod fields;

/// One question open for an answer.
pub(super) struct AnswerDraft {
    pub(super) path: PathBuf,
    question: Question,
    values: BTreeMap<String, AnswerValue>,
    field_error: Option<FieldError>,
    failure: Option<String>,
    reply: Option<Job<Result<(), String>>>,
    focus: Option<Focus>,
}

#[derive(Clone, PartialEq, Eq)]
enum AnswerValue {
    Text {
        text: String,
        other: bool,
    },
    Boolean(Option<bool>),
    Multi {
        chosen: Vec<String>,
        other: bool,
        custom: String,
    },
}

#[derive(Clone, PartialEq, Eq)]
struct FieldError {
    key: String,
    message: String,
}

impl FieldError {
    fn new(field: &Field, message: String) -> Self {
        Self {
            key: field.key.clone(),
            message,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
enum Focus {
    Field(String),
}

impl AnswerDraft {
    fn new(path: PathBuf, question: Question) -> Self {
        let mut focus = None;
        let values = match &question.kind {
            QuestionKind::Form(fields) => {
                focus = fields
                    .iter()
                    .find(|field| !field.hidden)
                    .map(|field| Focus::Field(field.key.clone()));
                fields
                    .iter()
                    .filter(|field| !field.hidden)
                    .map(|field| (field.key.clone(), default_value(&field.kind)))
                    .collect()
            }
            QuestionKind::Permission(_) => BTreeMap::new(),
        };
        Self {
            path,
            question,
            values,
            field_error: None,
            failure: None,
            reply: None,
            focus,
        }
    }

    /// The form's answers, as the service reads them.
    fn form_answer(&self) -> Result<Map<String, Value>, FieldError> {
        let QuestionKind::Form(fields) = &self.question.kind else {
            return Ok(Map::new());
        };
        let mut answer = Map::new();
        for field in fields.iter().filter(|field| !field.hidden) {
            if let Some(value) = self.values.get(&field.key) {
                answer_field(field, value, &mut answer)?;
            }
        }
        Ok(answer)
    }
}

fn answer_field(
    field: &Field,
    value: &AnswerValue,
    answer: &mut Map<String, Value>,
) -> Result<(), FieldError> {
    match (&field.kind, value) {
        (
            FieldKind::Text {
                options, custom, ..
            },
            AnswerValue::Text { text, other },
        ) => {
            let text = text.trim();
            if text.is_empty() {
                if field.required {
                    return Err(missing(field));
                }
                return Ok(());
            }
            if !options.is_empty()
                && !*other
                && !*custom
                && !options.iter().any(|option| option.value == text)
            {
                return Err(FieldError::new(
                    field,
                    format!("Choose an option for {}.", field.title),
                ));
            }
            answer.insert(field.key.clone(), Value::String(text.to_string()));
        }
        (FieldKind::Number { integer, .. }, AnswerValue::Text { text, .. }) => {
            let text = text.trim();
            if text.is_empty() {
                if field.required {
                    return Err(missing(field));
                }
                return Ok(());
            }
            let value = number(text, *integer).map_err(|error| FieldError::new(field, error))?;
            answer.insert(field.key.clone(), value);
        }
        (FieldKind::Boolean { .. }, AnswerValue::Boolean(checked)) => match checked {
            Some(checked) => {
                answer.insert(field.key.clone(), Value::Bool(*checked));
            }
            None if field.required => return Err(missing(field)),
            None => {}
        },
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
            let mut values = chosen.clone();
            if *other {
                let text = text.trim();
                if !text.is_empty() {
                    values.push(text.to_string());
                }
            }
            if values.is_empty() {
                if field.required {
                    return Err(missing(field));
                }
                return Ok(());
            }
            if !*custom
                && let Some(unknown) = values
                    .iter()
                    .find(|value| !options.iter().any(|option| &option.value == *value))
            {
                return Err(FieldError::new(
                    field,
                    format!("Choose options for {}: {unknown}.", field.title),
                ));
            }
            let values = values.into_iter().map(Value::String).collect();
            answer.insert(field.key.clone(), Value::Array(values));
        }
        (FieldKind::External { .. }, _) => {}
        _ => {}
    }
    Ok(())
}

fn default_value(kind: &FieldKind) -> AnswerValue {
    match kind {
        FieldKind::Text {
            options,
            default,
            custom,
            ..
        } => {
            let other = *custom
                && !default.trim().is_empty()
                && !options.iter().any(|option| option.value == *default);
            AnswerValue::Text {
                text: default.clone(),
                other,
            }
        }
        FieldKind::Number { default, .. } => AnswerValue::Text {
            text: default.clone(),
            other: false,
        },
        FieldKind::Boolean { default } => AnswerValue::Boolean(default.then_some(true)),
        FieldKind::Multi {
            options,
            default,
            custom,
        } => {
            let mut chosen = Vec::new();
            let mut text = String::new();
            for value in default {
                if options.iter().any(|option| option.value == *value) {
                    chosen.push(value.clone());
                } else if *custom {
                    text = value.clone();
                }
            }
            AnswerValue::Multi {
                chosen,
                other: !text.is_empty(),
                custom: text,
            }
        }
        FieldKind::External { .. } => AnswerValue::Text {
            text: String::new(),
            other: false,
        },
    }
}

fn missing(field: &Field) -> FieldError {
    FieldError::new(field, format!("Answer {} first.", field.title))
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

/// What the question card's buttons asked for.
#[derive(Clone, Copy)]
enum AnswerAction {
    Send,
    Permission(PermissionReply),
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
    pub(super) fn question_panel(
        &mut self,
        ui: &mut egui::Ui,
        path: &Path,
        question: &Question,
        max_height: f32,
    ) {
        if self
            .session
            .answer
            .as_ref()
            .is_none_or(|draft| draft.question.id != question.id || draft.path != path)
        {
            self.session.answer = Some(AnswerDraft::new(path.to_path_buf(), question.clone()));
        }
        let since = self.building_since(path);
        let project = self.question_project(question);
        let Some(draft) = self.session.answer.as_mut() else {
            return;
        };
        let action = card::answer_card(ui, question, draft, max_height, since, project.as_deref());
        let pressed = ui
            .ctx()
            .input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::Enter));
        if self
            .session
            .answer
            .as_ref()
            .is_some_and(|draft| draft.reply.is_some())
        {
            return;
        }
        match action {
            Some(AnswerAction::Send) => self.send_answer(None),
            Some(AnswerAction::Permission(reply)) => self.send_answer(Some(reply)),
            None if pressed => {
                let reply = matches!(&question.kind, QuestionKind::Permission(_))
                    .then_some(PermissionReply::Once);
                self.send_answer(reply);
            }
            None => {}
        }
    }

    /// The question a stopped build left behind, for the quiet note in its
    /// place.
    pub(super) fn stale_question(&self, path: &Path) -> Option<&Question> {
        self.session
            .answer
            .as_ref()
            .filter(|draft| draft.path == path)
            .filter(|_| self.question_for(path).is_none())
            .map(|draft| &draft.question)
    }

    pub(super) fn dismiss_answer(&mut self, path: &Path) {
        let stale = self
            .session
            .answer
            .as_ref()
            .is_some_and(|draft| draft.path == path);
        if !stale {
            return;
        }
        if let Some(draft) = self.session.answer.take() {
            self.forget_question(&draft.question);
        }
    }

    /// A stopped build's question: a quiet note, its answers kept in case the
    /// build resumes.
    pub(super) fn stale_answer_card(
        &mut self,
        ui: &mut egui::Ui,
        path: &Path,
        question: &Question,
    ) {
        let p = theme::palette(ui);
        let width = ui.available_width();
        let mut dismiss = false;
        Frame::new()
            .fill(p.surface_sunken)
            .corner_radius(CornerRadius::same(theme::RADIUS_MD))
            .inner_margin(Vec2::splat(theme::SPACE_4))
            .show(ui, |ui| {
                ui.set_width(width - 2.0 * theme::SPACE_4);
                ui.add(
                    egui::Label::new(
                        RichText::new("This build stopped before it was answered.")
                            .font(theme::ui_strong())
                            .color(p.ink),
                    )
                    .wrap(),
                );
                ui.add_space(theme::SPACE_2);
                ui.add(
                    egui::Label::new(
                        RichText::new(&question.title)
                            .font(theme::ui_small())
                            .color(p.ink_muted),
                    )
                    .wrap(),
                );
                ui.add_space(theme::SPACE_3);
                dismiss = widgets::button(ui, ButtonKind::Ghost, None, "Dismiss", None).clicked();
            });
        if dismiss {
            self.dismiss_answer(path);
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
                    if let Some(draft) = self.session.answer.as_mut() {
                        draft.failure = None;
                        draft.focus = Some(Focus::Field(error.key.clone()));
                        draft.field_error = Some(error);
                    }
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
            draft.field_error = None;
            draft.failure = None;
            draft.reply = Some(result);
        }
    }

    fn fail_answer(&mut self, error: String) {
        if let Some(draft) = self.session.answer.as_mut() {
            draft.field_error = None;
            draft.failure = Some(error);
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
                        draft.field_error = None;
                        draft.failure = Some(error);
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

    #[cfg(test)]
    pub(super) fn fail_answer_for_test(&mut self, error: &str) {
        if let Some(draft) = self.session.answer.as_mut() {
            draft.reply = Some(Job::done(Err(error.to_string())));
        }
    }

    #[cfg(test)]
    pub(super) fn form_answer_for_test(&self) -> Result<Map<String, Value>, String> {
        let Some(draft) = self.session.answer.as_ref() else {
            return Err("no answer".to_string());
        };
        draft.form_answer().map_err(|error| error.message)
    }
}

fn question_heading(question: &Question) -> String {
    match &question.kind {
        QuestionKind::Permission(permission) => {
            let action = permission.action.trim();
            let action = if action.is_empty() {
                "a command"
            } else {
                action
            };
            format!("Allow {action}?")
        }
        QuestionKind::Form(fields) => {
            if !question.title.trim().is_empty() {
                return question.title.trim().to_string();
            }
            let visible: Vec<&Field> = fields.iter().filter(|field| !field.hidden).collect();
            if visible.len() == 1 && !visible[0].title.trim().is_empty() {
                return visible[0].title.trim().to_string();
            }
            "Question".to_string()
        }
    }
}
