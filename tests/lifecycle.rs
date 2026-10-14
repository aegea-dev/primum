use primum::lifecycle::{self, parse, validate};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn example_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lifecycles/enrich-ticket.json")
}

/// The example without its project actionable, for tests that use no
/// definitions.
fn example_without_project_actionables() -> String {
    std::fs::read_to_string(example_path())
        .unwrap()
        .replace(
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
    assert_eq!(moves, [("create", "idle"), ("idle", "processing"), ("processing", "finished")]);
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
    assert!(check(&v).unwrap_err().contains("an action is a name"), "a number is neither");
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
    assert_eq!(lifecycle::Rules::of(&f).states, ["idle", "doing", "done", "wontfix"]);
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
    assert_eq!(expr::templates("no templates, ışık").unwrap(), Vec::<String>::new());
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
        (|v| v["name"] = json!("başka"), "name \"başka\" must match the file name \"akış\""),
        (|v| v["states"] = json!([]), "states is empty"),
        (|v| v["states"] = json!(["a", "B", "b", "c"]), "state \"B\" is lowercase letters, digits and -"),
        (|v| v["states"] = json!(["a", "a", "b", "c"]), "state \"a\" is listed twice"),
        (|v| v["states"] = json!(["a", "b", "c", "failed"]), "\"failed\" is the engine's own state; do not list it"),
        (|v| v["start"] = json!("x"), "start \"x\" is not a state"),
        (|v| v["end"] = json!(["x"]), "end \"x\" is not a state"),
        (|v| v["renk"] = json!(1), "unknown field `renk`"),
        (|v| v["transitions"][0]["to"] = json!("x"), "transition \"a -> x\": to \"x\" is not a state"),
        (|v| v["transitions"][0]["from"] = json!("x"), "transition \"x -> b\": from \"x\" is not a state"),
        (|v| v["transitions"][1]["while"] = json!("x"), "transition \"b -> c\": while \"x\" is not a state"),
        (|v| v["transitions"][1]["on_fail"] = json!("x"), "transition \"b -> c\": on_fail \"x\" is not a state"),
        (|v| v["transitions"][1]["on_timeout"] = json!("x"), "transition \"b -> c\": on_timeout \"x\" is not a state"),
        (|v| { v["transitions"][0].as_object_mut().unwrap().remove("on"); }, "transition \"a -> b\": a transition needs on, do, or both"),
        (|v| v["transitions"][0]["on"] = json!("Ticket Created"), "transition \"a -> b\": on \"Ticket Created\" is an event type, like ticket.created"),
        (|v| v["transitions"][1]["timeout"] = json!("soon"), "transition \"b -> c\": a duration is like"),
        (|v| { v["transitions"][1].as_object_mut().unwrap().remove("on_timeout"); }, "transition \"b -> c\": timeout needs on_timeout"),
        (|v| { v["transitions"][1].as_object_mut().unwrap().remove("timeout"); }, "transition \"b -> c\": on_timeout needs timeout"),
        (|v| v["transitions"][0]["when"] = json!("input.x &&"), "transition \"a -> b\": bad expression"),
        (|v| v["transitions"][1]["do"][0]["action"] = json!("fax@1"), "transition \"b -> c\": unknown actionable fax@1"),
        (|v| v["transitions"][1]["do"][0]["with"] = json!({}), "transition \"b -> c\": http.request@1 needs with.url"),
        (|v| v["transitions"][1]["do"][0]["with"]["renk"] = json!(1), "transition \"b -> c\": http.request@1 takes no with.renk"),
        (|v| v["transitions"][1]["do"][0]["with"]["url"] = json!("https://e.com/${ticket.id"), "transition \"b -> c\": unclosed"),
        (|v| v["transitions"][1]["do"][0]["expect"] = json!("output.status =="), "transition \"b -> c\": bad expression"),
        (|v| v["transitions"][1]["do"][0]["renk"] = json!(1), "unknown field `renk`"),
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
        ["state \"beklemede\": no transition enters or leaves it; only a person can move a ticket there"]
    );
    assert!(validate::warnings(&lifecycle::manual()).is_empty(), "manual has no transitions at all");
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
    assert_eq!(a.version, check(&v).unwrap().version, "descriptions do not count");
    let mut v = small();
    v["transitions"][1]["timeout"] = json!("6m");
    assert_ne!(a.version, check(&v).unwrap().version, "a transition changed");
    let mut v = small();
    v["transitions"][1]["do"][0]["with"]["url"] = json!("https://example.org/${ticket.id}");
    assert_ne!(a.version, check(&v).unwrap().version, "an argument changed");
    let mut v = small();
    v["transitions"][0]["do"] = json!([{ "action": "ticket.update@1", "with": { "note": "ü" } }]);
    let named = check(&v).unwrap().version;
    v["transitions"][0]["do"] = json!([{ "with": { "note": "ü" }, "action": "ticket.update@1" }]);
    assert_eq!(named, check(&v).unwrap().version, "key order does not count");
    assert_eq!(a.version, check(&small()).unwrap().version, "stable");
}
