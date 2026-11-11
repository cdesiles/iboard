//! Writing `design/tokens.json` into the committed `src/shell/theme/tokens.rs`.
//!
//! The generator runs from `idesign tokens --rust` and from `make tokens`. One
//! test in `theme.rs` fails when the committed file is stale, so a token change
//! is always a visible Rust diff.

use std::fmt::Write;
use std::path::Path;

use super::css::resolved_value;
use super::{DesignSystem, Token};
use crate::kernel::labels::Hue;

const PREFACE: &str = "\
//! Generated from `design/tokens.json`; run `make tokens`, don't edit by hand.

use egui::epaint::Shadow;
use egui::{Color32, FontId};

use super::{MONO_MEDIUM, SANS_MEDIUM, SANS_SEMIBOLD, family, hex};
use crate::kernel::labels::Hue;
";

/// The whole of `src/shell/theme/tokens.rs`, ready to write.
pub fn theme_tokens(system: &DesignSystem) -> Result<String, String> {
    let mut out = String::from(PREFACE);
    palette_struct(&mut out, system)?;
    for (theme, name) in [("light", "LIGHT"), ("dark", "DARK")] {
        palette(&mut out, system, theme, name)?;
    }
    points(&mut out, system, "spacing", "f32")?;
    points(&mut out, system, "radius", "u8")?;
    points(&mut out, system, "size", "f32")?;
    motion(&mut out, system)?;
    type_styles(&mut out, system)?;
    shadow(&mut out, system)?;
    Ok(out)
}

/// Write [`theme_tokens`] to `path`.
pub fn write(system: &DesignSystem, path: &Path) -> Result<(), String> {
    let text = theme_tokens(system)?;
    std::fs::write(path, text).map_err(|err| format!("couldn't write {}: {err}", path.display()))
}

fn doc(out: &mut String, indent: &str, usage: Option<&str>) {
    if let Some(usage) = usage {
        for line in usage.lines() {
            let _ = writeln!(out, "{indent}/// {line}");
        }
    }
}

fn ident(name: &str) -> String {
    name.replace('-', "_")
}

fn shout(name: &str) -> String {
    name.replace('-', "_").to_uppercase()
}

fn number(value: f32) -> String {
    let text = value.to_string();
    if text.contains('.') {
        text
    } else {
        format!("{text}.0")
    }
}

fn length(value: &str) -> Result<f32, String> {
    value
        .trim()
        .trim_end_matches("px")
        .parse()
        .map_err(|_| format!("`{value}` is not a length in pixels"))
}

fn color(value: &str) -> Result<String, String> {
    let value = value.trim();
    if let Some(hex) = value.strip_prefix('#') {
        let rgb =
            u32::from_str_radix(hex, 16).map_err(|_| format!("`{value}` is not a hex colour"))?;
        return Ok(format!("hex(0x{rgb:06x})"));
    }
    let Some(inner) = value
        .strip_prefix("rgba(")
        .or_else(|| value.strip_prefix("rgb("))
    else {
        return Err(format!("`{value}` is not a colour"));
    };
    let inner = inner
        .strip_suffix(')')
        .ok_or_else(|| format!("`{value}` has no closing parenthesis"))?;
    let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
    if parts.len() < 3 {
        return Err(format!("`{value}` needs three colour channels"));
    }
    let channel = |part: &str| {
        part.parse::<f32>()
            .map_err(|_| format!("`{part}` is not a colour channel"))
    };
    let (red, green, blue) = (channel(parts[0])?, channel(parts[1])?, channel(parts[2])?);
    let alpha = match parts.get(3) {
        Some(part) => channel(part)?,
        None => 1.0,
    };
    let premultiply = |c: f32| (c * alpha).round() as u8;
    Ok(format!(
        "Color32::from_rgba_premultiplied({}, {}, {}, {})",
        premultiply(red),
        premultiply(green),
        premultiply(blue),
        (alpha * 255.0).round() as u8
    ))
}

fn value(system: &DesignSystem, token: &Token, theme: &str) -> Result<String, String> {
    resolved_value(system, token, theme)
        .ok_or_else(|| format!("{} has no {theme} value", token.name))
}

fn shadow_color(value: &str) -> Result<String, String> {
    let at = value
        .find("rgba(")
        .or_else(|| value.find("rgb("))
        .or_else(|| value.rfind(char::is_whitespace))
        .ok_or_else(|| format!("`{value}` is not a shadow"))?;
    color(value[at..].trim())
}

fn label_tokens(system: &DesignSystem) -> Result<Vec<&Token>, String> {
    let mut labels = Vec::new();
    for hue in Hue::ALL {
        let name = format!("label-{}", hue.name());
        let token = system
            .token(&name)
            .ok_or_else(|| format!("tokens.json has no {name}"))?;
        labels.push(token);
    }
    for token in &system.tokens.color {
        if token.name.starts_with("label-") && !labels.iter().any(|known| known.name == token.name)
        {
            return Err(format!(
                "{} is not a hue in Hue::ALL, so the palette has nowhere to put it",
                token.name
            ));
        }
    }
    Ok(labels)
}

fn label_tint<'a>(system: &'a DesignSystem, name: &str) -> Result<&'a Token, String> {
    system
        .token(name)
        .ok_or_else(|| format!("tokens.json has no opacity token {name}"))
}

fn palette_struct(out: &mut String, system: &DesignSystem) -> Result<(), String> {
    out.push_str("\n/// One theme's color tokens.\npub struct Palette {\n");
    for token in &system.tokens.color {
        if token.name.starts_with("label-") {
            continue;
        }
        doc(out, "    ", token.usage.as_deref());
        let _ = writeln!(out, "    pub {}: Color32,", ident(&token.name));
    }
    for token in &system.tokens.shadow {
        doc(out, "    ", token.usage.as_deref());
        let _ = writeln!(out, "    pub {}: Color32,", shadow_field(&token.name));
    }
    out.push_str("    /// The `label-*` colours, indexed by [`Hue`].\n");
    out.push_str("    pub labels: [Color32; Hue::ALL.len()],\n");
    doc(
        out,
        "    ",
        label_tint(system, "label-tint")?.usage.as_deref(),
    );
    out.push_str("    pub label_tint: f32,\n}\n");
    Ok(())
}

fn shadow_field(name: &str) -> String {
    let base = name.strip_prefix("shadow-").unwrap_or(name);
    format!("{}_shadow", ident(base))
}

fn palette(out: &mut String, system: &DesignSystem, theme: &str, name: &str) -> Result<(), String> {
    let _ = writeln!(out, "\n/// The {theme} palette.");
    let _ = writeln!(out, "pub const {name}: Palette = Palette {{");
    for token in &system.tokens.color {
        if token.name.starts_with("label-") {
            continue;
        }
        let value = value(system, token, theme)?;
        let _ = writeln!(out, "    {}: {},", ident(&token.name), color(&value)?);
    }
    for token in &system.tokens.shadow {
        let value = value(system, token, theme)?;
        let _ = writeln!(
            out,
            "    {}: {},",
            shadow_field(&token.name),
            shadow_color(&value)?
        );
    }
    out.push_str("    labels: [\n");
    for token in label_tokens(system)? {
        let value = value(system, token, theme)?;
        let _ = writeln!(out, "        {},", color(&value)?);
    }
    let tint = value(
        system,
        label_tint(
            system,
            if theme == "dark" {
                "label-tint-dark"
            } else {
                "label-tint"
            },
        )?,
        theme,
    )?;
    let tint: f32 = tint
        .parse()
        .map_err(|_| format!("label tint `{tint}` is not a number"))?;
    let _ = writeln!(out, "    ],\n    label_tint: {},", number(tint));
    out.push_str("};\n");
    Ok(())
}

fn points(
    out: &mut String,
    system: &DesignSystem,
    section: &str,
    kind: &str,
) -> Result<(), String> {
    let tokens = match section {
        "spacing" => &system.tokens.spacing,
        "radius" => &system.tokens.radius,
        "size" => &system.tokens.size,
        _ => return Err(format!("unknown token section {section}")),
    };
    out.push('\n');
    for token in tokens {
        doc(out, "", token.usage.as_deref());
        let value = value(system, token, "light")?;
        let value = length(&value)?;
        let value = if kind == "u8" {
            (value.round() as u8).to_string()
        } else {
            number(value)
        };
        let _ = writeln!(out, "pub const {}: {kind} = {value};", shout(&token.name));
    }
    Ok(())
}

fn motion(out: &mut String, system: &DesignSystem) -> Result<(), String> {
    out.push('\n');
    for token in &system.tokens.motion {
        doc(out, "", token.usage.as_deref());
        let value = value(system, token, "light")?;
        let millis: f32 = value
            .trim()
            .trim_end_matches("ms")
            .parse()
            .map_err(|_| format!("`{value}` is not a duration in milliseconds"))?;
        let _ = writeln!(
            out,
            "pub const {}: f32 = {};",
            shout(&token.name),
            number(millis / 1000.0)
        );
    }
    Ok(())
}

fn type_styles(out: &mut String, system: &DesignSystem) -> Result<(), String> {
    out.push('\n');
    for group in &system.tokens.type_groups {
        for style in &group.styles {
            doc(out, "", style.usage.as_deref());
            let size = style
                .font_size
                .as_deref()
                .ok_or_else(|| format!("{} has no font size", style.name))?;
            let size = length(size)?;
            let body = font_expr(&group.family, style.weight, size)?;
            let name = ident(&style.name);
            let _ = writeln!(out, "pub fn {name}() -> FontId {{\n    {body}\n}}");
            if let Some(spacing) = &style.letter_spacing {
                let em: f32 = spacing
                    .trim()
                    .trim_end_matches("em")
                    .parse()
                    .map_err(|_| format!("`{spacing}` is not an em value"))?;
                let tracking = (em * size * 1000.0).round() / 1000.0;
                let _ = writeln!(
                    out,
                    "/// Letter spacing of `{}` ({spacing} at {size}pt).\npub const LABEL_TRACKING: f32 = {};",
                    style.name,
                    number(tracking)
                );
            }
        }
    }
    Ok(())
}

fn font_expr(family: &str, weight: Option<u32>, size: f32) -> Result<String, String> {
    let size = number(size);
    match (family, weight) {
        ("sans", Some(400)) => Ok(format!("FontId::proportional({size})")),
        ("sans", Some(500)) => Ok(format!("FontId::new({size}, family(SANS_MEDIUM))")),
        ("sans", Some(600)) => Ok(format!("FontId::new({size}, family(SANS_SEMIBOLD))")),
        ("mono", Some(400)) => Ok(format!("FontId::monospace({size})")),
        ("mono", Some(500)) => Ok(format!("FontId::new({size}, family(MONO_MEDIUM))")),
        (family, weight) => Err(format!("no egui face for {family} weight {weight:?}")),
    }
}

fn shadow(out: &mut String, system: &DesignSystem) -> Result<(), String> {
    out.push('\n');
    let token = system
        .tokens
        .shadow
        .iter()
        .find(|token| token.name == "shadow-popover")
        .ok_or_else(|| "tokens.json has no shadow-popover".to_string())?;
    doc(out, "", token.usage.as_deref());
    let value = value(system, token, "light")?;
    let parts: Vec<&str> = value.split_whitespace().collect();
    if parts.len() < 4 {
        return Err(format!("`{value}` is not a shadow"));
    }
    let length = |part: &str| length(part).map(|value| value.round() as i64);
    let offset = [length(parts[0])?, length(parts[1])?];
    let blur = length(parts[2])?;
    let _ = writeln!(
        out,
        "pub fn popover_shadow(p: &Palette) -> Shadow {{\n    Shadow {{\n        offset: [{}, {}],\n        blur: {},\n        spread: 0,\n        color: p.popover_shadow,\n    }}\n}}",
        offset[0], offset[1], blur
    );
    Ok(())
}
