//! The Design panel's pages for every category but Components: the Overview,
//! a page per kind of token, Assets and Files.

use std::path::Path;

use egui::{Color32, CornerRadius, Stroke, StrokeKind};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

use super::{heading, muted};
use crate::design::{self, Asset, DesignFile, DesignSystem, Token};
use crate::format;
use crate::theme;

/// The Overview landing: identity, counts, the last change and the brief.
pub(super) fn overview(
    ui: &mut egui::Ui,
    system: &DesignSystem,
    brief: Option<&str>,
    cache: &mut CommonMarkCache,
) {
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
            "{} components · {} color, {} spacing, {} radius, {} size, {} shadow, {} opacity, {} motion, {} type styles · themes {}",
            system.components.len(),
            system.tokens.color.len(),
            system.tokens.spacing.len(),
            system.tokens.radius.len(),
            system.tokens.size.len(),
            system.tokens.shadow.len(),
            system.tokens.opacity.len(),
            system.tokens.motion.len(),
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
    let Some(brief) = brief else {
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
    CommonMarkViewer::new().show(ui, cache, brief);
}

/// Colors: one row per color token, a swatch per theme, copy on click.
pub(super) fn colors(ui: &mut egui::Ui, system: &DesignSystem) {
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
                let swatch = swatch(ui, color, &format!("{id}: {value}"));
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
pub(super) fn typography(ui: &mut egui::Ui, system: &DesignSystem) {
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
pub(super) fn spacing(ui: &mut egui::Ui, system: &DesignSystem) {
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
pub(super) fn radius(ui: &mut egui::Ui, system: &DesignSystem) {
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
pub(super) fn shadows(ui: &mut egui::Ui, system: &DesignSystem) {
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
                .inner_margin(theme::SPACE_4)
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

/// Size, Opacity and Motion: one value per token, with the section's note.
pub(super) fn plain_tokens(ui: &mut egui::Ui, tokens: &[Token], note: Option<&str>) {
    let p = theme::palette(ui);
    for token in tokens {
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
    if let Some(note) = note {
        ui.add_space(theme::SPACE_3);
        muted(ui, note);
    }
}

/// Assets: the bundled fonts and any declared images, grouped.
pub(super) fn assets(ui: &mut egui::Ui, assets: &[Asset]) {
    let p = theme::palette(ui);
    if assets.is_empty() {
        ui.label(
            egui::RichText::new("No assets bundled or declared.")
                .color(p.ink_faint)
                .size(11.0),
        );
        return;
    }
    let mut current = String::new();
    for asset in assets {
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
                muted(ui, &format::bytes(asset.bytes));
            }
        });
    }
}

/// Files: every file under `design/`, with a count.
pub(super) fn files(ui: &mut egui::Ui, files: &[DesignFile]) {
    let p = theme::palette(ui);
    heading(ui, &format!("Files {}", files.len()));
    for file in files {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(&file.rel)
                    .font(theme::meta())
                    .color(p.ink),
            );
            muted(ui, &format::bytes(file.bytes));
        });
    }
}

/// A color swatch named by its tooltip, `hover`; a click is the caller's to
/// handle.
fn swatch(ui: &mut egui::Ui, color: Color32, hover: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), hover)
    });
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
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(hover)
}

/// The colour a token shows in `theme`, its `{references}` resolved in that
/// same theme.
pub(super) fn swatch_color(system: &DesignSystem, token: &Token, theme: &str) -> Option<Color32> {
    parse_color(&design::css::resolved_value(system, token, theme)?)
}

/// Parse a CSS color: `#rgb`, `#rrggbb`, `#rrggbbaa`, `rgb()` or `rgba()`.
pub(super) fn parse_color(text: &str) -> Option<Color32> {
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
pub(super) fn parse_shadow(text: &str) -> ([i8; 2], u8, Color32) {
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
