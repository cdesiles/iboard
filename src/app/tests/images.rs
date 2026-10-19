//! Images in an issue: pasted in the composer, pasted and dropped in the
//! editor, shown by the reader, and carried through moves, renames,
//! deletes and splits.

use super::*;
use crate::app::images::{Clipboard, with_placeholders};
use crate::assets;

/// A 4×4 screenshot, as PNG bytes.
fn screenshot() -> Vec<u8> {
    let mut png = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(4, 4, image::Rgba([200, 40, 40, 255]))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    png.into_inner()
}

/// `⌘V`, as the window reports it.
fn command_v() -> Vec<egui::Event> {
    vec![
        egui::Event::ModifiersChanged(egui::Modifiers::COMMAND),
        key(Key::V, egui::Modifiers::COMMAND),
    ]
}

/// A file dropped on the window.
#[derive(Debug)]
struct Dropped(PathBuf);

impl egui::DroppedFile for Dropped {
    fn path(&self) -> &Path {
        &self.0
    }

    fn bytes(&self) -> Result<Vec<u8>, String> {
        std::fs::read(&self.0).map_err(|err| err.to_string())
    }
}

/// One frame of `app` with `file` dropped on the window.
fn drop_file(ctx: &egui::Context, app: &mut IboardApp, file: &Path) {
    let input = egui::RawInput {
        screen_rect: Some(window()),
        dropped_files: vec![std::sync::Arc::new(Dropped(file.to_path_buf()))],
        ..Default::default()
    };
    ctx.run_ui(input, |ui| {
        app.frame_logic(ui.ctx());
        app.draw(ui);
    })
    .textures_delta
    .clear();
}

/// The reader shows every image of `path`: nothing reported missing.
fn renders_every_image(ctx: &egui::Context, app: &mut IboardApp, path: &Path) {
    app.error = None;
    app.open_issue(path.to_path_buf());
    frame(ctx, app, vec![]);
    frame(ctx, app, vec![]);
    assert!(
        app.board.find(path).is_some(),
        "{} is on the board",
        path.display()
    );
    assert_eq!(app.error, None, "{} shows its images", path.display());
}

/// A board with `todo/<name>` linking one image in its folder.
fn issue_with_image(tag: &str, name: &str, text: &str) -> (Scratch, PathBuf, IboardApp) {
    let dir = scratch_dir(tag);
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    for column in ["doing", "done"] {
        std::fs::create_dir_all(dir.join("issues").join(column)).unwrap();
    }
    let file = todo.join(name);
    std::fs::write(&file, text).unwrap();
    let root = dir.join("issues");
    assets::add(&root, &reviews::slug(&file), &screenshot()).unwrap();
    let app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    (dir, file, app)
}

#[test]
fn a_screenshot_pasted_in_the_composer_lands_in_the_new_ticket() {
    let dir = scratch_dir("paste-compose");
    let todo = dir.join("issues/todo");
    std::fs::create_dir_all(&todo).unwrap();
    std::fs::write(todo.join("001-older.md"), "# Older\n").unwrap();
    let mut app = IboardApp::from_located(discovery::locate(&dir).unwrap());
    app.clipboard = Clipboard::Held(Some(screenshot()));
    let ctx = themed_ctx();

    app.open_composer();
    frame(&ctx, &mut app, vec![]);
    frame(&ctx, &mut app, command_v());
    frame(&ctx, &mut app, command_v());
    let composer = app.session.composer.as_mut().unwrap();
    assert_eq!(composer.description.matches("![screenshot](").count(), 2);
    composer.title = "Crash on save".to_string();
    app.submit_composer(false);

    let created = todo.join("crash-on-save.md");
    let text = std::fs::read_to_string(&created).unwrap();
    assert!(
        text.contains("![screenshot](../.assets/crash-on-save/01.png)")
            && text.contains("![screenshot](../.assets/crash-on-save/02.png)"),
        "two numbered links: {text}"
    );
    let folders: Vec<String> = std::fs::read_dir(dir.join("issues/.assets"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(folders, ["crash-on-save"], "nothing is left pending");
    let shots = dir.join("issues/.assets/crash-on-save");
    assert!(shots.join("01.png").is_file() && shots.join("02.png").is_file());

    // Renumbered to the front of Todo, the ticket keeps its images.
    frame(&ctx, &mut app, vec![]);
    renders_every_image(&ctx, &mut app, &todo.join("001-crash-on-save.md"));
}

#[test]
fn text_on_the_clipboard_wins_over_an_image() {
    let (dir, file, mut app) = issue_with_image("paste-text", "demo.md", "# Demo\n");
    std::fs::remove_dir_all(dir.join("issues/.assets")).unwrap();
    app.clipboard = Clipboard::Held(Some(screenshot()));
    let ctx = themed_ctx();
    app.open_issue(file.clone());
    app.start_editing(&file);
    app.session.focus_editor = true;
    frame(&ctx, &mut app, vec![]);

    let mut events = command_v();
    events.push(egui::Event::Paste("a repro".to_string()));
    frame(&ctx, &mut app, events);
    let text = &app.session.editor.as_ref().unwrap().text;
    assert!(text.contains("a repro"), "{text}");
    assert!(!text.contains("!["), "{text}");
    assert!(!dir.join("issues/.assets").exists());
}

#[test]
fn images_dropped_on_the_editor_are_numbered_and_linked() {
    let (dir, file, mut app) = issue_with_image("drop", "demo.md", "# Demo\n");
    std::fs::remove_dir_all(dir.join("issues/.assets")).unwrap();
    let shot = dir.join("shot.jpg");
    image::RgbImage::from_pixel(4, 4, image::Rgb([1, 2, 3]))
        .save(&shot)
        .unwrap();
    let ctx = themed_ctx();
    app.open_issue(file.clone());
    app.start_editing(&file);
    frame(&ctx, &mut app, vec![]);

    drop_file(&ctx, &mut app, &shot);
    drop_file(&ctx, &mut app, &shot);
    app.flush_autosave(true);
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(
        text.contains("![screenshot](../.assets/demo/01.png)")
            && text.contains("![screenshot](../.assets/demo/02.png)"),
        "{text}"
    );
    let first = std::fs::read(dir.join("issues/.assets/demo/01.png")).unwrap();
    assert_eq!(
        image::guess_format(&first).unwrap(),
        image::ImageFormat::Png,
        "a JPEG is kept as PNG"
    );

    let gif = dir.join("anim.gif");
    std::fs::write(&gif, b"GIF89a\x01\x00\x01\x00").unwrap();
    drop_file(&ctx, &mut app, &gif);
    assert!(
        app.error
            .as_deref()
            .is_some_and(|e| e.contains("PNG, JPEG and TIFF")),
        "{:?}",
        app.error
    );
    assert!(!dir.join("issues/.assets/demo/03.png").exists());
}

#[test]
fn moving_archiving_and_restoring_keep_every_image() {
    let (dir, file, mut app) = issue_with_image(
        "image-moves",
        "demo.md",
        "# Demo\n\n![s](../.assets/demo/01.png)\n",
    );
    let ctx = themed_ctx();
    renders_every_image(&ctx, &mut app, &file);

    let doing = app.move_issue(&file, &dir.join("issues/doing")).unwrap();
    renders_every_image(&ctx, &mut app, &doing);
    let done = app.move_issue(&doing, &dir.join("issues/done")).unwrap();
    renders_every_image(&ctx, &mut app, &done);

    app.archive_issue(&done);
    let archived = dir.join("issues/.archive/demo.md");
    assert!(archived.is_file());
    let link = &assets::image_links(&std::fs::read_to_string(&archived).unwrap())[0];
    assert!(assets::local_file(&archived, &link.url).unwrap().is_file());

    let restored = dir.join("issues/todo/demo.md");
    std::fs::rename(&archived, &restored).unwrap();
    app.refresh();
    renders_every_image(&ctx, &mut app, &restored);
}

#[test]
fn renaming_moves_the_images_and_leaves_code_and_broken_links() {
    let text = "# Demo\n\n![s](../.assets/demo/01.png)\n\n```\n![s](../.assets/demo/01.png)\n```\n\n![gone](../.assets/demo/09.png)\n";
    let (dir, file, mut app) = issue_with_image("image-rename", "demo.md", text);
    app.start_editing(&file);
    app.session.editor.as_mut().unwrap().name = "shot.md".to_string();
    app.flush_autosave(false);

    let renamed = dir.join("issues/todo/shot.md");
    assert_eq!(
        std::fs::read_to_string(&renamed).unwrap(),
        "# Demo\n\n![s](../.assets/shot/01.png)\n\n```\n![s](../.assets/demo/01.png)\n```\n\n![gone](../.assets/demo/09.png)\n"
    );
    assert!(dir.join("issues/.assets/shot/01.png").is_file());
    assert!(!dir.join("issues/.assets/demo").exists());
    let editor = app.session.editor.as_ref().unwrap();
    assert!(!editor.is_dirty(), "the editor holds what was written");
}

#[test]
fn deleting_a_ticket_trashes_its_images_and_undo_brings_them_back() {
    let (dir, file, mut app) = issue_with_image(
        "image-delete",
        "demo.md",
        "# Demo\n\n![s](../.assets/demo/01.png)\n",
    );
    app.trash = Trash::Folder(dir.join("Trash"));
    app.confirm_delete = false;
    let shots = dir.join("issues/.assets/demo");

    app.request_delete(file.clone());
    assert!(!file.exists() && !shots.exists());
    assert!(dir.join("Trash/files/demo/01.png").is_file());

    app.undo_last();
    assert!(file.is_file() && shots.join("01.png").is_file());
}

#[test]
fn both_halves_of_a_split_keep_working_images() {
    let (dir, file, mut app) = issue_with_image(
        "image-split",
        "demo.md",
        "# Demo\n\n![s](../.assets/demo/01.png)\n",
    );
    let draft = |title: &str| split::SplitDraft {
        title: title.to_string(),
        body: "![s](../.assets/demo/01.png)".to_string(),
        include: true,
    };
    app.split_issue(&file, &[draft("First half"), draft("Second half")]);

    let ctx = themed_ctx();
    for slug in ["first-half", "second-half"] {
        assert!(
            dir.join("issues/.assets")
                .join(slug)
                .join("01.png")
                .is_file()
        );
        let child = dir.join(format!("issues/todo/{slug}.md"));
        let child = if child.exists() {
            child
        } else {
            app.board
                .columns
                .iter()
                .flat_map(|column| &column.issues)
                .find(|issue| reviews::slug(&issue.path) == slug)
                .unwrap()
                .path
                .clone()
        };
        let text = std::fs::read_to_string(&child).unwrap();
        assert!(
            text.contains(&format!("](../.assets/{slug}/01.png)")),
            "{text}"
        );
        renders_every_image(&ctx, &mut app, &child);
    }
    assert!(
        dir.join("issues/.assets/demo/01.png").is_file(),
        "the original keeps its own"
    );
}

#[test]
fn a_missing_image_shows_a_placeholder_and_says_so() {
    let (dir, file, mut app) = issue_with_image(
        "image-missing",
        "demo.md",
        "# Demo\n\n- [ ] see ![s](../.assets/demo/01.png)\n",
    );
    std::fs::remove_file(dir.join("issues/.assets/demo/01.png")).unwrap();
    let ctx = themed_ctx();
    app.open_issue(file.clone());
    frame(&ctx, &mut app, vec![]);
    let error = app.error.clone().unwrap_or_default();
    assert!(
        error.contains("Image missing") && error.contains("01.png"),
        "{error}"
    );

    app.error = None;
    frame(&ctx, &mut app, vec![]);
    assert_eq!(app.error, None, "said once, not every frame");
    assert_eq!(
        with_placeholders(
            "a ![s](../x/01.png) b",
            &assets::image_links("a ![s](../x/01.png) b")
        ),
        "a `image missing: 01.png` b"
    );
}
