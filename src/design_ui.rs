//! The Design panel: browse the project's design system and scaffold a
//! component.
//!
//! Opened from the board header, or as `idesign`'s whole window. It is laid out
//! like Claude Design: a category sidebar — Overview, Colors, Typography,
//! Spacing, Corner radius, Shadows, Size, Components, Assets, Files — and a
//! pane for the selected category. Components drills into a second level: the
//! rail slides sideways to a searchable component list and the pane shows the
//! chosen component. It reads the system the shared `design` module loaded and
//! renders component previews through the out-of-process helper, so the board
//! itself never loads WebKit: startup stays instant and a broken preview cannot
//! take the window down. Preview only loads the one component in view.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use egui::{Align2, Color32, CornerRadius, Rect, Stroke, StrokeKind, pos2};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

use crate::design::{self, Component, DesignSystem, Token};
use crate::job::{self, Job};

use crate::theme;
use crate::widgets::{self, ButtonKind};

/// Width of the browser's category sidebar.
const SIDEBAR_W: f32 = 200.0;

/// How long the rail slides between its two levels, in seconds.
const RAIL_SLIDE_SECS: f32 = 0.22;

/// What the panel asks the app to do after a frame.
pub enum Action {
    Keep,
    Close,
    /// Scaffold a new component (the deterministic part is the library's).
    Create {
        name: String,
        group: String,
        intent: String,
        /// Ask the configured agent to draft the preview fragment too.
        draft: bool,
    },
    /// Render a component's preview to a PNG, lazily, when its card is in view.
    Render(String),
    /// Write the catalog page and show it in a live webview.
    OpenCatalog,
}

/// One section of the browser, mirroring Claude Design's sidebar.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Category {
    Overview,
    Colors,
    Typography,
    Spacing,
    Radius,
    Shadows,
    Size,
    Components,
    Assets,
    Files,
}

impl Category {
    /// Every category, in sidebar order.
    const ALL: [Category; 10] = [
        Category::Overview,
        Category::Colors,
        Category::Typography,
        Category::Spacing,
        Category::Radius,
        Category::Shadows,
        Category::Size,
        Category::Components,
        Category::Assets,
        Category::Files,
    ];

    fn label(self) -> &'static str {
        match self {
            Category::Overview => "Overview",
            Category::Colors => "Colors",
            Category::Typography => "Typography",
            Category::Spacing => "Spacing",
            Category::Radius => "Corner radius",
            Category::Shadows => "Shadows",
            Category::Size => "Size",
            Category::Components => "Components",
            Category::Assets => "Assets",
            Category::Files => "Files",
        }
    }
}

/// A search hit: the category to open and, for an entry, what to select.
struct Hit {
    category: Category,
    label: String,
    detail: String,
}

/// The state of one component's preview.
enum Preview {
    /// Requested, waiting for the renderer.
    Pending,
    Ready(egui::TextureHandle),
    Failed(String),
}

/// The Design panel's session state.
pub struct Panel {
    category: Category,
    components_open: bool,
    selected: Option<String>,
    search: String,
    form_open: bool,
    name: String,
    group: String,
    intent: String,
    /// The "Draft with the agent" choice in the new-component form.
    draft: bool,
    /// The last outcome ("Created …"), shown under the toolbar.
    pub status: Option<String>,
    /// Previews already asked for, by component name. Loading is lazy: a card
    /// only appears here once it has scrolled into view.
    previews: BTreeMap<String, Preview>,
    /// The one render in flight, and the component it belongs to.
    running: Option<(String, Job<Result<PathBuf, String>>)>,
    /// Layout cache for the Overview brief's markdown.
    brief_cache: CommonMarkCache,
}

impl Default for Panel {
    fn default() -> Self {
        Self {
            category: Category::Overview,
            components_open: false,
            selected: None,
            search: String::new(),
            form_open: false,
            name: String::new(),
            group: "Components".to_string(),
            intent: String::new(),
            draft: false,
            status: None,
            previews: BTreeMap::new(),
            running: None,
            brief_cache: CommonMarkCache::default(),
        }
    }
}

impl Panel {
    /// Adopt a render the host started for `name`.
    pub fn start_render(&mut self, name: String, job: Job<Result<PathBuf, String>>) {
        self.running = Some((name, job));
    }

    /// Record that a render could not even be started.
    pub fn fail_render(&mut self, name: &str, error: String) {
        self.previews
            .insert(name.to_string(), Preview::Failed(error));
    }

    /// Drops what the panel knows of `name`'s preview, so it renders again:
    /// its `preview.html` changed.
    pub fn forget_preview(&mut self, name: &str) {
        self.previews.remove(name);
        if self
            .running
            .as_ref()
            .is_some_and(|(running, _)| running == name)
        {
            self.running = None;
        }
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        system: Option<&DesignSystem>,
        error: Option<&str>,
        render_available: bool,
        agent_ready: bool,
    ) -> Action {
        let mut action = Action::Keep;
        egui::Window::new("Design system")
            .collapsible(false)
            .resizable(true)
            .default_width(980.0)
            .default_height(640.0)
            .show(ctx, |ui| {
                action = self.body(ui, system, error, render_available, agent_ready);
            });
        action
    }

    /// The panel's content, without window chrome, so a host can frame it either
    /// way: the board shows it in a window, `idesign` fills its whole app.
    pub fn body(
        &mut self,
        ui: &mut egui::Ui,
        system: Option<&DesignSystem>,
        error: Option<&str>,
        render_available: bool,
        agent_ready: bool,
    ) -> Action {
        self.poll(ui.ctx());
        let mut action = Action::Keep;
        let p = theme::palette(ui);
        ui.horizontal(|ui| {
            let title = system
                .map(|system| system.meta.title.clone())
                .unwrap_or_else(|| "No design system".to_string());
            ui.label(egui::RichText::new(title).color(p.ink).strong());
            if let Some(system) = system {
                ui.label(
                    egui::RichText::new(format!("· {}", system.meta.namespace)).color(p.ink_muted),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::button(ui, ButtonKind::Secondary, None, "Close", None).clicked() {
                    action = Action::Close;
                }
                if system.is_some()
                    && widgets::button(
                        ui,
                        ButtonKind::Secondary,
                        Some(theme::Icon::Expand),
                        "Live catalog",
                        None,
                    )
                    .clicked()
                {
                    action = Action::OpenCatalog;
                }
            });
        });

        let Some(system) = system else {
            ui.separator();
            ui.label(
                egui::RichText::new(error.unwrap_or("No design system in this project."))
                    .color(p.ink_muted),
            );
            ui.label(
                egui::RichText::new(
                    "Add design/design-system.json, or `idesign import <package>`.",
                )
                .color(p.ink_faint)
                .size(11.0),
            );
            return action;
        };

        ui.separator();
        // A horizontal row: the sidebar, then a *vertical* pane. The explicit
        // vertical layout matters — a scroll area inherits its parent's
        // direction, so without it the whole content pane would lay out in a
        // line.
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(SIDEBAR_W, ui.available_height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| self.sidebar(ui, system),
            );
            ui.separator();
            ui.vertical(|ui| {
                egui::ScrollArea::vertical()
                    .id_salt("design-content")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        self.content(ui, system, render_available, agent_ready, &mut action);
                    });
            });
        });
        action
    }

    /// The left rail: the fixed search bar over two levels that slide past each
    /// other — the categories, and the Components list.
    fn sidebar(&mut self, ui: &mut egui::Ui, system: &DesignSystem) {
        ui.add_space(theme::SPACE_2);
        ui.add(
            egui::TextEdit::singleline(&mut self.search)
                .hint_text("Search")
                .desired_width(f32::INFINITY),
        );
        ui.add_space(theme::SPACE_3);

        // 0 = categories, 1 = the component list; the pages stay a width apart.
        let target = if self.components_open { 1.0 } else { 0.0 };
        let progress =
            ui.ctx()
                .animate_value_with_time(ui.id().with("rail-slide"), target, RAIL_SLIDE_SECS);
        let slide = egui::emath::ease_in_ease_out(progress);

        let area = ui.available_rect_before_wrap();
        let width = area.width();
        ui.allocate_rect(area, egui::Sense::hover());
        let page = |ui: &mut egui::Ui, id: &'static str, offset: f32| {
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(area.translate(egui::vec2(offset, 0.0)))
                    .layout(egui::Layout::top_down(egui::Align::Min))
                    .id_salt(id),
            );
            child.set_clip_rect(area);
            child
        };

        // Only the open level reacts, so just one list scrolls or is clicked.
        let mut main = page(ui, "rail-main", -slide * width);
        if self.components_open {
            main.disable();
        }
        egui::ScrollArea::vertical()
            .id_salt("rail-main-scroll")
            .auto_shrink([false, false])
            .show(&mut main, |ui| self.categories(ui, system));

        let mut components = page(ui, "rail-components", (1.0 - slide) * width);
        if !self.components_open {
            components.disable();
        }
        if back_row(&mut components, "Components").clicked() {
            self.components_open = false;
        }
        components.add_space(theme::SPACE_2);
        egui::ScrollArea::vertical()
            .id_salt("rail-components-scroll")
            .auto_shrink([false, false])
            .show(&mut components, |ui| self.component_rows(ui, system));
    }

    fn categories(&mut self, ui: &mut egui::Ui, system: &DesignSystem) {
        let p = theme::palette(ui);
        let query = self.search.trim().to_lowercase();
        if !query.is_empty() {
            let hits = search(system, &query);
            if hits.is_empty() {
                ui.label(
                    egui::RichText::new("No matches")
                        .color(p.ink_faint)
                        .size(11.0),
                );
            }
            for hit in hits {
                if nav_row(ui, &hit.label, Some(&hit.detail), false).clicked() {
                    if hit.category == Category::Components {
                        self.open_components(system, Some(hit.label));
                    } else {
                        self.category = hit.category;
                    }
                    self.search.clear();
                }
            }
            return;
        }

        for category in Category::ALL {
            let meta = match category {
                Category::Files => Some(system.files().len().to_string()),
                Category::Components => Some(system.components.len().to_string()),
                _ => None,
            };
            if nav_row(
                ui,
                category.label(),
                meta.as_deref(),
                self.category == category,
            )
            .clicked()
            {
                if category == Category::Components {
                    self.open_components(system, None);
                } else {
                    self.category = category;
                }
            }
        }
    }

    fn component_rows(&mut self, ui: &mut egui::Ui, system: &DesignSystem) {
        let p = theme::palette(ui);
        let list = ordered_components(system, self.search.trim());

        if list.is_empty() {
            ui.label(
                egui::RichText::new(if system.components.is_empty() {
                    "No components yet"
                } else {
                    "No matches"
                })
                .color(p.ink_faint)
                .size(11.0),
            );
            return;
        }
        for component in list {
            let selected = self.selected.as_deref() == Some(component.name.as_str());
            if nav_row(ui, &component.name, None, selected).clicked() {
                self.selected = Some(component.name.clone());
            }
        }
    }

    /// Open the Components level, choosing: the named component when it exists,
    /// else the one already selected, else the first.
    fn open_components(&mut self, system: &DesignSystem, name: Option<String>) {
        self.category = Category::Components;
        self.components_open = true;
        let exists = |name: &str| system.components.iter().any(|c| c.name == name);
        if let Some(name) = name
            && exists(&name)
        {
            self.selected = Some(name);
            return;
        }
        if !self.selected.as_deref().is_some_and(exists) {
            self.selected = system.components.first().map(|c| c.name.clone());
        }
    }

    /// The right pane, one renderer per category.
    fn content(
        &mut self,
        ui: &mut egui::Ui,
        system: &DesignSystem,
        render_available: bool,
        agent_ready: bool,
        action: &mut Action,
    ) {
        ui.add_space(theme::SPACE_4);
        match self.category {
            Category::Overview => overview(ui, system, &mut self.brief_cache),
            Category::Colors => colors(ui, system),
            Category::Typography => typography(ui, system),
            Category::Spacing => spacing(ui, system),
            Category::Radius => radius(ui, system),
            Category::Shadows => shadows(ui, system),
            Category::Size => size(ui, system),
            Category::Components => {
                self.components(ui, system, render_available, agent_ready, action)
            }
            Category::Assets => assets(ui, system),
            Category::Files => files(ui, system),
        }
    }

    /// The Components pane: one component at a time.
    fn components(
        &mut self,
        ui: &mut egui::Ui,
        system: &DesignSystem,
        render_available: bool,
        agent_ready: bool,
        action: &mut Action,
    ) {
        let p = theme::palette(ui);
        let selected = self.selected.clone();
        let component = selected
            .as_deref()
            .and_then(|name| system.components.iter().find(|c| c.name == name));

        ui.horizontal(|ui| {
            let title = component.map(|c| c.name.as_str()).unwrap_or("Components");
            ui.label(
                egui::RichText::new(title)
                    .font(theme::ui_strong())
                    .color(p.ink),
            );
            ui.label(
                egui::RichText::new(system.components.len().to_string())
                    .font(theme::meta())
                    .color(p.ink_faint),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.new_component_button(ui);
            });
        });
        if let Some(status) = &self.status {
            muted(ui, status);
        }
        if self.form_open {
            self.new_component_form(ui, action, agent_ready);
        }
        ui.separator();

        let Some(component) = component else {
            ui.label(
                egui::RichText::new(if system.components.is_empty() {
                    "No components yet."
                } else {
                    "Pick a component in the list."
                })
                .color(p.ink_faint)
                .size(11.0),
            );
            return;
        };

        if let Some(summary) = design::css::component_summary(component) {
            muted(ui, &summary);
        }
        ui.add_space(theme::SPACE_3);
        self.preview_card(ui, component, render_available, action);

        let Some(readme) = design::css::component_readme(component) else {
            return;
        };
        let notes = without_heading(&readme);
        if notes.is_empty() {
            return;
        }
        ui.add_space(theme::SPACE_4);
        ui.separator();
        ui.add_space(theme::SPACE_3);
        let styles = &mut ui.style_mut().text_styles;
        styles.insert(egui::TextStyle::Body, egui::FontId::proportional(13.0));
        styles.insert(egui::TextStyle::Heading, theme::ui_title());
        styles.insert(egui::TextStyle::Monospace, egui::FontId::monospace(12.0));
        ui.spacing_mut().item_spacing.y = theme::SPACE_3;
        CommonMarkViewer::new().show(ui, &mut self.brief_cache, notes);
    }

    /// The selected component's preview frame, rendered lazily once in view.
    fn preview_card(
        &mut self,
        ui: &mut egui::Ui,
        component: &Component,
        render_available: bool,
        action: &mut Action,
    ) {
        let p = theme::palette(ui);
        let width = ui.available_width().clamp(200.0, 680.0);
        // Reserve the height the card declares (a 56pt checkbox is a 56pt
        // card), rather than a fixed 16:9 frame that leaves dead space.
        let reference = component.card.height.unwrap_or(640) as f32;
        let height = (width * reference / 900.0).clamp(48.0, 720.0);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
        frame_placeholder(ui, rect, p);

        if !component.has_preview {
            centred(ui, rect, "No preview.html", p.ink_faint);
            return;
        }
        if !render_available {
            centred(ui, rect, "No preview renderer on this machine", p.ink_faint);
            return;
        }

        let name = component.name.clone();
        if ui.is_rect_visible(rect)
            && !self.previews.contains_key(&name)
            && self.running.is_none()
            && matches!(action, Action::Keep)
        {
            self.previews.insert(name.clone(), Preview::Pending);
            *action = Action::Render(name.clone());
        }

        match self.previews.get(&name) {
            Some(Preview::Ready(texture)) => {
                let size = texture.size_vec2();
                let inner = rect.shrink(theme::SPACE_3);
                let scale = (inner.width() / size.x)
                    .min(inner.height() / size.y)
                    .min(1.0);
                let draw = Rect::from_center_size(inner.center(), size * scale);
                ui.painter().image(
                    texture.id(),
                    draw,
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            Some(Preview::Pending) => centred(ui, rect, "Rendering…", p.ink_muted),
            Some(Preview::Failed(err)) => {
                let first = err.lines().next().unwrap_or("Preview failed");
                centred(ui, rect, first, p.danger);
            }
            None => centred(ui, rect, "Loading…", p.ink_faint),
        }
    }

    fn new_component_button(&mut self, ui: &mut egui::Ui) {
        let label = if self.form_open {
            "Cancel"
        } else {
            "New component…"
        };
        if widgets::button(
            ui,
            ButtonKind::Secondary,
            Some(theme::Icon::Plus),
            label,
            None,
        )
        .clicked()
        {
            self.form_open = !self.form_open;
        }
    }

    fn new_component_form(&mut self, ui: &mut egui::Ui, action: &mut Action, agent_ready: bool) {
        let p = theme::palette(ui);
        egui::Grid::new("design-new-component")
            .num_columns(2)
            .spacing([8.0, 6.0])
            .show(ui, |ui| {
                ui.label(egui::RichText::new("Name").color(p.ink_muted).size(11.0));
                ui.add(
                    egui::TextEdit::singleline(&mut self.name)
                        .hint_text("Toast")
                        .desired_width(240.0),
                );
                ui.end_row();

                ui.label(egui::RichText::new("Group").color(p.ink_muted).size(11.0));
                ui.add(egui::TextEdit::singleline(&mut self.group).desired_width(240.0));
                ui.end_row();

                ui.label(egui::RichText::new("Intent").color(p.ink_muted).size(11.0));
                ui.add(
                    egui::TextEdit::singleline(&mut self.intent)
                        .hint_text("Transient confirmation")
                        .desired_width(240.0),
                );
                ui.end_row();
            });
        if agent_ready {
            ui.checkbox(&mut self.draft, "Draft the preview with the agent");
        } else {
            self.draft = false;
        }
        let ready = !self.name.trim().is_empty();
        if ui
            .add_enabled(ready, egui::Button::new("Create component"))
            .on_disabled_hover_text("Name the component first")
            .clicked()
        {
            *action = Action::Create {
                name: self.name.trim().to_string(),
                group: self.group.trim().to_string(),
                intent: self.intent.trim().to_string(),
                draft: self.draft && agent_ready,
            };
            self.name.clear();
            self.intent.clear();
            self.form_open = false;
        }
    }

    /// Poll the running render and, when it lands, decode it into a texture.
    fn poll(&mut self, ctx: &egui::Context) {
        let Some(result) = self.running.as_ref().map(|(_, job)| job.answer()) else {
            return;
        };
        let Some(result) = result else {
            ctx.request_repaint_after(job::POLL_EVERY);
            return;
        };
        let Some((name, _)) = self.running.take() else {
            return;
        };
        let preview = match result.and_then(|path| load_texture(ctx, &path)) {
            Ok(texture) => Preview::Ready(texture),
            Err(err) => Preview::Failed(err),
        };
        self.previews.insert(name, preview);
        // Let the next card in view load without waiting for input.
        ctx.request_repaint();
    }
}

/// The second level's header: a back chevron and the level's name.
fn back_row(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let p = theme::palette(ui);
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 26.0), egui::Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        if response.hovered() {
            painter.rect_filled(rect, CornerRadius::same(theme::RADIUS_SM), p.surface_hover);
        }
        let icon = Rect::from_center_size(
            egui::Pos2::new(
                rect.min.x + theme::SPACE_3 + theme::ICON_SIZE / 2.0,
                rect.center().y,
            ),
            egui::Vec2::splat(theme::ICON_SIZE),
        );
        theme::paint_icon(painter, theme::Icon::Back, icon, p.ink_muted);
        painter.text(
            egui::Pos2::new(icon.max.x + theme::SPACE_2, rect.center().y),
            Align2::LEFT_CENTER,
            label,
            theme::ui_strong(),
            p.ink,
        );
    }
    response
}

/// A sidebar row: the category name, an optional `meta` count, and the
/// `accent-soft` fill when it is the open one.
fn nav_row(ui: &mut egui::Ui, label: &str, meta: Option<&str>, selected: bool) -> egui::Response {
    let p = theme::palette(ui);
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 26.0), egui::Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let fill = if selected {
            p.accent_soft
        } else if response.hovered() {
            p.surface_hover
        } else {
            Color32::TRANSPARENT
        };
        painter.rect_filled(rect, CornerRadius::same(theme::RADIUS_SM), fill);
        if let Some(meta) = meta {
            painter.text(
                egui::Pos2::new(rect.max.x - theme::SPACE_3, rect.center().y),
                Align2::RIGHT_CENTER,
                meta,
                theme::meta(),
                p.ink_faint,
            );
        }
        painter.text(
            egui::Pos2::new(rect.min.x + theme::SPACE_3, rect.center().y),
            Align2::LEFT_CENTER,
            label,
            theme::ui_body(),
            if selected { p.ink } else { p.ink_muted },
        );
    }
    response
}

/// A component README without its leading `# Name` heading, whose text the
/// pane already shows as its title.
fn without_heading(readme: &str) -> &str {
    let readme = readme.trim();
    match readme.strip_prefix("# ") {
        Some(rest) => rest.find('\n').map_or("", |at| rest[at + 1..].trim()),
        None => readme,
    }
}

/// A category heading inside the content pane.
fn heading(ui: &mut egui::Ui, text: &str) {
    let p = theme::palette(ui);
    ui.add_space(theme::SPACE_4);
    ui.label(
        egui::RichText::new(text)
            .font(theme::ui_label())
            .color(p.ink_faint),
    );
}

/// A quiet line of detail.
fn muted(ui: &mut egui::Ui, text: &str) {
    let p = theme::palette(ui);
    ui.label(egui::RichText::new(text).color(p.ink_muted).size(11.0));
}

/// The background of a preview card.
fn frame_placeholder(ui: &egui::Ui, rect: Rect, p: &theme::Palette) {
    ui.painter().rect(
        rect,
        CornerRadius::same(theme::RADIUS_MD),
        p.surface_sunken,
        Stroke::new(1.0, p.line),
        StrokeKind::Inside,
    );
}

/// A line of text, centred in a preview card.
fn centred(ui: &egui::Ui, rect: Rect, text: &str, color: Color32) {
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        theme::ui_body(),
        color,
    );
}

/// The Overview landing: identity, counts, the last change and the brief.
fn overview(ui: &mut egui::Ui, system: &DesignSystem, cache: &mut CommonMarkCache) {
    let p = theme::palette(ui);
    ui.label(
        egui::RichText::new(&system.meta.title)
            .font(theme::ui_title())
            .color(p.ink),
    );
    muted(
        ui,
        &format!("{} · v{}", system.meta.namespace, system.meta.version),
    );

    let themes: Vec<&str> = system.tokens.themes.iter().map(|t| t.id.as_str()).collect();
    muted(
        ui,
        &format!(
            "{} components · {} color, {} spacing, {} radius, {} size, {} shadow, {} type styles · themes {}",
            system.components.len(),
            system.tokens.color.len(),
            system.tokens.spacing.len(),
            system.tokens.radius.len(),
            system.tokens.size.len(),
            system.tokens.shadow.len(),
            system.tokens.type_style_count(),
            if themes.is_empty() {
                "—".to_string()
            } else {
                themes.join(", ")
            },
        ),
    );

    if let Some(change) = &system.meta.last_change {
        let who = change.by.as_deref().unwrap_or("unknown");
        let at = change.at.as_deref().unwrap_or("");
        let via = change.via.as_deref().unwrap_or("");
        muted(ui, &format!("Last change {who}, {at} via {via}"));
    }
    if !system.warnings.is_empty() {
        muted(ui, &format!("{} warning(s)", system.warnings.len()));
    }

    ui.separator();
    let Some(brief) = system.brief() else {
        ui.label(
            egui::RichText::new("No design/README.md brief yet.")
                .color(p.ink_faint)
                .size(11.0),
        );
        return;
    };
    // Render the brief as markdown, at reading sizes a touch smaller than the
    // Reader's so it fits the panel.
    let styles = &mut ui.style_mut().text_styles;
    styles.insert(egui::TextStyle::Body, egui::FontId::proportional(13.0));
    styles.insert(egui::TextStyle::Heading, theme::ui_title());
    styles.insert(egui::TextStyle::Monospace, egui::FontId::monospace(12.0));
    ui.spacing_mut().item_spacing.y = theme::SPACE_3;
    CommonMarkViewer::new().show(ui, cache, &brief);
}

/// Colors: one row per color token, a swatch per theme, copy on click.
fn colors(ui: &mut egui::Ui, system: &DesignSystem) {
    let p = theme::palette(ui);
    let themes: Vec<String> = if system.tokens.themes.is_empty() {
        vec!["light".to_string()]
    } else {
        system.tokens.themes.iter().map(|t| t.id.clone()).collect()
    };

    for token in &system.tokens.color {
        ui.horizontal(|ui| {
            ui.add_sized(
                [150.0, 18.0],
                egui::Label::new(
                    egui::RichText::new(&token.name)
                        .font(theme::meta())
                        .color(p.ink),
                ),
            );
            for id in &themes {
                let value = design::css::token_value_for(token, id).unwrap_or_default();
                let color = swatch_color(system, token, id).unwrap_or(p.surface_sunken);
                let swatch = swatch(ui, color).on_hover_text(format!("{id}: {value}"));
                if swatch.clicked() {
                    ui.ctx().copy_text(value.clone());
                }
                ui.label(
                    egui::RichText::new(&value)
                        .font(theme::meta())
                        .color(p.ink_muted),
                );
            }
            if let Some(usage) = &token.usage {
                ui.label(egui::RichText::new(usage).color(p.ink_faint).size(11.0));
            }
        });
    }
}

/// Typography: each style rendered as a real sample in its family.
fn typography(ui: &mut egui::Ui, system: &DesignSystem) {
    let p = theme::palette(ui);
    for group in &system.tokens.type_groups {
        heading(ui, &group.name);
        if let Some(stack) = system.tokens.families.get(&group.family) {
            muted(ui, stack);
        }
        for style in &group.styles {
            let size = parse_px(style.font_size.as_deref().unwrap_or("13px")).unwrap_or(13.0);
            let font = if group.family.contains("mono") {
                egui::FontId::monospace(size)
            } else {
                egui::FontId::proportional(size)
            };
            let sample = style.sample.clone().unwrap_or_else(|| style.name.clone());
            ui.label(egui::RichText::new(sample).font(font).color(p.ink));
            let weight = style
                .weight
                .map(|w| w.to_string())
                .unwrap_or_else(|| "—".to_string());
            let line = style.line_height.as_deref().unwrap_or("—");
            muted(
                ui,
                &format!("{} · {}px · {} · line {line}", style.name, size, weight),
            );
            ui.add_space(theme::SPACE_3);
        }
        ui.add_space(theme::SPACE_4);
    }
}

/// Spacing: the scale as horizontal bars.
fn spacing(ui: &mut egui::Ui, system: &DesignSystem) {
    let p = theme::palette(ui);
    for token in &system.tokens.spacing {
        let value = design::css::token_value_for(token, "light").unwrap_or_default();
        let resolved = design::css::resolved_value(system, token, "light").unwrap_or_default();
        let px = parse_px(&resolved).unwrap_or(0.0);
        ui.horizontal(|ui| {
            ui.add_sized(
                [90.0, 18.0],
                egui::Label::new(
                    egui::RichText::new(&token.name)
                        .font(theme::meta())
                        .color(p.ink),
                ),
            );
            let (rect, _) =
                ui.allocate_exact_size(egui::vec2(px.max(1.0), 14.0), egui::Sense::hover());
            ui.painter()
                .rect_filled(rect, CornerRadius::same(2), p.accent);
            ui.label(
                egui::RichText::new(&value)
                    .font(theme::meta())
                    .color(p.ink_muted),
            );
        });
    }
}

/// Corner radius: a square per token, drawn with that radius.
fn radius(ui: &mut egui::Ui, system: &DesignSystem) {
    let p = theme::palette(ui);
    for token in &system.tokens.radius {
        let value = design::css::token_value_for(token, "light").unwrap_or_default();
        let resolved = design::css::resolved_value(system, token, "light").unwrap_or_default();
        let px = parse_px(&resolved).unwrap_or(0.0);
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(56.0, 56.0), egui::Sense::hover());
            ui.painter().rect(
                rect,
                CornerRadius::same(px.clamp(0.0, 28.0) as u8),
                p.surface_sunken,
                Stroke::new(1.0, p.line_strong),
                StrokeKind::Inside,
            );
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(&token.name)
                        .font(theme::meta())
                        .color(p.ink),
                );
                ui.label(
                    egui::RichText::new(&value)
                        .font(theme::meta())
                        .color(p.ink_muted),
                );
            });
        });
        ui.add_space(theme::SPACE_3);
    }
}

/// Shadows: a lifted card per token, with its value per theme.
fn shadows(ui: &mut egui::Ui, system: &DesignSystem) {
    let p = theme::palette(ui);
    for token in &system.tokens.shadow {
        let value = design::css::token_value_for(token, "light").unwrap_or_default();
        let resolved = design::css::resolved_value(system, token, "light").unwrap_or_default();
        let (offset, blur, color) = parse_shadow(&resolved);
        ui.horizontal(|ui| {
            egui::Frame::new()
                .fill(p.surface)
                .corner_radius(CornerRadius::same(theme::RADIUS_MD))
                .shadow(egui::epaint::Shadow {
                    offset,
                    blur,
                    spread: 0,
                    color: if color == Color32::TRANSPARENT {
                        p.popover_shadow
                    } else {
                        color
                    },
                })
                .inner_margin(egui::Margin::same(theme::SPACE_4 as i8))
                .show(ui, |ui| {
                    ui.set_min_width(140.0);
                    ui.label(
                        egui::RichText::new(&token.name)
                            .font(theme::meta())
                            .color(p.ink),
                    );
                });
            ui.vertical(|ui| {
                for theme in &system.tokens.themes {
                    let value = design::css::token_value_for(token, &theme.id).unwrap_or_default();
                    muted(ui, &format!("{}: {value}", theme.id));
                }
                if system.tokens.themes.is_empty() {
                    muted(ui, &value);
                }
            });
        });
        ui.add_space(theme::SPACE_5);
    }
}

/// Size: the fixed measurements, with the section's note.
fn size(ui: &mut egui::Ui, system: &DesignSystem) {
    let p = theme::palette(ui);
    for token in &system.tokens.size {
        let value = design::css::token_value_for(token, "light").unwrap_or_default();
        ui.horizontal(|ui| {
            ui.add_sized(
                [120.0, 18.0],
                egui::Label::new(
                    egui::RichText::new(&token.name)
                        .font(theme::meta())
                        .color(p.ink),
                ),
            );
            ui.label(egui::RichText::new(&value).color(p.ink).size(12.0));
            if let Some(usage) = &token.usage {
                ui.label(egui::RichText::new(usage).color(p.ink_faint).size(11.0));
            }
        });
    }
    if let Some(note) = &system.tokens.size_note {
        ui.add_space(theme::SPACE_3);
        muted(ui, note);
    }
}

/// Assets: the bundled fonts and any declared images, grouped.
fn assets(ui: &mut egui::Ui, system: &DesignSystem) {
    let p = theme::palette(ui);
    let assets = system.assets();
    if assets.is_empty() {
        ui.label(
            egui::RichText::new("No assets bundled or declared.")
                .color(p.ink_faint)
                .size(11.0),
        );
        return;
    }
    let mut current = String::new();
    for asset in &assets {
        if asset.group != current {
            heading(ui, &asset.group);
            current = asset.group.clone();
        }
        ui.horizontal(|ui| {
            if let Some(path) = asset.path.as_deref() {
                if is_image(path) {
                    ui.add(
                        egui::Image::new(format!("file://{}", path.display()))
                            .fit_to_exact_size(egui::vec2(40.0, 40.0))
                            .corner_radius(CornerRadius::same(theme::RADIUS_SM)),
                    );
                } else {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::hover());
                    ui.painter().rect(
                        rect,
                        CornerRadius::same(theme::RADIUS_SM),
                        p.surface_sunken,
                        Stroke::new(1.0, p.line),
                        StrokeKind::Inside,
                    );
                }
            }
            ui.label(egui::RichText::new(&asset.name).color(p.ink).size(12.0));
            if asset.bytes > 0 {
                muted(ui, &human_bytes(asset.bytes));
            }
        });
    }
}

/// Files: every file under `design/`, with a count.
fn files(ui: &mut egui::Ui, system: &DesignSystem) {
    let p = theme::palette(ui);
    let files = system.files();
    heading(ui, &format!("Files {}", files.len()));
    for file in files {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(&file.rel)
                    .font(theme::meta())
                    .color(p.ink),
            );
            muted(ui, &human_bytes(file.bytes));
        });
    }
}

/// A color swatch; a click is the caller's to handle.
fn swatch(ui: &mut egui::Ui, color: Color32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let p = theme::palette(ui);
        ui.painter().rect(
            rect,
            CornerRadius::same(theme::RADIUS_SM),
            color,
            Stroke::new(1.0, p.line_strong),
            StrokeKind::Inside,
        );
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The colour a token shows in `theme`, its `{references}` resolved in that
/// same theme.
fn swatch_color(system: &DesignSystem, token: &Token, theme: &str) -> Option<Color32> {
    parse_color(&design::css::resolved_value(system, token, theme)?)
}

/// Parse a CSS color: `#rgb`, `#rrggbb`, `#rrggbbaa`, `rgb()` or `rgba()`.
fn parse_color(text: &str) -> Option<Color32> {
    let text = text.trim();
    if let Some(hex) = text.strip_prefix('#') {
        let hex = hex.trim();
        if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        let channel = |pair: &str| u8::from_str_radix(pair, 16).ok();
        return match hex.len() {
            3 => {
                let d = |i: usize| {
                    let c = hex.as_bytes()[i] as char;
                    channel(&format!("{c}{c}"))
                };
                Some(Color32::from_rgb(d(0)?, d(1)?, d(2)?))
            }
            6 => Some(Color32::from_rgb(
                channel(&hex[0..2])?,
                channel(&hex[2..4])?,
                channel(&hex[4..6])?,
            )),
            8 => Some(Color32::from_rgba_unmultiplied(
                channel(&hex[0..2])?,
                channel(&hex[2..4])?,
                channel(&hex[4..6])?,
                channel(&hex[6..8])?,
            )),
            _ => None,
        };
    }
    let inner = text
        .strip_prefix("rgba")
        .or_else(|| text.strip_prefix("rgb"))?
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')');
    let parts: Vec<&str> = inner.split([',', '/']).map(str::trim).collect();
    let channel = |part: &str| -> Option<u8> {
        if let Some(percent) = part.strip_suffix('%') {
            return percent
                .parse::<f32>()
                .ok()
                .map(|v| (v / 100.0 * 255.0).round().clamp(0.0, 255.0) as u8);
        }
        part.parse::<f32>()
            .ok()
            .map(|v| v.round().clamp(0.0, 255.0) as u8)
    };
    let r = channel(parts.first()?)?;
    let g = channel(parts.get(1)?)?;
    let b = channel(parts.get(2)?)?;
    let a = parts
        .get(3)
        .and_then(|part| part.parse::<f32>().ok())
        .map_or(255, |alpha| (alpha * 255.0).round().clamp(0.0, 255.0) as u8);
    Some(Color32::from_rgba_unmultiplied(r, g, b, a))
}

/// A number with an optional `px` suffix (`"4px"`, `"0"`, `"16"`).
fn parse_px(text: &str) -> Option<f32> {
    let text = text.trim();
    let text = text.strip_suffix("px").unwrap_or(text);
    text.trim().parse::<f32>().ok()
}

/// The offset, blur and color of a CSS `box-shadow`, for `egui`'s `Shadow`.
fn parse_shadow(text: &str) -> ([i8; 2], u8, Color32) {
    let mut numbers = Vec::new();
    let mut color = None;
    for part in text.split_whitespace() {
        if part.starts_with("rgb") {
            color = parse_color(part);
        } else if let Some(number) = parse_px(part) {
            numbers.push(number);
        }
    }
    let offset = [
        numbers.first().copied().unwrap_or(0.0) as i8,
        numbers.get(1).copied().unwrap_or(4.0) as i8,
    ];
    let blur = numbers.get(2).copied().unwrap_or(16.0).clamp(0.0, 255.0) as u8;
    (offset, blur, color.unwrap_or(Color32::TRANSPARENT))
}

fn is_image(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()).map(str::to_lowercase),
        Some(ext) if matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp")
    )
}

fn human_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.0} kB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// The components the rail lists, narrowed to those matching `query`, in the
/// alphabetical order Components has in Claude Design.
fn ordered_components<'a>(system: &'a DesignSystem, query: &str) -> Vec<&'a Component> {
    let query = query.trim().to_lowercase();
    let mut list: Vec<&Component> = system
        .components
        .iter()
        .filter(|c| query.is_empty() || c.name.to_lowercase().contains(&query))
        .collect();
    list.sort_by_key(|c| c.name.to_lowercase());
    list
}

/// Search every section for entries whose name matches `query`.
fn search(system: &DesignSystem, query: &str) -> Vec<Hit> {
    let matches = |name: &str| name.to_lowercase().contains(query);
    let mut hits = Vec::new();
    let mut token_hits = |category: Category, detail: &str, tokens: &[Token]| {
        for token in tokens {
            if matches(&token.name) {
                hits.push(Hit {
                    category,
                    label: token.name.clone(),
                    detail: detail.to_string(),
                });
            }
        }
    };
    token_hits(Category::Colors, "color", &system.tokens.color);
    token_hits(Category::Spacing, "spacing", &system.tokens.spacing);
    token_hits(Category::Radius, "radius", &system.tokens.radius);
    token_hits(Category::Shadows, "shadow", &system.tokens.shadow);
    token_hits(Category::Size, "size", &system.tokens.size);
    for component in &system.components {
        if matches(&component.name) {
            hits.push(Hit {
                category: Category::Components,
                label: component.name.clone(),
                detail: "component".to_string(),
            });
        }
    }
    for group in &system.tokens.type_groups {
        for style in &group.styles {
            if matches(&style.name) {
                hits.push(Hit {
                    category: Category::Typography,
                    label: style.name.clone(),
                    detail: group.name.clone(),
                });
            }
        }
    }
    hits.truncate(40);
    hits
}

/// Decode a PNG into an egui texture.
pub fn load_texture(ctx: &egui::Context, path: &Path) -> Result<egui::TextureHandle, String> {
    let bytes =
        std::fs::read(path).map_err(|err| format!("Couldn't read {}: {err}", path.display()))?;
    let image = image::load_from_memory(&bytes)
        .map_err(|err| format!("Couldn't decode the preview: {err}"))?
        .to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
    Ok(ctx.load_texture(path.to_string_lossy(), color, egui::TextureOptions::LINEAR))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> DesignSystem {
        DesignSystem::discover(Path::new(env!("CARGO_MANIFEST_DIR"))).expect("repo design")
    }

    fn panel_ctx() -> (egui::Context, egui::RawInput) {
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(980.0, 640.0),
            )),
            ..Default::default()
        };
        (ctx, input)
    }

    /// Drive one frame of the panel and return what it asked for.
    fn frame(panel: &mut Panel, system: &DesignSystem, render_available: bool) -> Action {
        let (ctx, raw) = panel_ctx();
        let mut action = Action::Keep;
        let mut output = ctx.run_ui(raw, |ui| {
            action = panel.body(ui, Some(system), None, render_available, false);
        });
        output.textures_delta.clear();
        action
    }

    #[test]
    fn the_selected_component_requests_one_lazy_render() {
        let system = repo();
        let mut panel = Panel {
            category: Category::Components,
            components_open: true,
            selected: Some("Button".to_string()),
            ..Panel::default()
        };
        match frame(&mut panel, &system, true) {
            Action::Render(name) => assert_eq!(name, "Button"),
            _ => panic!("the selected component should load its preview lazily"),
        }
        // Only one card is asked for per frame, and it is marked pending.
        assert_eq!(panel.previews.len(), 1);
        assert!(panel.running.is_none(), "the host has not started it yet");
    }

    #[test]
    fn no_render_without_a_renderer() {
        let system = repo();
        let mut panel = Panel {
            category: Category::Components,
            components_open: true,
            selected: Some("Button".to_string()),
            ..Panel::default()
        };
        assert!(matches!(frame(&mut panel, &system, false), Action::Keep));
        assert!(panel.previews.is_empty());
    }

    #[test]
    fn a_second_frame_waits_while_a_render_runs() {
        let system = repo();
        let mut panel = Panel {
            category: Category::Components,
            components_open: true,
            selected: Some("Button".to_string()),
            ..Panel::default()
        };
        let _ = frame(&mut panel, &system, true);
        // Pretend the host started the render and it is still running.
        let running = Job::spawn(|cancel| {
            while !cancel.is_cancelled() {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            Err("stopped".to_string())
        });
        panel.start_render("Button".to_string(), running);
        assert!(matches!(frame(&mut panel, &system, true), Action::Keep));
    }

    #[test]
    fn a_redrafted_preview_is_rendered_again() {
        let mut panel = Panel::default();
        panel.fail_render("Toast", "old fragment".to_string());
        panel.start_render("Toast".to_string(), Job::done(Err("old".to_string())));
        panel.fail_render("Button", "unrelated".to_string());

        panel.forget_preview("Toast");

        assert!(!panel.previews.contains_key("Toast"));
        assert!(
            panel.running.is_none(),
            "the render of the old fragment is dropped"
        );
        assert!(panel.previews.contains_key("Button"));
    }

    #[test]
    fn opening_components_picks_one_and_back_leaves_the_pane() {
        let system = repo();
        let mut panel = Panel::default();
        panel.open_components(&system, None);
        assert!(panel.components_open);
        assert_eq!(panel.category, Category::Components);
        assert!(panel.selected.is_some(), "the first component is picked");

        panel.open_components(&system, Some("Button".to_string()));
        assert_eq!(panel.selected.as_deref(), Some("Button"));

        // Back only closes the rail; the pane keeps the chosen component.
        panel.components_open = false;
        assert_eq!(panel.selected.as_deref(), Some("Button"));
    }

    #[test]
    fn components_are_listed_alphabetically() {
        let system = repo();
        let names = |query: &str| {
            ordered_components(&system, query)
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>()
        };
        let all = names("");
        let mut sorted = all.clone();
        sorted.sort_by_key(|name| name.to_lowercase());
        assert_eq!(all, sorted, "the rail lists components alphabetically");
        assert!(all.contains(&"Button"));
        assert_eq!(names("button"), vec!["Button", "ReviewButton"]);
    }

    #[test]
    fn every_category_renders_without_panicking() {
        let system = repo();
        for category in Category::ALL {
            let mut panel = Panel {
                category,
                ..Panel::default()
            };
            frame(&mut panel, &system, false);
        }
    }

    #[test]
    fn a_component_readme_drops_its_title_heading() {
        assert_eq!(without_heading("# Button\n\nA verb.\n"), "A verb.");
        assert_eq!(without_heading("No heading here"), "No heading here");
        assert_eq!(without_heading("# Only a title"), "");
    }

    #[test]
    fn colors_and_shadows_parse() {
        assert_eq!(
            parse_color("#ffffff"),
            Some(Color32::from_rgb(255, 255, 255))
        );
        assert_eq!(parse_color("#000"), Some(Color32::from_rgb(0, 0, 0)));
        assert_eq!(
            parse_color("rgba(21,24,29,0.12)"),
            Some(Color32::from_rgba_unmultiplied(21, 24, 29, 31))
        );
        assert_eq!(parse_color("oklch(0.6 0.2 250)"), None);
        assert_eq!(parse_color("#€€"), None, "six bytes, not six digits");
        assert_eq!(parse_color("#é"), None);

        let (offset, blur, _) = parse_shadow("0 4px 16px rgba(21,24,29,0.12)");
        assert_eq!(offset, [0, 4]);
        assert_eq!(blur, 16);
    }

    #[test]
    fn search_finds_tokens_and_components() {
        let system = repo();
        let hits = search(&system, "butt");
        assert!(
            hits.iter()
                .any(|hit| hit.category == Category::Components && hit.label == "Button")
        );
        let hits = search(&system, "space-");
        assert!(!hits.is_empty());
        assert!(hits.iter().all(|hit| hit.category == Category::Spacing));
    }

    #[test]
    fn a_swatch_resolves_its_references_in_its_own_theme() {
        let system = repo();
        let focus = system.token("focus").expect("focus is {accent}");
        let accent = |theme| {
            let accent = system.token("accent").unwrap();
            parse_color(&design::css::token_value_for(accent, theme).unwrap())
        };
        assert_eq!(swatch_color(&system, focus, "light"), accent("light"));
        assert_eq!(swatch_color(&system, focus, "dark"), accent("dark"));
        assert_ne!(accent("light"), accent("dark"));
    }
}
