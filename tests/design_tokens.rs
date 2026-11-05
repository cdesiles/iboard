//! Keeps the Design System type's generated `tokens.css` and `api/tokens.md`
//! in step with `tokens.json`, whatever the order they list things in.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;
use std::path::Path;

use iboard::design::css::resolved_value;
use iboard::design::{DesignSystem, Token, TokenValue, TypeGroup, TypeStyle};

fn design() -> DesignSystem {
    DesignSystem::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("design")).unwrap()
}

fn generated(file: &str) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("design")
            .join(file),
    )
    .unwrap()
}

/// The trimmed lines of the block that opens with `selector {`.
fn block(css: &str, selector: &str) -> BTreeSet<String> {
    let opening = format!("{selector} {{");
    css.lines()
        .skip_while(|line| *line != opening)
        .skip(1)
        .take_while(|line| *line != "}")
        .map(|line| line.trim().to_string())
        .collect()
}

fn assert_same(what: &str, expected: &BTreeSet<String>, found: &BTreeSet<String>) {
    let missing: Vec<_> = expected.difference(found).collect();
    let stale: Vec<_> = found.difference(expected).collect();
    assert!(
        missing.is_empty() && stale.is_empty(),
        "{what} no longer follows tokens.json\nmissing: {missing:#?}\nstale: {stale:#?}"
    );
}

fn declaration(name: &str, value: &str, usage: Option<&str>) -> String {
    let value = value.replace('{', "var(--").replace('}', ")");
    match usage {
        Some(usage) => format!("--{name}: {value}; /* {usage} */"),
        None => format!("--{name}: {value};"),
    }
}

fn themed_block(system: &DesignSystem, theme: &str, usage: bool) -> BTreeSet<String> {
    system
        .tokens
        .all()
        .filter(|token| matches!(token.value, TokenValue::Themed(_)))
        .map(|token| {
            declaration(
                &token.name,
                token.value.theme(theme).unwrap(),
                token.usage.as_deref().filter(|_| usage),
            )
        })
        .collect()
}

fn styles(system: &DesignSystem) -> impl Iterator<Item = (&TypeGroup, &TypeStyle)> {
    system
        .tokens
        .type_groups
        .iter()
        .flat_map(|group| group.styles.iter().map(move |style| (group, style)))
}

fn metrics(style: &TypeStyle) -> (&str, &str, u32) {
    (
        style.font_size.as_deref().unwrap(),
        style.line_height.as_deref().unwrap(),
        style.weight.unwrap(),
    )
}

fn root_block(system: &DesignSystem) -> BTreeSet<String> {
    let tokens = &system.tokens;
    let plain = tokens
        .all()
        .filter_map(|token| match &token.value {
            TokenValue::Plain(value) => {
                Some(declaration(&token.name, value, token.usage.as_deref()))
            }
            TokenValue::Themed(_) => None,
        })
        .collect::<Vec<_>>();
    let families = tokens
        .families
        .iter()
        .map(|(key, stack)| declaration(&format!("font-{key}"), stack, None));
    let shorthands = styles(system).map(|(group, style)| {
        let (size, line, weight) = metrics(style);
        declaration(
            &format!("text-{}", style.name),
            &format!("{weight} {size}/{line} var(--font-{})", group.family),
            style.usage.as_deref(),
        )
    });
    plain
        .into_iter()
        .chain(families)
        .chain(shorthands)
        .collect()
}

#[test]
fn tokens_css_declares_every_token_of_tokens_json() {
    let system = design();
    let css = generated("tokens.css");
    assert_same(
        "the light block of tokens.css",
        &themed_block(&system, "light", true),
        &block(&css, r#":root, [data-theme="light"]"#),
    );
    assert_same(
        "the dark block of tokens.css",
        &themed_block(&system, "dark", false),
        &block(&css, r#"[data-theme="dark"]"#),
    );
    assert_same(
        "the :root block of tokens.css",
        &root_block(&system),
        &block(&css, ":root"),
    );
}

#[test]
fn tokens_css_has_a_class_for_every_type_style() {
    let system = design();
    let css = generated("tokens.css");
    for (group, style) in styles(&system) {
        let (size, line, weight) = metrics(style);
        let spacing = style.letter_spacing.as_deref().unwrap_or("0");
        let expected = [
            format!("font-family: var(--font-{});", group.family),
            format!("font-size: {size};"),
            format!("line-height: {line};"),
            format!("font-weight: {weight};"),
            format!("letter-spacing: {spacing};"),
        ];
        assert_same(
            &format!("the .{} class of tokens.css", style.name),
            &expected.into_iter().collect(),
            &block(&css, &format!(".{}", style.name)),
        );
    }
}

fn row_of<'a>(markdown: &'a str, name: &str) -> &'a str {
    let start = format!("| `{name}` |");
    markdown
        .lines()
        .find(|line| line.starts_with(&start))
        .unwrap_or_else(|| panic!("api/tokens.md has no row for {name}"))
}

/// The interaction states' table lists the values alone, without a usage.
fn usage_shown(token: &Token) -> bool {
    matches!(token.value, TokenValue::Plain(_)) || !token.name.ends_with("-hover")
}

#[test]
fn tokens_md_lists_every_token_with_its_values_and_usage() {
    let system = design();
    let markdown = generated("api/tokens.md");
    for token in system.tokens.all() {
        let row = row_of(&markdown, &format!("--{}", token.name));
        for theme in ["light", "dark"] {
            let value = resolved_value(&system, token, theme).unwrap();
            assert!(row.contains(&value), "{value} is not in `{row}`");
        }
        if usage_shown(token) {
            let usage = token.usage.as_deref().unwrap();
            assert!(row.contains(usage), "{usage} is not in `{row}`");
        }
    }
}

#[test]
fn tokens_md_lists_no_token_gone_from_tokens_json() {
    let system = design();
    let markdown = generated("api/tokens.md");
    let known: BTreeSet<&str> = system
        .tokens
        .all()
        .map(|token| token.name.as_str())
        .collect();
    let stale: Vec<&str> = markdown
        .lines()
        .filter_map(|line| line.strip_prefix("| `--")?.split('`').next())
        .filter(|name| !known.contains(name))
        .collect();
    assert!(stale.is_empty(), "api/tokens.md still lists {stale:?}");
}

#[test]
fn tokens_md_counts_each_section_and_shows_the_real_font_stacks() {
    let system = design();
    let markdown = generated("api/tokens.md");
    let tokens = &system.tokens;
    let sections = [
        ("Spacing", tokens.spacing.len()),
        ("Radius", tokens.radius.len()),
        ("Shadow", tokens.shadow.len()),
        ("Size", tokens.size.len()),
    ];
    for (section, count) in sections {
        assert!(
            markdown.contains(&format!("{section} — {count} token")),
            "the summary should count {count} {section} tokens"
        );
        let after = markdown.split(&format!("## {section}\n")).nth(1).unwrap();
        assert!(
            after.contains(&format!("### Tokens ({count})")),
            "{section} should count {count} tokens"
        );
    }
    for (key, stack) in &tokens.families {
        let row = row_of(&markdown, key);
        assert!(row.contains(stack), "{stack} is not in `{row}`");
    }
    let (group, style) = styles(&system).next().unwrap();
    let (size, line, weight) = metrics(style);
    let stack = &tokens.families[&group.family];
    let example = format!("font: {weight} {size}/{line} {stack};");
    assert!(
        markdown.contains(&example),
        "no `{example}` in the Use block"
    );
    for (group, style) in styles(&system) {
        let (size, line, weight) = metrics(style);
        let row = row_of(&markdown, &style.name);
        let usage = style.usage.as_deref().unwrap();
        for part in [
            format!("{size} / {line}"),
            format!("| {weight} |"),
            format!("| `{}` |", group.family),
            usage.to_string(),
        ] {
            assert!(row.contains(&part), "{part} is not in `{row}`");
        }
    }
}
