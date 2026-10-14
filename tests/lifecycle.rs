use primum::lifecycle::definitions::{self, Definition};
use primum::lifecycle::{self, parse, validate};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn example_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lifecycles/enrich-ticket.json")
}

/// The example without its project actionable, for tests that use no
/// definitions.
fn example_without_project_actionables() -> String {
    std::fs::read_to_string(example_path()).unwrap().replace(
        r#"{ "action": "checks.is-valid@1", "with": { "ticket": "${ticket}" },
          "expect": "output.ok" }"#,
        r#"{ "action": "command@1", "with": { "run": ["./check.sh", "${ticket.id}"] } }"#,
    )
}

fn small() -> Value {
    json!({
        "name": "akış",
        "states": ["a", "b", "c"],
        "start": "a",
        "end": ["c"],
        "transitions": [
            { "from": "a", "on": "ticket.created", "to": "b" },
            { "from": "b", "to": "c", "timeout": "5m", "on_timeout": "c",
              "do": [ { "action": "http.request@1",
                        "with": { "url": "https://example.com/${ticket.id}" },
                        "expect": "output.status == 200" } ] }
        ]
    })
}

fn check(v: &Value) -> Result<lifecycle::Lifecycle, String> {
    lifecycle::load_text(Path::new("lifecycles/akış.json"), &v.to_string(), &[])
        .map_err(|e| e.to_string())
}

#[test]
fn the_example_parses() {
    let f = parse(&example_path(), &example_without_project_actionables()).unwrap();
    assert_eq!(f.name, "enrich-ticket");
    assert_eq!(f.start, "create");
    assert_eq!(f.end, ["finished", "timed-out"]);
    let moves: Vec<(&str, &str)> = f
        .transitions
        .iter()
        .map(|t| (t.from.as_str(), t.to.as_str()))
        .collect();
    assert_eq!(
        moves,
        [
            ("create", "idle"),
            ("idle", "processing"),
            ("processing", "finished")
        ]
    );
    assert_eq!(f.transitions[1].showing.as_deref(), Some("waiting"));
    assert_eq!(f.transitions[2].actions[0].action, "json.pick@1");
}

#[test]
fn an_action_is_a_name_or_an_object() {
    let mut v = small();
    v["transitions"][1]["do"] = json!(["json.pick@1"]);
    let e = check(&v).unwrap_err();
    assert!(e.contains("json.pick@1 needs with.path"), "{e}");
    v["transitions"][1]["do"] = json!([{ "action": "json.pick@1", "with": { "path": "a" } }]);
    check(&v).unwrap();
    v["transitions"][1]["do"] = json!([12]);
    assert!(
        check(&v).unwrap_err().contains("an action is a name"),
        "a number is neither"
    );
}

#[test]
fn the_built_in_manual_lifecycle() {
    let f = lifecycle::manual();
    assert_eq!(f.name, "manual");
    assert_eq!(f.states, ["idle", "doing", "done", "wontfix"]);
    assert_eq!(f.start, "idle");
    assert_eq!(f.end, ["done", "wontfix"]);
    assert!(f.transitions.is_empty(), "a person moves it");
    assert_eq!(lifecycle::states(&f), ["idle", "doing", "done", "wontfix"]);
    assert_eq!(lifecycle::closed_states(&f), ["done", "wontfix"]);
    assert_eq!(lifecycle::first_state(&f), "idle");
    assert_eq!(
        lifecycle::Rules::of(&f).states,
        ["idle", "doing", "done", "wontfix"]
    );
}

#[test]
fn a_lifecycle_that_runs_can_also_be_failed() {
    let l = check(&small()).unwrap();
    let r = lifecycle::Rules::of(&l.file);
    assert_eq!(r.states, ["a", "b", "c", "failed"]);
    assert_eq!(r.closed, ["c"]);
}

#[test]
fn expressions_templates_and_durations() {
    use primum::lifecycle::expr;
    assert!(expr::compile("output.status >= 200 && output.status < 300").is_ok());
    assert!(expr::compile("input.source == 'http' &&").is_err());
    assert_eq!(
        expr::templates("${project.api_url}/updates/${ticket.id}").unwrap(),
        ["project.api_url", "ticket.id"]
    );
    assert!(expr::templates("open ${ticket.id").is_err());
    assert_eq!(
        expr::templates("no templates, ışık").unwrap(),
        Vec::<String>::new()
    );
    assert_eq!(expr::duration("30s").unwrap().as_secs(), 30);
    assert_eq!(expr::duration("1h 30m").unwrap().as_secs(), 5_400);
    for bad in ["", "7", "-1s", "soon"] {
        assert!(expr::duration(bad).is_err(), "{bad}");
    }
}

#[test]
fn every_broken_lifecycle_says_what_is_wrong() {
    type Fault = fn(&mut Value);
    let cases: Vec<(Fault, &str)> = vec![
        (
            |v| v["name"] = json!("başka"),
            "name \"başka\" must match the file name \"akış\"",
        ),
        (|v| v["states"] = json!([]), "states is empty"),
        (
            |v| v["states"] = json!(["a", "B", "b", "c"]),
            "state \"B\" is lowercase letters, digits and -",
        ),
        (
            |v| v["states"] = json!(["a", "a", "b", "c"]),
            "state \"a\" is listed twice",
        ),
        (
            |v| v["states"] = json!(["a", "b", "c", "failed"]),
            "\"failed\" is the engine's own state; do not list it",
        ),
        (|v| v["start"] = json!("x"), "start \"x\" is not a state"),
        (|v| v["end"] = json!(["x"]), "end \"x\" is not a state"),
        (|v| v["renk"] = json!(1), "unknown field `renk`"),
        (
            |v| v["transitions"][0]["to"] = json!("x"),
            "transition \"a -> x\": to \"x\" is not a state",
        ),
        (
            |v| v["transitions"][0]["from"] = json!("x"),
            "transition \"x -> b\": from \"x\" is not a state",
        ),
        (
            |v| v["transitions"][1]["while"] = json!("x"),
            "transition \"b -> c\": while \"x\" is not a state",
        ),
        (
            |v| v["transitions"][1]["on_fail"] = json!("x"),
            "transition \"b -> c\": on_fail \"x\" is not a state",
        ),
        (
            |v| v["transitions"][1]["on_timeout"] = json!("x"),
            "transition \"b -> c\": on_timeout \"x\" is not a state",
        ),
        (
            |v| {
                v["transitions"][0].as_object_mut().unwrap().remove("on");
            },
            "transition \"a -> b\": a transition needs on, do, or both",
        ),
        (
            |v| v["transitions"][0]["on"] = json!("Ticket Created"),
            "transition \"a -> b\": on \"Ticket Created\" is an event type, like ticket.created",
        ),
        (
            |v| v["transitions"][1]["timeout"] = json!("soon"),
            "transition \"b -> c\": a duration is like",
        ),
        (
            |v| {
                v["transitions"][1]
                    .as_object_mut()
                    .unwrap()
                    .remove("on_timeout");
            },
            "transition \"b -> c\": timeout needs on_timeout",
        ),
        (
            |v| {
                v["transitions"][1]
                    .as_object_mut()
                    .unwrap()
                    .remove("timeout");
            },
            "transition \"b -> c\": on_timeout needs timeout",
        ),
        (
            |v| v["transitions"][0]["when"] = json!("input.x &&"),
            "transition \"a -> b\": bad expression",
        ),
        (
            |v| v["transitions"][1]["do"][0]["action"] = json!("fax@1"),
            "transition \"b -> c\": unknown actionable fax@1",
        ),
        (
            |v| v["transitions"][1]["do"][0]["with"] = json!({}),
            "transition \"b -> c\": http.request@1 needs with.url",
        ),
        (
            |v| v["transitions"][1]["do"][0]["with"]["renk"] = json!(1),
            "transition \"b -> c\": http.request@1 takes no with.renk",
        ),
        (
            |v| v["transitions"][1]["do"][0]["with"]["url"] = json!("https://e.com/${ticket.id"),
            "transition \"b -> c\": unclosed",
        ),
        (
            |v| v["transitions"][1]["do"][0]["expect"] = json!("output.status =="),
            "transition \"b -> c\": bad expression",
        ),
        (
            |v| v["transitions"][1]["do"][0]["renk"] = json!(1),
            "unknown field `renk`",
        ),
    ];
    for (fault, want) in cases {
        let mut v = small();
        fault(&mut v);
        let e = check(&v).unwrap_err();
        assert!(e.contains(want), "want {want:?}, got {e}");
        assert!(e.starts_with("lifecycles/akış.json: "), "{e}");
    }
}

#[test]
fn the_example_and_the_manual_lifecycle_are_valid() {
    lifecycle::load_text(&example_path(), &example_without_project_actionables(), &[]).unwrap();
    validate::validate(Path::new("manual.json"), &lifecycle::manual(), &[]).unwrap();
}

#[test]
fn a_state_only_a_person_reaches_is_a_warning() {
    let mut v = small();
    v["states"] = json!(["a", "b", "c", "beklemede"]);
    let l = check(&v).unwrap();
    assert_eq!(
        validate::warnings(&l.file),
        [
            "state \"beklemede\": no transition enters or leaves it; only a person can move a ticket there"
        ]
    );
    assert!(
        validate::warnings(&lifecycle::manual()).is_empty(),
        "manual has no transitions at all"
    );
}

#[test]
fn versions_follow_content_not_formatting_or_descriptions() {
    let a = check(&small()).unwrap();
    assert_eq!(a.version.len(), 64);
    assert_eq!(lifecycle::short(&a.version).len(), 12);
    let pretty = serde_json::to_string_pretty(&small()).unwrap();
    let b = lifecycle::load_text(Path::new("lifecycles/akış.json"), &pretty, &[]).unwrap();
    assert_eq!(a.version, b.version, "formatting does not count");
    let mut v = small();
    v["description"] = json!("açıklama");
    v["transitions"][0]["description"] = json!("başlangıç");
    v["transitions"][1]["do"][0]["description"] = json!("istek");
    assert_eq!(
        a.version,
        check(&v).unwrap().version,
        "descriptions do not count"
    );
    let mut v = small();
    v["transitions"][1]["timeout"] = json!("6m");
    assert_ne!(
        a.version,
        check(&v).unwrap().version,
        "a transition changed"
    );
    let mut v = small();
    v["transitions"][1]["do"][0]["with"]["url"] = json!("https://example.org/${ticket.id}");
    assert_ne!(a.version, check(&v).unwrap().version, "an argument changed");
    let mut v = small();
    v["transitions"][0]["do"] = json!([{ "action": "ticket.update@1", "with": { "note": "ü" } }]);
    let named = check(&v).unwrap().version;
    v["transitions"][0]["do"] = json!([{ "with": { "note": "ü" }, "action": "ticket.update@1" }]);
    assert_eq!(
        named,
        check(&v).unwrap().version,
        "key order does not count"
    );
    assert_eq!(a.version, check(&small()).unwrap().version, "stable");
}

fn checks_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/actionables/checks/actionable.json")
}

fn checks() -> Definition {
    definitions::load_file(&checks_path()).unwrap()
}

#[test]
fn the_example_with_its_project_actionable() {
    let text = std::fs::read_to_string(example_path()).unwrap();
    let l = lifecycle::load_text(&example_path(), &text, &[checks()]).unwrap();
    assert_eq!(l.file.transitions[2].actions[2].action, "checks.is-valid@1");
    let e = lifecycle::load_text(&example_path(), &text, &[])
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("transition \"processing -> finished\": unknown actionable checks.is-valid@1"),
        "{e}"
    );
    let mut v: Value = serde_json::from_str(&text).unwrap();
    v["transitions"][2]["do"][2]["action"] = json!("checks.is-valid@2");
    let e = lifecycle::load_text(&example_path(), &v.to_string(), &[checks()])
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("actionable checks is at version 1, not 2"),
        "{e}"
    );
    v["transitions"][2]["do"][2]["action"] = json!("checks.yok@1");
    let e = lifecycle::load_text(&example_path(), &v.to_string(), &[checks()])
        .unwrap_err()
        .to_string();
    assert!(e.contains("actionable checks has no target yok"), "{e}");
}

#[test]
fn a_definition_counts_toward_the_version() {
    let text = std::fs::read_to_string(example_path()).unwrap();
    let a = lifecycle::load_text(&example_path(), &text, &[checks()])
        .unwrap()
        .version;
    let mut d = checks();
    d.description = "başka açıklama".into();
    assert_eq!(
        a,
        lifecycle::load_text(&example_path(), &text, &[d])
            .unwrap()
            .version
    );
    let mut d = checks();
    d.environment.insert("REGION".into(), "eu".into());
    assert_ne!(
        a,
        lifecycle::load_text(&example_path(), &text, &[d])
            .unwrap()
            .version
    );
}

#[test]
fn every_broken_definition_says_what_is_wrong() {
    type Fault = fn(&mut Value);
    let base: Value =
        serde_json::from_str(&std::fs::read_to_string(checks_path()).unwrap()).unwrap();
    let cases: Vec<(&str, Fault, &str)> = vec![
        (
            "checks",
            |v| v["name"] = json!("başka"),
            "name \"başka\" must match the folder name \"checks\"",
        ),
        (
            "http",
            |v| v["name"] = json!("http"),
            "name \"http\" is taken by a built-in actionable",
        ),
        ("checks", |v| v["version"] = json!(0), "version starts at 1"),
        (
            "checks",
            |v| v["type"] = json!("ruby"),
            "type is python@<version>, bun@<version> or executable, not \"ruby\"",
        ),
        (
            "checks",
            |v| v["type"] = json!("python@"),
            "type is python@<version>",
        ),
        (
            "checks",
            |v| {
                v["type"] = json!("bun@1.2");
                v["targets"] = json!({ "slug": { "file": "text.ts", "export": "slug" } });
            },
            "home and requirements are for python@ types",
        ),
        ("checks", |v| v["targets"] = json!({}), "targets is empty"),
        (
            "checks",
            |v| v["targets"] = json!({ "Bad": { "module": "m", "callable": "c" } }),
            "target \"Bad\": a target name is lowercase letters, digits and -",
        ),
        (
            "checks",
            |v| v["targets"]["is-valid"] = json!({ "module": "rules" }),
            "target \"is-valid\": a python@ target is a module and a callable",
        ),
        (
            "checks",
            |v| v["targets"]["is-valid"] = json!({ "file": "a.ts", "export": "f" }),
            "target \"is-valid\": a python@ target is a module and a callable",
        ),
        (
            "checks",
            |v| v["limits"]["timeout"] = json!("soon"),
            "a duration is like",
        ),
        (
            "checks",
            |v| v["environment"]["API_KEY"] = json!("${env.API_KEY"),
            "unclosed",
        ),
        ("checks", |v| v["renk"] = json!(1), "unknown field `renk`"),
    ];
    for (folder, fault, want) in cases {
        let mut v = base.clone();
        fault(&mut v);
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join(folder).join("actionable.json");
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, v.to_string()).unwrap();
        let e = definitions::load_file(&f).unwrap_err().to_string();
        assert!(e.contains(want), "want {want:?}, got {e}");
        assert!(e.starts_with(&f.display().to_string()), "{e}");
    }
    let bun = json!({ "name": "metin", "version": 2, "type": "bun@1.2",
        "targets": { "slug": { "file": "text.ts", "export": "slug" } } });
    let exe = json!({ "name": "araç", "version": 1, "type": "executable",
        "targets": { "run": { "executable": "./run.sh", "arguments": ["--hızlı"] } } });
    for v in [bun, exe] {
        let d = tempfile::tempdir().unwrap();
        let f = d
            .path()
            .join(v["name"].as_str().unwrap())
            .join("actionable.json");
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, v.to_string()).unwrap();
        definitions::load_file(&f).unwrap();
    }
}

#[test]
fn the_projects_actionables_are_read_from_their_folders() {
    let d = tempfile::tempdir().unwrap();
    let p = primum::store::Project {
        root: d.path().to_path_buf(),
        name: "café".into(),
    };
    assert!(definitions::load_all(&p).unwrap().is_empty());
    let dir = d.path().join("actionables/checks");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(checks_path(), dir.join("actionable.json")).unwrap();
    std::fs::create_dir_all(d.path().join("actionables/boş")).unwrap(); // no actionable.json: skipped
    let all = definitions::load_all(&p).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].name, "checks");
}
