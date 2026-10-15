mod common;
use primum::store::{Changes, attach, insert, load};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn primum() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_primum"))
}

/// Runs primum in `dir` with `stdin`; `store` false keeps the OS secret
/// store out of it.
fn run(dir: &Path, args: &[&str], stdin: &str, store: bool, env: &[(&str, &str)]) -> Output {
    let mut c = Command::new(primum());
    c.args(args)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_remove("API_KEY");
    if !store {
        c.env("PRIMUM_NO_SECRET_STORE", "1");
    }
    for (k, v) in env {
        c.env(k, v);
    }
    let mut child = c.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

#[test]
fn names_are_like_environment_variables() {
    let d = tempfile::tempdir().unwrap();
    let o = run(d.path(), &["secret", "set", "kötü-ad"], "x\n", false, &[]);
    assert!(!o.status.success());
    assert!(
        err(&o).contains("a letter or _, then letters, digits or _"),
        "{}",
        err(&o)
    );
}

#[test]
fn with_the_store_off_secret_commands_say_so() {
    let d = tempfile::tempdir().unwrap();
    for args in [
        &["secret", "set", "API_KEY"][..],
        &["secret", "unset", "API_KEY"],
        &["secret", "list"],
    ] {
        let o = run(d.path(), args, "gizli\n", false, &[]);
        assert!(!o.status.success(), "{args:?}");
        assert!(
            err(&o).contains("the OS secret store is off"),
            "{args:?}: {}",
            err(&o)
        );
    }
}

/// A project whose lifecycle sends `${env.<name>}` to a local server.
fn fetching(d: &Path, url: &str, name: &str) -> (primum::store::Project, String) {
    let p = common::project(d);
    std::fs::write(d.join("project.json"), "{\"name\": \"deneme\"}\n").unwrap();
    std::fs::create_dir_all(d.join("lifecycles")).unwrap();
    std::fs::write(
        d.join("lifecycles/getir.json"),
        format!(
            r#"{{ "name": "getir", "states": ["yeni", "bitti"], "start": "yeni", "end": ["bitti"],
                 "transitions": [ {{ "from": "yeni", "on": "lifecycle.attached", "to": "bitti",
                   "do": [ {{ "action": "http.request@1",
                             "with": {{ "url": "{url}/", "headers": {{ "Authorization": "Bearer ${{env.{name}}}" }} }} }} ] }} ] }}"#
        ),
    )
    .unwrap();
    let id = insert(&p, "TODO", "Gölge", Changes::default()).unwrap();
    attach(&p, &id, "getir", None).unwrap();
    (p, id)
}

#[test]
fn with_the_store_off_the_environment_still_gives_secrets() {
    let d = tempfile::tempdir().unwrap();
    let (url, server) = common::http_server();
    let (p, id) = fetching(d.path(), &url, "API_KEY");
    let o = run(
        d.path(),
        &["daemon", "--once"],
        "",
        false,
        &[("API_KEY", "çevreden")],
    );
    assert!(o.status.success(), "{}", err(&o));
    assert_eq!(server.join().unwrap()["authorization"], "Bearer çevreden");
    assert_eq!(load(&p).unwrap().get(&id).unwrap().status, "bitti");
}

/// Writes to the person's real secret store; runs only when asked.
#[test]
fn a_stored_secret_reaches_the_worker() {
    if std::env::var_os("PRIMUM_TEST_SECRET_STORE").is_none() {
        eprintln!("skipped: set PRIMUM_TEST_SECRET_STORE=1 to use the real OS secret store");
        return;
    }
    let name = format!("PRIMUM_TEST_{}", uuid::Uuid::now_v7().simple());
    let d = tempfile::tempdir().unwrap();
    let o = run(
        d.path(),
        &["secret", "set", &name],
        "depodan-gizli\n",
        true,
        &[],
    );
    assert!(o.status.success(), "{}", err(&o));
    let list = run(d.path(), &["secret", "list"], "", true, &[]);
    let listed = String::from_utf8_lossy(&list.stdout).to_string();
    assert!(
        listed.contains(&name) && !listed.contains("depodan-gizli"),
        "{listed}"
    );
    let (url, server) = common::http_server();
    let (p, id) = fetching(d.path(), &url, &name);
    let o = run(d.path(), &["daemon", "--once"], "", true, &[]);
    assert!(o.status.success(), "{}", err(&o));
    assert_eq!(
        server.join().unwrap()["authorization"],
        "Bearer depodan-gizli"
    );
    assert_eq!(load(&p).unwrap().get(&id).unwrap().status, "bitti");
    let o = run(d.path(), &["secret", "unset", &name], "", true, &[]);
    assert!(o.status.success(), "{}", err(&o));
    let list = run(d.path(), &["secret", "list"], "", true, &[]);
    assert!(!String::from_utf8_lossy(&list.stdout).contains(&name));
}
