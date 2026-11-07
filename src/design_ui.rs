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
use std::time::{Duration, Instant};

use egui::{Align2, Color32, CornerRadius, Rect, Stroke, StrokeKind, pos2};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

use crate::design::{self, Asset, CardMeta, Component, DesignFile, DesignSystem, Token};
use crate::job::{self, Job};

use crate::theme;
use crate::widgets::{self, ButtonKind};

mod pages;

use pages::{assets, colors, files, overview, plain_tokens, radius, shadows, spacing, typography};

/// Width of the browser's category sidebar.
const SIDEBAR_W: f32 = 200.0;

/// How long the rail slides between its two levels, in seconds.
const RAIL_SLIDE_SECS: f32 = 0.22;

/// How tall a preview card is when its `@dsCard` declares no height.
const UNDECLARED_CARD_H: f32 = 480.0;

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
    /// Write the catalog page and show it in a live webview, in the theme the
    /// panel draws in.
    OpenCatalog {
        theme: &'static str,
    },
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
    Opacity,
    Motion,
    Components,
    Assets,
    Files,
}

impl Category {
    /// Every category, in sidebar order.
    const ALL: [Category; 12] = [
        Category::Overview,
        Category::Colors,
        Category::Typography,
        Category::Spacing,
        Category::Radius,
        Category::Shadows,
        Category::Size,
        Category::Opacity,
        Category::Motion,
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
            Category::Opacity => "Opacity",
            Category::Motion => "Motion",
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
    /// What the panes read from the design folder.
    disk: Disk,
}

/// What the panes read from the design folder, read again at most every
/// [`DISK_EVERY`], or when the system changes.
#[derive(Default)]
struct Disk {
    /// The system it was read for: its root and its number of components.
    key: Option<(PathBuf, usize)>,
    at: Option<Instant>,
    files: Vec<DesignFile>,
    assets: Vec<Asset>,
    brief: Option<String>,
}

const DISK_EVERY: Duration = Duration::from_secs(2);

impl Disk {
    fn read(&mut self, system: &DesignSystem) {
        let key = Some((system.root.clone(), system.components.len()));
        let fresh = self.key == key && self.at.is_some_and(|at| at.elapsed() < DISK_EVERY);
        if !fresh {
            self.key = key;
            self.at = Some(Instant::now());
            self.files = system.files();
            self.assets = system.assets();
            self.brief = system.brief();
        }
    }
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
            disk: Disk::default(),
        }
    }
}

/// A vertical scroll area that fills the space it is given.
fn filling(salt: &str) -> egui::ScrollArea {
    egui::ScrollArea::vertical()
        .id_salt(salt)
        .content_margin(theme::scroll_margin(0.0, 0.0))
        .auto_shrink(false)
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
        if let Some(system) = system {
            self.disk.read(system);
        }
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
                    action = Action::OpenCatalog {
                        theme: if ui.visuals().dark_mode {
                            "dark"
                        } else {
                            "light"
                        },
                    };
                }
            });
        });
        if let Some(status) = &self.status {
            muted(ui, status);
        }

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
                theme::scroll_area(ui, filling("design-content"), |ui| {
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
        widgets::text_entry(
            ui,
            egui::TextEdit::singleline(&mut self.search)
                .hint_text("Search")
                .desired_width(f32::INFINITY),
        );
        ui.add_space(theme::SPACE_3);

        let query = self.search.trim().to_lowercase();
        if !query.is_empty() {
            theme::scroll_area(ui, filling("rail-hits"), |ui| {
                self.search_hits(ui, system, &query);
            });
            return;
        }

        // 0 = categories, 1 = the component list; the pages stay a width apart.
        let target = if self.components_open { 1.0 } else { 0.0 };
        let secs = theme::motion(ui.ctx(), RAIL_SLIDE_SECS);
        let progress =
            ui.ctx()
                .animate_value_with_time(ui.scope_id().with("rail-slide"), target, secs);
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
        theme::scroll_area(&mut main, filling("rail-main-scroll"), |ui| {
            self.categories(ui, system);
        });

        let mut components = page(ui, "rail-components", (1.0 - slide) * width);
        if !self.components_open {
            components.disable();
        }
        if back_row(&mut components, "Components").clicked() {
            self.components_open = false;
        }
        components.add_space(theme::SPACE_2);
        let rows = filling("rail-components-scroll");
        theme::scroll_area(&mut components, rows, |ui| {
            self.component_rows(ui, system);
        });
    }

    /// The rail while searching, at either level: what matches, each with its
    /// category; picking one opens it and clears the search.
    fn search_hits(&mut self, ui: &mut egui::Ui, system: &DesignSystem, query: &str) {
        let hits = search(system, query);
        if hits.is_empty() {
            ui.label(
                egui::RichText::new("No matches")
                    .color(theme::palette(ui).ink_faint)
                    .size(11.0),
            );
        }
        for hit in hits {
            if nav_row(ui, &hit.label, Some(hit.category.label()), false).clicked() {
                if hit.category == Category::Components {
                    self.open_components(system, Some(hit.label));
                } else {
                    self.category = hit.category;
                    self.components_open = false;
                }
                self.search.clear();
            }
        }
    }

    fn categories(&mut self, ui: &mut egui::Ui, system: &DesignSystem) {
        for category in Category::ALL {
            let meta = match category {
                Category::Files => Some(self.disk.files.len().to_string()),
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
        let list = system.components_by_name("");
        if list.is_empty() {
            ui.label(
                egui::RichText::new("No components yet.")
                    .color(theme::palette(ui).ink_faint)
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
            Category::Overview => overview(
                ui,
                system,
                self.disk.brief.as_deref(),
                &mut self.brief_cache,
            ),
            Category::Colors => colors(ui, system),
            Category::Typography => typography(ui, system),
            Category::Spacing => spacing(ui, system),
            Category::Radius => radius(ui, system),
            Category::Shadows => shadows(ui, system),
            Category::Size => {
                plain_tokens(ui, &system.tokens.size, system.tokens.size_note.as_deref())
            }
            Category::Opacity => plain_tokens(ui, &system.tokens.opacity, None),
            Category::Motion => plain_tokens(
                ui,
                &system.tokens.motion,
                system.tokens.motion_note.as_deref(),
            ),
            Category::Components => {
                self.components(ui, system, render_available, agent_ready, action)
            }
            Category::Assets => assets(ui, &self.disk.assets),
            Category::Files => files(ui, &self.disk.files),
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
        let height = card_height(&component.card);
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
                let fields = [
                    ("Name", &mut self.name, "Toast"),
                    ("Group", &mut self.group, ""),
                    ("Intent", &mut self.intent, "Transient confirmation"),
                ];
                for (label, text, hint) in fields {
                    ui.label(egui::RichText::new(label).color(p.ink_muted).size(11.0));
                    let edit = egui::TextEdit::singleline(text).hint_text(hint);
                    widgets::text_entry(ui, edit.desired_width(240.0));
                    ui.end_row();
                }
            });
        if agent_ready {
            widgets::checkbox(ui, &mut self.draft, "Draft the preview with the agent");
        } else {
            self.draft = false;
        }
        let ready = !self.name.trim().is_empty();
        if ui
            .add_enabled_ui(ready, |ui| {
                widgets::button(ui, ButtonKind::Secondary, None, "Create component", None)
            })
            .inner
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

/// A preview card is as tall as it declares: a 56pt checkbox is a 56pt card.
fn card_height(card: &CardMeta) -> f32 {
    card.height
        .map_or(UNDECLARED_CARD_H, |height| height as f32)
}

/// A rail row's focus ring. The rail's clip stops at the rows' edges, and the
/// ring goes outside them.
fn rail_focus_ring(ui: &mut egui::Ui, response: &egui::Response) {
    let clip = ui.clip_rect();
    ui.set_clip_rect(clip.expand(4.0));
    widgets::focus_ring(ui, response, theme::RADIUS_SM);
    ui.set_clip_rect(clip);
}

/// The second level's header: a back chevron and the level's name.
fn back_row(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let p = theme::palette(ui);
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 26.0), egui::Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), label));
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
    rail_focus_ring(ui, &response);
    response
}

/// A sidebar row: the category name, an optional `meta` count, and the
/// `accent-soft` fill when it is the open one.
fn nav_row(ui: &mut egui::Ui, label: &str, meta: Option<&str>, selected: bool) -> egui::Response {
    let p = theme::palette(ui);
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 26.0), egui::Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Button, ui.is_enabled(), selected, label)
    });
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
    rail_focus_ring(ui, &response);
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

/// Search every section for entries whose name matches `query`.
fn search(system: &DesignSystem, query: &str) -> Vec<Hit> {
    let matches = |name: &str| name.to_lowercase().contains(query);
    let mut hits = Vec::new();
    let mut token_hits = |category: Category, tokens: &[Token]| {
        for token in tokens {
            if matches(&token.name) {
                hits.push(Hit {
                    category,
                    label: token.name.clone(),
                });
            }
        }
    };
    token_hits(Category::Colors, &system.tokens.color);
    token_hits(Category::Spacing, &system.tokens.spacing);
    token_hits(Category::Radius, &system.tokens.radius);
    token_hits(Category::Shadows, &system.tokens.shadow);
    token_hits(Category::Size, &system.tokens.size);
    token_hits(Category::Opacity, &system.tokens.opacity);
    token_hits(Category::Motion, &system.tokens.motion);
    for component in &system.components {
        if matches(&component.name) {
            hits.push(Hit {
                category: Category::Components,
                label: component.name.clone(),
            });
        }
    }
    for group in &system.tokens.type_groups {
        for style in &group.styles {
            if matches(&style.name) {
                hits.push(Hit {
                    category: Category::Typography,
                    label: style.name.clone(),
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
mod tests;
