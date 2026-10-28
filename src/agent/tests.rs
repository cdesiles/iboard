use super::*;

#[test]
fn verdicts_survive_markdown_and_punctuation() {
    let answer = "**RELEVANCE:** Pass\nStill needed.\n\
                  ## Size: **Challenge** — split it\nIt does two things.\n\
                  - CLARITY: Block.\nWhich tool?\n\
                  **PROPOSAL:**\n# Better\n";
    let report = parse(answer).expect("a report");
    let verdicts: Vec<(&str, Verdict)> = report
        .findings
        .iter()
        .map(|finding| (finding.axis.as_str(), finding.verdict))
        .collect();
    assert_eq!(
        verdicts,
        [
            ("Relevance", Verdict::Pass),
            ("Size", Verdict::Challenge),
            ("Clarity", Verdict::Block)
        ]
    );
    assert_eq!(report.findings[1].body, "split it\nIt does two things.");
    assert_eq!(report.proposal.as_deref(), Some("# Better"));
}

#[test]
fn a_line_that_only_looks_like_a_finding_stays_text() {
    let answer = "RELEVANCE: Pass\nSize: the issue is large\nSIZE: Pass\nok\nCLARITY: Pass\nok\nPROPOSAL: NONE";
    let report = parse(answer).expect("a report");
    assert_eq!(report.findings[0].body, "Size: the issue is large");
    assert_eq!(report.proposal, None);
    let echoed = crate::prompts::review_prompt("# An issue", ReviewMode::Quick, &[]);
    assert!(parse(&echoed).is_none(), "an agent repeating the prompt");
}

fn custom(command: &str) -> AgentConfig {
    AgentConfig {
        tool: Tool::Custom,
        model: String::new(),
        command: command.into(),
        configured: true,
        ..AgentConfig::default()
    }
}

fn args(command: &Command) -> Vec<String> {
    command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect()
}

#[test]
fn a_custom_command_reads_the_prompt_file_and_answers() {
    let job = start(
        &custom("cat {prompt}"),
        "hello there",
        Path::new("todo/a.md"),
        None,
    );
    let answer = job.wait().expect("the job answers").expect("cat succeeds");
    assert_eq!(answer, "hello there");
}

#[test]
fn a_failing_command_reports_its_error_instead_of_panicking() {
    let config = custom("echo nope >&2; exit 3 # {prompt}");
    let error = run(&config, "x", Path::new("a.md"), None, &Cancel::default());
    assert_eq!(error, Err("nope".to_string()));
}

#[test]
fn an_agent_runs_from_its_project() {
    let scratch = tempfile::tempdir().unwrap();
    let project = std::fs::canonicalize(scratch.path()).unwrap();
    let config = custom("pwd -P # {prompt}");
    let answer = run(
        &config,
        "x",
        Path::new("a.md"),
        Some(&project),
        &Cancel::default(),
    );
    assert_eq!(answer, Ok(project.display().to_string()));
}

#[test]
fn file_names_never_run_as_shell_code() {
    let scratch = tempfile::tempdir().unwrap();
    let dir = scratch.path();
    let planted = dir.join("planted");
    let issue = dir.join(format!(
        "$(touch {}) `touch {}`.md",
        planted.display(),
        planted.display()
    ));
    for template in [
        "printf %s {issue} # {prompt}",
        "printf %s \"{issue}\" # {prompt}",
        "printf %s '{issue}' # {prompt}",
    ] {
        let answer = run(&custom(template), "x", &issue, None, &Cancel::default());
        assert_eq!(answer, Ok(issue.display().to_string()), "{template}");
    }
    assert!(!planted.exists(), "a file name ran as code");
}

#[cfg(unix)]
#[test]
fn dropping_a_run_stops_the_agent() {
    // SAFETY: signal 0 only checks that the process exists.
    let alive = |pid: i32| unsafe { libc::kill(pid, 0) == 0 };
    let eventually = |done: &dyn Fn() -> bool| {
        let start = std::time::Instant::now();
        while !done() && start.elapsed() < std::time::Duration::from_secs(5) {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        done()
    };
    let scratch = tempfile::tempdir().unwrap();
    let dir = scratch.path();
    let pid_file = dir.join("agent.pid");
    let script = format!(
        "echo $$ > '{}'; sleep 30; echo late # {{prompt}}",
        pid_file.display()
    );
    let job = start(&custom(&script), "x", Path::new("a.md"), None);
    let pid = || {
        std::fs::read_to_string(&pid_file)
            .ok()
            .and_then(|text| text.trim().parse::<i32>().ok())
    };
    assert!(eventually(&|| pid().is_some()), "the agent never started");
    let pid = pid().unwrap();
    assert!(alive(pid));

    drop(job);

    assert!(eventually(&|| !alive(pid)), "the agent outlived its job");
}

#[test]
fn each_tool_gets_its_one_shot_arguments() {
    let file = Path::new("/tmp/prompt.md");
    let issue = Path::new("/p/todo/a b.md");
    let opencode = AgentConfig {
        tool: Tool::OpenCode,
        ..AgentConfig::default()
    };
    let built = command(
        &opencode,
        "vendor/model",
        "PROMPT",
        file,
        issue,
        Purpose::Answer,
    );
    assert_eq!(args(&built), ["run", "--model", "vendor/model", "PROMPT"]);
    let built = command(
        &opencode,
        "",
        "PROMPT",
        file,
        issue,
        Purpose::Review {
            mode: ReviewMode::Quick,
            plan: false,
        },
    );
    assert_eq!(args(&built), ["run", "--format", "json", "PROMPT"]);

    let claude = AgentConfig {
        tool: Tool::Claude,
        ..AgentConfig::default()
    };
    let built = command(&claude, "", "PROMPT", file, issue, Purpose::Answer);
    assert_eq!(args(&built), ["-p", "PROMPT"]);
    let stream = ["-p", "--output-format", "stream-json", "--verbose"];
    let quick = Purpose::Review {
        mode: ReviewMode::Quick,
        plan: false,
    };
    let built = command(&claude, "", "PROMPT", file, issue, quick);
    assert_eq!(
        args(&built),
        [&stream[..], &["--tools", "", "PROMPT"]].concat()
    );
    let deep = Purpose::Review {
        mode: ReviewMode::Deep,
        plan: false,
    };
    let built = command(&claude, "", "PROMPT", file, issue, deep);
    assert_eq!(args(&built), [&stream[..], &["PROMPT"]].concat());
    let built = command(&claude, "", "PROMPT", file, issue, Purpose::Build);
    assert_eq!(
        args(&built),
        [&stream[..], &["--permission-mode", "acceptEdits", "PROMPT"]].concat()
    );

    let built = command(
        &custom("llm -m {model} < {prompt} # {issue}"),
        "gpt",
        "PROMPT",
        file,
        issue,
        Purpose::Review {
            mode: ReviewMode::Deep,
            plan: true,
        },
    );
    assert_eq!(built.get_program(), "sh");
    assert_eq!(
        args(&built),
        [
            "-c",
            "llm -m \"$IBOARD_MODEL\" < \"$IBOARD_PROMPT\" # \"$IBOARD_ISSUE\""
        ]
    );
    let env = |name: &str| {
        built
            .get_envs()
            .find(|(key, _)| *key == name)
            .and_then(|(_, value)| value)
            .map(|value| value.to_string_lossy().into_owned())
    };
    assert_eq!(env("IBOARD_MODEL").as_deref(), Some("gpt"));
    assert_eq!(env("IBOARD_PROMPT").as_deref(), Some("/tmp/prompt.md"));
    assert_eq!(env("IBOARD_ISSUE").as_deref(), Some("/p/todo/a b.md"));
    assert_eq!(env("IBOARD_MODE").as_deref(), Some("deep"));
}

/// Events recorded from `opencode run --format json` and
/// `claude -p --output-format stream-json --verbose`, trimmed.
const OPENCODE: &str = r#"{"type":"step_start","part":{"type":"step-start"}}
{"type":"text","part":{"type":"text","text":"Let me look."}}
{"type":"tool_use","part":{"type":"tool","tool":"read","state":{"status":"completed","input":{"path":"/p/src/app/review.rs"}}}}
{"type":"step_finish","part":{"type":"step-finish","reason":"tool-calls"}}
{"type":"step_start","part":{"type":"step-start"}}
{"type":"text","part":{"type":"text","text":"RELEVANCE: Pass"}}"#;
const CLAUDE: &str = r#"{"type":"system","subtype":"init"}
{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Grep","input":{"pattern":"fn review"}}]}}
{"type":"user","message":{"content":[{"type":"tool_result"}]}}
{"type":"assistant","message":{"content":[{"type":"text","text":"RELEVANCE: Pass"}]}}
{"type":"result","subtype":"success","result":"RELEVANCE: Pass"}"#;

#[test]
fn streamed_events_give_steps_and_the_answer() {
    let steps: Vec<String> = OPENCODE
        .lines()
        .chain(CLAUDE.lines())
        .filter_map(step_of)
        .collect();
    assert_eq!(steps, ["Reading app/review.rs…", "Searching fn review…"]);
    assert_eq!(final_answer(OPENCODE), "RELEVANCE: Pass");
    assert_eq!(final_answer(CLAUDE), "RELEVANCE: Pass");
    assert_eq!(final_answer("plain answer\n"), "plain answer\n");
}

#[test]
fn an_agent_failing_in_its_stream_reports_its_own_error() {
    let opencode = r#"{"type":"error","error":{"type":"provider.rate-limit","message":"[anthropic-auth category=x; [connection 1]; request-id=r] Fast mode needs usage credits."}}"#;
    assert_eq!(
        stream_error(opencode).as_deref(),
        Some("Fast mode needs usage credits.")
    );
    let claude = r#"{"type":"result","subtype":"error_during_execution","is_error":true,"result":"Invalid model"}"#;
    assert_eq!(stream_error(claude).as_deref(), Some("Invalid model"));
    assert_eq!(stream_error(CLAUDE), None);
    assert_eq!(stream_error(OPENCODE), None);
    let config = custom(
        "printf '%s\\n' '{\"type\":\"error\",\"error\":{\"message\":\"no credits\"}}'; exit 1 # {prompt}",
    );
    let error = run(&config, "x", Path::new("a.md"), None, &Cancel::default());
    assert_eq!(error, Err("no credits".to_string()));
}

#[test]
fn a_review_reports_its_steps_and_answers() {
    let config = AgentConfig {
        tool: Tool::Custom,
        command: "printf %s \"$IBOARD_MODE\" # {prompt}".into(),
        configured: true,
        ..AgentConfig::default()
    };
    let (job, progress) = start_review(&config, "x", Path::new("a.md"), None, ReviewMode::Deep);
    assert_eq!(job.wait().unwrap(), Ok("deep".to_string()));
    assert_eq!(step(&progress), None, "a custom command reports no steps");
}

/// The fake `opencode` and `claude`: each answers with its arguments, one per
/// line, and rejects plan mode when the prompt says `NO-PLAN`.
pub(crate) const FAKES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fakes");

fn tool(tool: Tool) -> AgentConfig {
    AgentConfig {
        tool,
        model: "m".into(),
        configured: true,
        ..AgentConfig::default()
    }
}

fn reviewed(config: &AgentConfig, prompt: &str, mode: ReviewMode) -> Reviewed {
    let (job, _) = start_plan_review(config, prompt, Path::new("a.md"), None, mode);
    job.wait().unwrap().unwrap()
}

#[test]
fn opencode_and_claude_review_in_plan_mode() {
    let answer = reviewed(&tool(Tool::OpenCode), "PROMPT", ReviewMode::Deep);
    assert!(answer.plan);
    assert_eq!(
        answer.answer,
        "run\n--format\njson\n--agent\nplan\n--model\nm\nPROMPT"
    );
    let answer = reviewed(&tool(Tool::Claude), "PROMPT", ReviewMode::Quick);
    assert!(answer.plan);
    assert_eq!(
        answer.answer,
        "-p\n--output-format\nstream-json\n--verbose\n--tools\n\n\
         --permission-mode\nplan\n--model\nm\nPROMPT"
    );
    let answer = run(
        &tool(Tool::Claude),
        "PROMPT",
        Path::new("a.md"),
        None,
        &Cancel::default(),
    );
    let answer = answer.unwrap();
    assert_eq!(answer, "-p\n--model\nm\nPROMPT", "only reviews plan");
}

#[test]
fn a_rejected_plan_flag_falls_back_to_the_plain_review() {
    for agent in [Tool::OpenCode, Tool::Claude] {
        let answer = reviewed(&tool(agent), "NO-PLAN", ReviewMode::Deep);
        assert!(!answer.plan, "{agent:?}");
        assert!(!answer.answer.contains("plan\n"), "{}", answer.answer);
        assert!(answer.answer.ends_with("--model\nm\nNO-PLAN"));
    }
    assert!(rejects_plan("error: unknown option '--permission-mode'"));
    assert!(rejects_plan("Error: Agent plan not found"));
    assert!(!rejects_plan("Rate limit reached: upgrade your plan"));
}

#[test]
fn an_agent_without_plan_mode_reviews_as_before() {
    let config = custom("printf '%s|%s' \"$IBOARD_MODE\" \"$*\" # {prompt}");
    let answer = reviewed(&config, "NO-PLAN", ReviewMode::Deep);
    assert_eq!(
        answer,
        Reviewed {
            answer: "deep|".into(),
            plan: false
        }
    );
}

#[test]
fn a_custom_command_without_prompt_does_not_validate() {
    assert!(custom("llm -m {model}").validate().is_err());
    assert!(custom("llm < {prompt}").validate().is_ok());
    assert!(AgentConfig::default().validate().is_ok());
}

#[test]
fn the_label_names_the_tool_and_model() {
    let mut config = AgentConfig {
        tool: Tool::OpenCode,
        ..AgentConfig::default()
    };
    assert_eq!(config.label(), "opencode · default");
    config.model = "vendor/model".into();
    assert_eq!(config.label(), "opencode · vendor/model");
}

#[test]
fn a_tool_iboard_does_not_run_is_reported() {
    let read = |text: &str| {
        let mut config = UserConfig::parse(text, Path::new("iboard.toml"));
        (load(&mut config), config.problems)
    };
    let (agent, problems) = read("[ai]\ntool = \"claude\"\nmodel = \"Default\"\n");
    assert_eq!(agent.tool, Tool::Claude);
    assert!(agent.configured() && agent.model.is_empty() && problems.is_empty());

    let (agent, problems) = read("[ai]\ntool = \"/usr/local/bin/claude\"\n");
    assert!(!agent.configured());
    assert_eq!(problems.len(), 1);

    let (agent, problems) = read("");
    assert!(!agent.configured() && problems.is_empty());
}

#[test]
fn the_review_scope_defaults_to_the_ticket_alone() {
    let read = |text: &str| load(&mut UserConfig::parse(text, Path::new("iboard.toml"))).scope;
    assert_eq!(read("[ai]\ntool = \"claude\"\n"), ReviewMode::Quick);
    assert_eq!(read(""), ReviewMode::Quick);
    assert_eq!(
        read("[ai]\ntool = \"claude\"\nscope = \"ticket\"\n"),
        ReviewMode::Quick
    );
    let project = "[ai]\ntool = \"claude\"\nscope = \"project\"\n";
    assert_eq!(read(project), ReviewMode::Deep);
}

#[test]
fn only_the_review_section_stores_the_scope() {
    let config = AgentConfig {
        tool: Tool::Claude,
        configured: true,
        scope: ReviewMode::Deep,
        ..AgentConfig::default()
    };
    let scratch = tempfile::tempdir().unwrap();
    let file = scratch.path().join("iboard.toml");
    let sections = [section(&config).unwrap(), build_section(&config).unwrap()];
    settings::save(&file, &sections).unwrap();
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "[ai]\ntool = \"claude\"\nmodel = \"default\"\nscope = \"project\"\n\n\
         [build]\ntool = \"claude\"\nmodel = \"default\"\n"
    );
}

#[test]
fn the_build_agent_falls_back_to_the_review_agent() {
    let read = |text: &str| {
        let mut config = UserConfig::parse(text, Path::new("iboard.toml"));
        let review = load(&mut config);
        load_build(&mut config, &review)
    };
    let review = "[ai]\ntool = \"claude\"\nmodel = \"opus\"\n";
    let build = read(review);
    assert_eq!((build.tool, build.model.as_str()), (Tool::Claude, "opus"));
    let build = read(&format!("{review}[build]\ntool = \"opencode\"\n"));
    assert_eq!(build.tool, Tool::OpenCode);
    assert!(build.configured() && build.model.is_empty());
    assert!(!read("").configured());
}

#[test]
fn versions_and_model_lists_parse() {
    assert_eq!(
        parse_version("opencode v2.0.20\n").as_deref(),
        Some("v2.0.20")
    );
    assert_eq!(
        parse_version("2.1.287 (Claude Code)").as_deref(),
        Some("v2.1.287")
    );
    assert_eq!(parse_version("no version here"), None);

    assert_eq!(
        parse_model_list("Models available:\nanthropic/a\n\n  openai/b \n"),
        ["anthropic/a", "openai/b"]
    );
    assert_eq!(
        parse_opencode_favorites(
            r#"{"recent":[],"favorite":[{"providerID":"deepseek","modelID":"flash"}]}"#
        ),
        ["deepseek/flash"]
    );
    assert!(parse_opencode_favorites("not json").is_empty());
    assert_eq!(
        parse_claude_model(r#"{"model": "opus"}"#).as_deref(),
        Some("opus")
    );
    assert_eq!(parse_claude_model("{}"), None);
}

#[test]
fn a_full_answer_parses_into_three_findings_and_a_proposal() {
    let answer = "\
RELEVANCE: Pass
Still needed, nothing else covers it.
SIZE: Challenge
Two features in one. Split them.
CLARITY: Block
An implementer would ask which tool runs it.
PROPOSAL:
# Better issue

Some context.";
    let report = parse(answer).expect("the shape is complete");
    assert_eq!(report.findings.len(), 3);
    assert_eq!(report.findings[0].axis, "Relevance");
    assert_eq!(report.findings[0].verdict, Verdict::Pass);
    assert_eq!(report.findings[1].verdict, Verdict::Challenge);
    assert_eq!(report.findings[2].verdict, Verdict::Block);
    assert!(report.findings[2].body.contains("which tool"));
    assert!(
        report
            .proposal
            .as_deref()
            .unwrap()
            .starts_with("# Better issue")
    );
}

#[test]
fn a_split_answer_parses_into_tickets() {
    let answer = "Here you go.\n=== TICKET ===\n# [refactor] Split design.rs\n\nInto model and load.\n\
                  === TICKET ===\n# [refactor] Split widgets.rs\nInto button and menu.\n";
    let drafts = parse_split(answer).expect("two tickets");
    assert_eq!(drafts[0].title, "[refactor] Split design.rs");
    assert_eq!(drafts[0].body, "Into model and load.");
    assert_eq!(drafts[1].body, "Into button and menu.");
    assert!(parse_split("=== TICKET ===\n# Only one\n").is_none());
    assert!(parse_split("=== TICKET ===\n# A\n=== TICKET ===\nno title\n").is_none());
    assert!(parse_split("prose").is_none());
}

#[test]
fn a_refinement_turn_parts_the_challenge_from_the_proposal() {
    let turn = parse_refinement("Who asked for it?\n\n**PROPOSAL:** NONE\n");
    assert_eq!(turn.text, "Who asked for it?");
    assert_eq!(turn.proposal, None);
    let turn = parse_refinement("Settled.\nPROPOSAL:\n# [feat] Refine\n\nThe spec.");
    assert_eq!(turn.text, "Settled.");
    assert_eq!(
        turn.proposal.as_deref(),
        Some("# [feat] Refine\n\nThe spec.")
    );
    let turn = parse_refinement(" Just prose. ");
    assert_eq!((turn.text.as_str(), turn.tldr), ("Just prose.", None));
}

#[test]
fn a_refinement_turn_opens_on_its_tldr_and_ends_on_options() {
    let turn = parse_refinement(
        "**TL;DR:** Ship the smaller fix.\nDrop the redesign.\n\n## Why\n\nIt costs a week.\n\
         OPTIONS:\n1. Keep the fix only\n- **Both**, later\nNONE\nPROPOSAL: NONE",
    );
    assert_eq!(
        turn.tldr.as_deref(),
        Some("Ship the smaller fix. Drop the redesign.")
    );
    assert_eq!(turn.text, "## Why\n\nIt costs a week.");
    assert_eq!(turn.options, ["Keep the fix only", "**Both**, later"]);
    assert_eq!(turn.proposal, None);
}

#[test]
fn a_choice_splits_into_its_title_and_detail() {
    assert_eq!(
        choice_parts("**Split it**: four tickets, one each."),
        ("Split it".to_string(), Some("four tickets, one each."))
    );
    assert_eq!(
        choice_parts("Narrow it — keep the CTA: drop the rest"),
        ("Narrow it".to_string(), Some("keep the CTA: drop the rest"))
    );
    assert_eq!(
        choice_parts("Keep the fix only"),
        ("Keep the fix only".to_string(), None)
    );
}

#[test]
fn a_none_proposal_is_dropped() {
    let answer = "RELEVANCE: Pass\nok\nSIZE: Pass\nok\nCLARITY: Pass\nok\nPROPOSAL:\nnone";
    let report = parse(answer).unwrap();
    assert!(report.proposal.is_none());
}

#[test]
fn free_form_answers_do_not_parse() {
    assert!(parse("This is just prose the agent wrote.").is_none());
    let partial = "RELEVANCE: Pass\nok\nSIZE: Pass\nok";
    assert!(parse(partial).is_none());
}

#[test]
fn the_configuration_round_trips_through_a_file() {
    let scratch = tempfile::tempdir().unwrap();
    let dir = scratch.path();
    let file = dir.join("iboard.toml");
    std::fs::write(&file, "[other]\nkey = 1\n").unwrap();

    let config = AgentConfig {
        tool: Tool::OpenCode,
        model: "vendor/model".into(),
        command: String::new(),
        configured: true,
        scope: ReviewMode::Deep,
    };
    settings::save(&file, &[section(&config).unwrap()]).unwrap();
    assert_eq!(load(&mut UserConfig::load(Some(&file))), config);
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("[other]"), "the other section survives");
    assert!(text.contains("tool = \"opencode\""));

    // A missing file is simply an unconfigured agent.
    let absent = dir.join("absent.toml");
    assert!(!load(&mut UserConfig::load(Some(&absent))).configured());
}

#[test]
fn direnv_wraps_the_command_and_keeps_its_environment() {
    use std::ffi::OsStr;
    let mut command = std::process::Command::new("claude");
    command
        .args(["-p", "x"])
        .env("PATH", "/bin")
        .current_dir("/tmp");
    let project = Path::new("/code/app");
    let wrapped = direnv::wrap(&command, Path::new("/opt/direnv"), project);
    assert_eq!(wrapped.get_program(), "/opt/direnv");
    let args: Vec<_> = wrapped.get_args().collect();
    assert_eq!(args, ["exec", "/code/app", "claude", "-p", "x"]);
    let envs: Vec<_> = wrapped.get_envs().collect();
    assert_eq!(envs, [(OsStr::new("PATH"), Some(OsStr::new("/bin")))]);
    assert_eq!(wrapped.get_current_dir(), Some(Path::new("/tmp")));
}

#[test]
fn direnv_applies_only_when_on_and_the_project_has_an_envrc() {
    let scratch = tempfile::tempdir().unwrap();
    let project = scratch.path();
    assert!(!direnv::applies(true, project));
    std::fs::write(project.join(".envrc"), "export A=1\n").unwrap();
    assert!(direnv::applies(true, project));
    assert!(!direnv::applies(false, project));
}
