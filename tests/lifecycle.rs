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
