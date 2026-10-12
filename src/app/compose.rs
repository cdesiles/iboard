//! The quick-add composer: typing a new issue, naming it, and writing it.

use super::*;

impl IboardApp {
    /// Opens the quick-add composer at the top of Todo, from any screen.
    pub(super) fn open_composer(&mut self) {
        if self.board.todo_column().is_none() {
            return;
        }
        if matches!(self.session.screen, Screen::Board) {
            self.show_composer();
        } else {
            self.leave(Leave::Composer);
        }
    }

    pub(super) fn show_composer(&mut self) {
        // New issues belong to the board's Todo column, so leave ColumnFocus.
        self.column_focus.column = None;
        // Checked on every open: Apple Intelligence or an agent may have been
        // set up meanwhile.
        let engine = Some(self.title_engine).filter(|engine| {
            *engine != Engine::Manual && titles::availability(*engine, &self.agent).is_ready()
        });
        if let Some(engine) = engine {
            titles::prewarm(engine);
        }
        let composer = self.session.composer.get_or_insert_with(Composer::default);
        composer.engine = engine;
        composer.focus = Some(if composer.stage == TitleStage::Idle {
            ComposerField::Description
        } else {
            ComposerField::Title
        });
    }

    /// Writes `# <title>`, then the description, to a new file in Todo named
    /// after the title. Returns the new file's path; the caller decides
    /// whether to open it.
    pub(super) fn create_issue(
        &mut self,
        dir: &Path,
        title: &str,
        description: &str,
    ) -> Result<PathBuf, String> {
        let title = title.trim();
        let (name, _) = crate::issue::new_file_name(dir, title);
        let path = dir.join(&name);
        let body = description.trim_end();
        let content = if body.trim().is_empty() {
            format!("# {title}\n")
        } else {
            format!("# {title}\n\n{body}\n")
        };
        match atomic_write(&path, &content) {
            Ok(()) => {
                self.refresh();
                Ok(path)
            }
            Err(err) => Err(format!(
                "Couldn't create {name} in {}: {err}",
                self.relative(dir)
            )),
        }
    }

    /// `↩` or `⌘S` in the composer. With no title yet and an engine ready,
    /// it names the issue first and stops on the suggestion; otherwise it
    /// writes the issue and clears the composer for the next one. A click
    /// elsewhere (`close`) never stops: it names if needed, writes, and
    /// closes the composer.
    pub(super) fn submit_composer(&mut self, close: bool) {
        let Some(composer) = self.session.composer.as_mut() else {
            return;
        };
        if composer.stage == TitleStage::Naming {
            if close && let Some(run) = composer.naming.as_mut() {
                run.then_close = true;
            }
            return;
        }
        let typed = composer.title.trim().to_string();
        let description = composer.description.clone();
        if typed.is_empty() && description.trim().is_empty() {
            return;
        }
        let title = if !typed.is_empty() {
            typed
        } else if let (Some(engine), TitleStage::Idle | TitleStage::Inferred) =
            (composer.engine, composer.stage)
        {
            composer.naming = Some(NamingRun {
                answer: titles::start(engine, &self.agent, &description),
                then_close: close,
            });
            composer.stage = TitleStage::Naming;
            composer.error = None;
            return;
        } else {
            titles::first_line_title(&description)
        };
        self.write_composed(&title, &description, close);
    }

    /// Writes the composed issue to Todo; on success the composer clears (or
    /// closes), on failure it keeps everything and says why.
    pub(super) fn write_composed(&mut self, title: &str, description: &str, close: bool) {
        let Some(dir) = self.board.todo_column().map(|c| c.dir.clone()) else {
            return;
        };
        match self.create_issue(&dir, title, description) {
            Ok(path) => {
                // A new ticket lands at position 1, right after the composer.
                // The renumbering is deferred to the end of the frame so a
                // click that added the ticket still reaches the card under it.
                if self.board.find(&path).is_some() {
                    self.session.pending_front = Some((dir.clone(), path.clone()));
                }
                if close {
                    self.session.composer = None;
                } else if let Some(composer) = self.session.composer.as_mut() {
                    *composer = Composer {
                        engine: composer.engine,
                        focus: Some(ComposerField::Description),
                        ..Composer::default()
                    };
                }
                self.session.selected = Some(path.clone());
                self.session.new_card = Some((path, Instant::now()));
            }
            Err(err) => {
                if let Some(composer) = self.session.composer.as_mut() {
                    composer.error = Some(err);
                    composer.focus = Some(ComposerField::Description);
                }
            }
        }
    }

    /// Adopts the engine's title when it lands, or gives up after
    /// [`NAMING_BUDGET`] and lets the user type it.
    pub(super) fn poll_naming(&mut self, ctx: &egui::Context) {
        let Some(run) = self
            .session
            .composer
            .as_ref()
            .and_then(|c| c.naming.as_ref())
        else {
            return;
        };
        let result = match run.answer.answer() {
            Some(result) => result,
            None if run.answer.elapsed() < NAMING_BUDGET => {
                ctx.request_repaint_after(job::POLL_EVERY);
                return;
            }
            None => Err("the engine took too long".to_string()),
        };
        let Some(composer) = self.session.composer.as_mut() else {
            return;
        };
        let then_close = composer.naming.take().is_some_and(|run| run.then_close);
        let description = composer.description.clone();
        match result {
            Ok(title) if then_close => self.write_composed(&title, &description, true),
            Ok(title) => {
                composer.title = title;
                composer.stage = TitleStage::Inferred;
                composer.select_title = true;
                composer.focus = Some(ComposerField::Title);
            }
            // A click elsewhere never stops: the first line names it.
            Err(_) if then_close => {
                let title = titles::first_line_title(&description);
                self.write_composed(&title, &description, true);
            }
            Err(_) => {
                composer.stage = TitleStage::Failed;
                composer.focus = Some(ComposerField::Title);
            }
        }
    }

    /// `esc` in the composer: while naming, stop and type the title instead;
    /// otherwise close it.
    pub(super) fn composer_escape(&mut self) {
        match self.session.composer.as_mut() {
            Some(composer) if composer.stage == TitleStage::Naming => {
                composer.naming = None;
                composer.stage = TitleStage::Skipped;
                composer.focus = Some(ComposerField::Title);
            }
            _ => self.session.composer = None,
        }
    }
}

/// The quick-add composer: a card framed in `accent` with a reserved title
/// row, a description that grows from 3 to 8 lines, and the file it will
/// write underneath. See `design/components/QuickAdd`.
pub(super) fn composer_ui(
    ui: &mut egui::Ui,
    composer: &mut Composer,
    column: &Column,
    folder: &str,
    intents: &mut Vec<Intent>,
) {
    let p = theme::palette(ui);
    composer_enter(ui, composer, intents);
    let frame = Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(2.0, p.accent))
        .corner_radius(CornerRadius::same(theme::RADIUS_MD))
        .inner_margin(Margin::symmetric(
            theme::SPACE_4 as i8,
            theme::SPACE_3 as i8,
        ))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = theme::SPACE_2;
            title_row(ui, composer, intents);
            description_field(ui, composer, intents);
            composer_meta(ui, composer, column, folder);
        });

    // A click anywhere else on the board (another card, blank space) adds
    // what was typed and closes the composer.
    let pressed = ui.input(|i| {
        i.pointer
            .press_origin()
            .filter(|_| i.pointer.primary_pressed())
    });
    let clicked_away = pressed.is_some_and(|pos| {
        !frame.response.rect.contains(pos) && ui.ctx().layer_id_at(pos) == Some(ui.layer_id())
    });
    if clicked_away {
        intents.push(if composer.is_empty() {
            Intent::CloseComposer
        } else {
            Intent::SubmitComposer { close: true }
        });
    }
}

const TITLE_ID: &str = "composer";
const DESCRIPTION_ID: &str = "composer-description";

/// `↩` adds, from either field. `⇧↩` and `⌘↩` are a new line: the
/// description's own return key (as `⇧↩`), or a step down from the title.
/// Taken before the fields see the key.
fn composer_enter(ui: &mut egui::Ui, composer: &mut Composer, intents: &mut Vec<Intent>) {
    let in_title = ui.memory(|m| m.has_focus(egui::Id::new(TITLE_ID)));
    let in_description = ui.memory(|m| m.has_focus(egui::Id::new(DESCRIPTION_ID)));
    if !in_title && !in_description {
        return;
    }
    let (mut add, mut new_line) = (false, false);
    ui.input_mut(|i| {
        i.events.retain_mut(|event| match event {
            egui::Event::Key {
                key: Key::Enter,
                pressed: true,
                modifiers,
                ..
            } if modifiers.shift || modifiers.command => {
                new_line = true;
                *modifiers = egui::Modifiers::SHIFT;
                in_description
            }
            egui::Event::Key {
                key: Key::Enter,
                pressed: true,
                ..
            } => {
                add = true;
                false
            }
            _ => true,
        })
    });
    let naming = composer.stage == TitleStage::Naming;
    if !naming && add {
        intents.push(Intent::SubmitComposer { close: false });
    }
    if in_title && new_line && !naming {
        composer.focus = Some(ComposerField::Description);
    }
}

/// The title row, always 22pt so no state makes the card jump: the title
/// (or "Naming…"), the character count near the limit, and the engine tag,
/// over a rule that says how the title will be named.
fn title_row(ui: &mut egui::Ui, composer: &mut Composer, intents: &mut Vec<Intent>) {
    let p = theme::palette(ui);
    let width = ui.available_width();
    let (row, _) = ui.allocate_exact_size(Vec2::new(width, 22.0), Sense::hover());
    let rule_y = row.max.y - 0.5;
    let rule = [Pos2::new(row.min.x, rule_y), Pos2::new(row.max.x, rule_y)];
    // A title typed by hand is Manual, whatever the engine: `↩` won't infer
    // one.
    let manual = composer.stage != TitleStage::Inferred && !composer.title.trim().is_empty();
    let engine = if manual { None } else { composer.engine };
    match (composer.stage, engine) {
        (TitleStage::Failed | TitleStage::Skipped, _) => {
            ui.painter()
                .line_segment(rule, Stroke::new(1.0, p.status_doing));
        }
        (_, None) => {
            ui.painter().extend(Shape::dashed_line(
                &rule,
                Stroke::new(1.0, p.line_strong),
                4.0,
                3.0,
            ));
        }
        _ => {
            ui.painter().line_segment(rule, Stroke::new(1.0, p.line));
        }
    }
    let mut row_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(row.with_max_y(row.max.y - 4.0))
            .layout(Layout::right_to_left(Align::Center)),
    );
    row_ui.spacing_mut().item_spacing.x = theme::SPACE_3;
    let (tag, inferred) = match (composer.stage, engine) {
        (TitleStage::Inferred, _) => (Some("Inferred"), true),
        (TitleStage::Failed | TitleStage::Skipped, _) => (None, false),
        (_, Some(engine)) => (Some(engine.tag()), false),
        (_, None) => (Some(Engine::Manual.tag()), false),
    };
    if let Some(tag) = tag {
        let response = widgets::tag(&mut row_ui, tag, inferred);
        if !inferred
            && response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text("Title inference (⌘, › Titles)")
                .clicked()
        {
            intents.push(Intent::TitleSettings);
        }
    }
    let chars = composer.title.chars().count();
    if chars >= titles::MAX_TITLE_CHARS - 10 {
        row_ui.label(
            RichText::new(format!("{chars}/{}", titles::MAX_TITLE_CHARS))
                .font(theme::meta())
                .color(p.ink_muted),
        );
    }
    if composer.stage == TitleStage::Naming {
        row_ui.with_layout(Layout::left_to_right(Align::Center), naming_dot);
    } else {
        title_field(&mut row_ui, composer);
    }
}

/// "Naming…" with its pulsing `status-doing` dot, steady with reduced motion.
fn naming_dot(ui: &mut egui::Ui) {
    let p = theme::palette(ui);
    ui.spacing_mut().item_spacing.x = 6.0;
    let (dot, _) = ui.allocate_exact_size(Vec2::splat(6.0), Sense::hover());
    let pulse = if theme::reduced_motion(ui.ctx()) {
        1.0
    } else {
        (ui.input(|i| i.time) * std::f64::consts::TAU).sin() as f32
    };
    ui.painter().circle_filled(
        dot.center(),
        3.0,
        p.status_doing.gamma_multiply(0.65 + 0.35 * pulse),
    );
    ui.label(
        RichText::new("Naming…")
            .font(theme::ui_body())
            .color(p.ink_muted),
    );
    ui.ctx().request_repaint();
}

/// The title's text field; focused on request, with the whole suggestion
/// selected so typing replaces it.
fn title_field(row: &mut egui::Ui, composer: &mut Composer) {
    let p = theme::palette(row);
    let title_id = egui::Id::new(TITLE_ID);
    let hint = match (composer.stage, composer.engine) {
        (TitleStage::Failed | TitleStage::Skipped, _) => "Type a title",
        (_, None) => "Add a title",
        _ => "Title is named from the description",
    };
    let width = row.available_width();
    let mut output = egui::TextEdit::singleline(&mut composer.title)
        .id(title_id)
        .font(theme::ui_strong())
        .hint_text(
            RichText::new(hint)
                .font(theme::ui_body())
                .color(p.ink_faint),
        )
        .char_limit(titles::MAX_TITLE_CHARS)
        .frame(Frame::NONE)
        .margin(Margin::ZERO)
        .desired_width(width)
        .show(row);
    if composer.focus == Some(ComposerField::Title) {
        composer.focus = None;
        output.response.request_focus();
        if std::mem::take(&mut composer.select_title) {
            let end = CCursor::new(composer.title.chars().count());
            output
                .state
                .cursor
                .set_char_range(Some(CCursorRange::two(CCursor::new(0), end)));
            output.state.store(row.ctx(), title_id);
        }
    }
    // Editing a suggestion makes it the user's own title.
    if output.response.changed() && composer.stage == TitleStage::Inferred {
        composer.stage = TitleStage::Idle;
    }
}

/// The description: 3 lines to start, 8 at most, then it scrolls with its
/// last line fading out. Leaving an empty composer closes it.
fn description_field(ui: &mut egui::Ui, composer: &mut Composer, intents: &mut Vec<Intent>) {
    let p = theme::palette(ui);
    let naming = composer.stage == TitleStage::Naming;
    let line = 18.0;
    let scroll = egui::ScrollArea::vertical()
        .id_salt("composer-description-scroll")
        .max_height(8.0 * line)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.add_enabled(
                !naming,
                egui::TextEdit::multiline(&mut composer.description)
                    .id(egui::Id::new(DESCRIPTION_ID))
                    .font(theme::ui_body())
                    .text_color(if naming { p.ink_muted } else { p.ink })
                    .hint_text(RichText::new("Describe the issue").color(p.ink_faint))
                    .frame(Frame::NONE)
                    .margin(Margin::ZERO)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY)
                    .return_key(Some(egui::KeyboardShortcut::new(
                        egui::Modifiers::SHIFT,
                        Key::Enter,
                    ))),
            )
        });
    let capped = scroll.content_size.y > scroll.inner_rect.height() + 1.0;
    if capped && scroll.state.offset.y + scroll.inner_rect.height() < scroll.content_size.y - 1.0 {
        fade_out(ui, scroll.inner_rect, p.surface);
    }
    let description = scroll.inner;
    if composer.focus == Some(ComposerField::Description) {
        composer.focus = None;
        description.request_focus();
    }
    // Clicking elsewhere closes an empty composer; one with text stays.
    let empty = composer.title.trim().is_empty() && composer.description.trim().is_empty();
    let still_focused = ui.memory(|m| {
        m.has_focus(egui::Id::new(TITLE_ID)) || m.has_focus(egui::Id::new(DESCRIPTION_ID))
    });
    if empty && description.lost_focus() && !still_focused && composer.focus.is_none() {
        intents.push(Intent::CloseComposer);
    }
}

/// The composer's last line: the file it will write (or why it can't name
/// it yet) on the left, the keys that apply on the right.
pub(super) fn composer_meta(ui: &mut egui::Ui, composer: &Composer, column: &Column, folder: &str) {
    let p = theme::palette(ui);
    let meta = |text: &str, color: Color32| RichText::new(text).font(theme::meta()).color(color);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        if let Some(error) = &composer.error {
            ui.add(egui::Label::new(meta(error, p.danger)).truncate());
            return;
        }
        let title = composer.title.trim();
        let described = !composer.description.trim().is_empty();
        let (hints, left): (&[(&str, &str)], Vec<RichText>) = match composer.stage {
            TitleStage::Naming => (
                &[("esc", "type it")],
                vec![meta(folder, p.ink_muted), meta("naming", p.ink_faint)],
            ),
            TitleStage::Failed if title.is_empty() => (
                &[("↩", "add")],
                vec![meta(
                    "Couldn't name it · ↩ uses the first line",
                    p.status_doing,
                )],
            ),
            _ if !title.is_empty() => {
                let (name, taken) = crate::issue::new_file_name(&column.dir, title);
                let color = if taken {
                    p.status_doing
                } else if composer.stage == TitleStage::Inferred {
                    p.status_done
                } else {
                    p.ink_muted
                };
                (
                    &[("↩", "add"), ("⇧↩", "new line")],
                    vec![meta(&format!("{folder}{name}"), color)],
                )
            }
            TitleStage::Skipped | TitleStage::Failed => (
                &[("↩", "add")],
                vec![
                    meta(folder, p.ink_muted),
                    meta("empty title uses the first line", p.ink_faint),
                ],
            ),
            _ if composer.engine.is_none() => (
                &[("↩", "add")],
                vec![
                    meta(folder, p.ink_muted),
                    meta("empty title uses the first line", p.ink_faint),
                ],
            ),
            _ => (
                if described {
                    &[("↩", "add"), ("⇧↩", "new line")]
                } else {
                    &[("↩", "add"), ("esc", "close")]
                },
                vec![
                    meta(folder, p.ink_muted),
                    meta("named on save", p.ink_faint),
                ],
            ),
        };
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = theme::SPACE_3;
            for (key, action) in hints.iter().rev() {
                ui.horizontal(|ui| widgets::hint(ui, key, action));
            }
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                for part in left {
                    ui.add(egui::Label::new(part).truncate());
                }
            });
        });
    });
}

/// Fades the bottom 22pt of `rect` into `ground`, to show more text below.
pub(super) fn fade_out(ui: &egui::Ui, rect: Rect, ground: Color32) {
    let band = Rect::from_min_max(Pos2::new(rect.min.x, rect.max.y - 22.0), rect.max);
    let mut mesh = egui::Mesh::default();
    let clear = Color32::from_rgba_unmultiplied(ground.r(), ground.g(), ground.b(), 0);
    mesh.colored_vertex(band.left_top(), clear);
    mesh.colored_vertex(band.right_top(), clear);
    mesh.colored_vertex(band.right_bottom(), ground);
    mesh.colored_vertex(band.left_bottom(), ground);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    ui.painter().add(Shape::mesh(mesh));
}
