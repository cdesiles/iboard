//! Ask AI: a request about an issue, handed to the build agent.

use super::*;

/// `⌘K` opens Ask AI with its boxes at their defaults; Send hands the request
/// to the build agent in the issue's project, whose answer lands in the status
/// bar and the agent log.
#[test]
fn cmd_k_asks_the_agent_about_the_open_issue() {
    let (dir, mut app) = project(
        "ask",
        &[("doing/migrate.md", "# Migrate\n\n- [ ] point 1\n")],
    );
    let file = dir.join("issues/doing/migrate.md");
    app.prefs.build_agent =
        fake_agent("printf 'Did point 1 in %s, as %s' \"$(pwd -P)\" \"$IBOARD_MODE\" # {prompt}");
    app.open_issue(file.clone());
    let ctx = themed_ctx();
    frame(&ctx, &mut app, cmd(Key::K));
    let draft = app.session.ask_draft.as_mut().expect("⌘K opens the dialog");
    assert_eq!(
        draft.leash(),
        agent::Leash {
            edits: true,
            commits: true,
            deploys: false
        }
    );
    draft.request = "Take care of point 1".to_string();
    frame(&ctx, &mut app, cmd(Key::Enter));
    assert!(app.session.ask_draft.is_none() && app.asking(&file));
    assert!(until(|| {
        app.poll_asks();
        !app.asking(&file)
    }));
    let (notice, _) = app.session.notice.clone().expect("the answer shows");
    assert!(
        notice.contains(&format!("Did point 1 in {}, as ask", dir.display())),
        "{notice}"
    );
    assert!(app.has_agent_log(&file));
}

/// Bug: a renumber in Todo left the Ask dialog and the ask at work on the
/// issue's old name: Send found no issue, and the answer missed the reader.
#[test]
fn an_ask_follows_its_issue_through_a_renumber() {
    let (dir, mut app) = project(
        "ask-renumber",
        &[
            ("todo/001-a.md", "# Issue\n"),
            ("todo/002-b.md", "# Issue\n"),
            ("todo/003-c.md", "# Issue\n"),
        ],
    );
    let todo = dir.join("issues/todo");
    app.prefs.build_agent = fake_agent("printf done # {prompt}");
    let issue = |name: &str| todo.join(name);
    app.open_issue(issue("001-a.md"));
    app.toggle_ask(&issue("001-a.md"));
    app.session.ask_draft.as_mut().unwrap().request = "Take care of it".to_string();
    app.place_issue(&issue("002-b.md"), &todo, Some(issue("001-a.md")));
    app.send_ask();
    assert!(
        app.asking(&issue("002-a.md")),
        "Send asks under the new name"
    );

    app.place_issue(&issue("003-c.md"), &todo, Some(issue("002-a.md")));
    let a = issue("003-a.md");
    assert!(app.asking(&a), "the ask follows the issue");
    assert!(until(|| {
        app.poll_asks();
        !app.asking(&a)
    }));
    assert!(
        app.agent_log_open(&a),
        "its answer opens the agent log beside it"
    );
}
