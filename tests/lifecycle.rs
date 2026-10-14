use primum::lifecycle::{self, parse};
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> (PathBuf, String) {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/lifecycles")
        .join(format!("{name}.toml"));
    let text = std::fs::read_to_string(&p).unwrap();
    (p, text)
}

#[test]
fn the_example_parses() {
    let (p, text) = fixture("stakeholder-update");
    let f = parse(&p, &text).unwrap();
    assert_eq!(f.name, "stakeholder-update");
    assert_eq!(f.start, "notify");
    let ids: Vec<&str> = f.step.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "notify",
            "await-reply",
            "call-api",
            "mark-doing",
            "await-input",
            "report",
            "approve",
            "finish"
        ]
    );
}

#[test]
fn the_built_in_manual_lifecycle_parses() {
    let f = lifecycle::manual();
    assert_eq!(f.name, "manual");
    assert_eq!(f.start, "idle");
    assert_eq!(
        f.states.as_deref().unwrap(),
        ["idle", "doing", "done", "wontfix"]
    );
}

#[test]
fn unknown_keys_are_errors() {
    let e = parse(
        Path::new("lifecycles/x.toml"),
        "name = \"x\"\nstart = \"a\"\nrenk = 1\n[[step]]\nid = \"a\"\nend = true\n",
    )
    .unwrap_err();
    assert!(e.to_string().starts_with("lifecycles/x.toml: "), "{e}");
    assert!(e.to_string().contains("renk"), "{e}");
}

#[test]
fn expressions_templates_and_durations() {
    use primum::lifecycle::expr;
    assert!(expr::compile("output.status >= 200 && output.status < 300").is_ok());
    assert!(expr::compile("input.source == 'http' &&").is_err());
    assert_eq!(
        expr::templates("${needs.api_url}/updates/${ticket.id}").unwrap(),
        ["needs.api_url", "ticket.id"]
    );
    assert!(expr::templates("open ${ticket.id").is_err());
    assert_eq!(
        expr::templates("no templates, ışık").unwrap(),
        Vec::<String>::new()
    );
    assert_eq!(expr::duration("30s").unwrap().as_secs(), 30);
    assert_eq!(expr::duration("7d").unwrap().as_secs(), 7 * 86_400);
    assert_eq!(expr::duration("1h 30m").unwrap().as_secs(), 5_400);
    assert_eq!(expr::duration("2w").unwrap().as_secs(), 14 * 86_400);
    for bad in ["", "7", "-1s", "soon"] {
        assert!(expr::duration(bad).is_err(), "{bad}");
    }
}

#[test]
fn every_broken_file_says_what_is_wrong() {
    let cases = [
        (
            "name-mismatch",
            "name \"other\" must match the file name \"name-mismatch\"",
        ),
        ("start-missing", "start \"nowhere\" is not a step"),
        ("duplicate-id", "step \"a\": the id is used twice"),
        (
            "bad-id",
            "step \"Çağrı\": an id is lowercase letters, digits and -",
        ),
        ("next-missing", "step \"a\": next \"nowhere\" is not a step"),
        (
            "both-do-wait",
            "step \"a\": a step either does something (do) or waits (wait), not both",
        ),
        (
            "nothing-to-do",
            "step \"a\": a step does something (do) or waits (wait); only a closing step (end = true) may do neither",
        ),
        (
            "neither-next-end",
            "step \"a\": a step needs next, or end = true",
        ),
        ("unreachable", "step \"b\": not reachable from start"),
        (
            "no-way-out",
            "step \"a\": no closing step is reachable from here",
        ),
        ("unknown-action", "step \"a\": unknown actionable fax@1"),
        (
            "command-no-run",
            "step \"a\": command@1 needs with.run, a list of strings",
        ),
        ("with-without-do", "step \"a\": with goes with do"),
        (
            "bad-state",
            "step \"a\": state \"dreaming\" is not one of idle, waiting, processing, effects, done, wontfix",
        ),
        ("bad-target", "step \"a\": bad expression"),
        ("bad-template", "step \"a\": unclosed"),
        ("bad-duration", "step \"a\": a duration is like"),
        (
            "bad-wait",
            "step \"a\": wait for \"input\" or \"approval\", not \"email\"",
        ),
    ];
    for (name, want) in cases {
        let p = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("tests/fixtures/lifecycles/broken/{name}.toml"));
        let e = lifecycle::load_file(&p).unwrap_err().to_string();
        assert!(e.contains(want), "{name}: {e}");
        assert!(e.contains(&format!("{name}.toml: ")), "{name}: {e}");
    }
}

#[test]
fn the_example_and_the_manual_lifecycle_are_valid() {
    let (p, _) = fixture("stakeholder-update");
    // approve -> await-input loops back, with a way out
    lifecycle::load_file(&p).unwrap();
    lifecycle::validate::validate(std::path::Path::new("manual.toml"), &lifecycle::manual())
        .unwrap();
}

#[test]
fn versions_follow_steps_not_formatting() {
    let (p, text) = fixture("stakeholder-update");
    let a = lifecycle::load_text(&p, &text).unwrap();
    assert_eq!(a.version.len(), 64);
    assert_eq!(lifecycle::short(&a.version).len(), 12);
    let reformatted = text.replace(" = ", "=").replace("# remind: send again", "");
    let b = lifecycle::load_text(&p, &reformatted).unwrap();
    assert_eq!(a.version, b.version, "comments and spacing do not count");
    let changed = text.replace("timeout = \"7d\"", "timeout = \"8d\"");
    let c = lifecycle::load_text(&p, &changed).unwrap();
    assert_ne!(a.version, c.version);
    assert_ne!(
        a.steps[1].1, c.steps[1].1,
        "await-reply's own version changed"
    );
    assert_eq!(a.steps[0].1, c.steps[0].1, "notify's did not");
    // an actionable's version is part of the step's version
    let mut step = a.file.step[0].clone();
    let before = lifecycle::version::step(&step);
    step.action = Some("command@2".into());
    assert_ne!(lifecycle::version::step(&step), before);
    // the version is stable: the same text, the same hash, every time
    assert_eq!(a.version, lifecycle::load_text(&p, &text).unwrap().version);
}

#[test]
fn states_and_closed_states_of_a_lifecycle() {
    let m = lifecycle::manual();
    assert_eq!(lifecycle::states(&m), ["idle", "doing", "done", "wontfix"]);
    assert_eq!(lifecycle::closed_states(&m), ["done", "wontfix"]);
    assert_eq!(lifecycle::first_state(&m), "idle");
    let (p, text) = fixture("stakeholder-update");
    let f = parse(&p, &text).unwrap();
    assert_eq!(
        lifecycle::states(&f),
        ["effects", "waiting", "processing"],
        "the states its steps name, in order"
    );
    assert_eq!(lifecycle::first_state(&f), "effects");
}
