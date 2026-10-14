mod common;
use std::process::{Command, Output, Stdio};

/// A temporary workspace (also HOME) with a project `café`, made by the
/// command line.
struct T {
    dir: tempfile::TempDir,
}

impl T {
    fn new() -> T {
        let t = T {
            dir: tempfile::tempdir().unwrap(),
        };
        let ws = t.run_at(&["workspace", "init"], t.dir.path(), "");
        assert!(ws.status.success(), "{ws:?}");
        let init = t.run_at(&["init", "café"], t.dir.path(), "");
        assert!(init.status.success(), "{init:?}");
        t
    }

    /// Runs primum in the project `café`.
    fn run_in(&self, args: &[&str], stdin: &str) -> Output {
        self.run_at(args, &self.dir.path().join("café"), stdin)
    }

    fn run_at(&self, args: &[&str], dir: &std::path::Path, stdin: &str) -> Output {
        use std::io::Write;
        let mut c = Command::new(env!("CARGO_BIN_EXE_primum"))
            .args(args)
            .current_dir(dir)
            .env_clear()
            .env("PATH", std::env::var("PATH").unwrap())
            .env("HOME", self.dir.path())
            .env("NO_COLOR", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        c.stdin.take().unwrap().write_all(stdin.as_bytes()).unwrap();
        c.wait_with_output().unwrap()
    }

    fn run(&self, args: &[&str]) -> Output {
        self.run_in(args, "")
    }

    fn out(&self, args: &[&str]) -> String {
        let o = self.run(args);
        assert!(o.status.success(), "{args:?}: {o:?}");
        String::from_utf8(o.stdout).unwrap()
    }

    fn err(&self, args: &[&str]) -> String {
        String::from_utf8(self.run(args).stderr).unwrap()
    }

    fn file(&self, name: &str, text: &str) -> String {
        let p = self.dir.path().join(name);
        std::fs::write(&p, text).unwrap();
        p.to_string_lossy().into()
    }

    fn item(&self, id: &str) -> String {
        let name = format!("{}.md", id.replace(':', "-"));
        std::fs::read_to_string(self.dir.path().join("café/items").join(name)).unwrap()
    }

    fn items(&self) -> usize {
        std::fs::read_dir(self.dir.path().join("café/items"))
            .unwrap()
            .count()
    }
}

#[test]
fn init_names_the_project_after_its_folder() {
    let t = T::new();
    let out = t.out(&["projects"]);
    assert!(out.contains("café") && out.contains("ok"), "{out}");
    assert!(t.run(&["init"]).status.code() == Some(1));
}

#[test]
fn add_reads_the_body_from_a_file() {
    let t = T::new();
    let f = t.file("b.md", "# Neden\n\nÇünkü.\n");
    let args = [
        "add",
        "todo",
        "Gökçe'nin galerisi",
        "--impact",
        "7",
        "--body-file",
        &f,
    ];
    assert_eq!(t.out(&args).trim(), "1");
    let item = t.item("1");
    assert!(item.contains("impact: 7") && item.ends_with("Çünkü.\n"));
}

#[test]
fn body_from_standard_input() {
    let t = T::new();
    t.run_in(
        &["add", "BUG", "x", "--body-file", "-"],
        "satır bir\nsatır iki\n",
    );
    assert!(t.item("1").ends_with("\nsatır bir\nsatır iki\n"));
}

#[test]
fn set_start_done_note_doc_kind() {
    let t = T::new();
    t.out(&["add", "TODO", "x", "--risk", "4"]);
    assert_eq!(
        t.out(&["set", "1", "--effort", "6", "--title", "Işık"])
            .trim(),
        "#1: title, effort"
    );
    assert_eq!(t.out(&["start", "#1"]).trim(), "#1: doing");
    let i = t.item("1");
    assert!(
        i.contains("title: \"Işık\"")
            && i.contains("effort: 6")
            && i.contains("risk: 4")
            && i.contains("status: doing"),
        "{i}"
    );
    let n = t.file("n.md", "Ölçüldü.\n");
    assert!(
        t.out(&["note", "1", "--file", &n])
            .starts_with("#1: note 20")
    );
    assert!(t.item("1").contains("\n## 20"));
    let s = t.file("s.md", "# Spec\n");
    assert_eq!(
        t.out(&["doc", "1", "spec", "--file", &s]).trim(),
        "items/1.spec.md"
    );
    assert_eq!(t.out(&["kind", "1", "bug"]).trim(), "#1: kind BUG");
    assert_eq!(t.out(&["done", "1"]).trim(), "#1: done");
    let i = t.item("1");
    assert!(i.contains("kind: BUG") && i.contains("status: done") && i.contains("finished: "));
}

#[test]
fn needs_are_checked_and_cleared() {
    let t = T::new();
    t.out(&["add", "BUG", "a"]);
    t.out(&["add", "TODO", "b", "--needs", "1, #7"]);
    assert!(t.item("2").contains("needs: [1, 7]"));
    for bad in ["fikir", "TODO:1"] {
        assert_eq!(t.run(&["set", "2", "--needs", bad]).status.code(), Some(1));
    }
    t.out(&["set", "2", "--needs", ""]);
    assert!(t.item("2").contains("needs: []"));
}

#[test]
fn errors() {
    let t = T::new();
    assert_eq!(t.run(&["set", "9", "--effort", "3"]).status.code(), Some(1));
    assert_eq!(
        t.err(&["set", "9", "--effort", "3"]).trim(),
        "primum: no item 9"
    );
    assert_eq!(
        t.err(&["show", "TODO:1"]).trim(),
        "primum: TODO:1 is not an item id, like 12 or #12"
    );
    assert_eq!(t.run(&["add", "IDEA", "x"]).status.code(), Some(1));
    assert_eq!(
        t.run(&["add", "TODO", "x", "--body-file", "/yok/b.md"])
            .status
            .code(),
        Some(1)
    );
    assert_eq!(
        t.run(&["add", "TODO", "x", "--effort", "11"]).status.code(),
        Some(2)
    );
    assert_eq!(t.items(), 0);
}

#[test]
fn list_and_next_follow_priority_and_json_is_complete() {
    let t = T::new();
    t.out(&["add", "TODO", "düşük", "--impact", "2"]);
    t.out(&["add", "BUG", "hata"]);
    t.out(&["add", "TODO", "yüksek", "--impact", "9"]);
    t.out(&["done", "1"]);
    let out = t.out(&["list"]);
    let rows: Vec<Vec<&str>> = out
        .lines()
        .skip(1)
        .map(|l| l.split_whitespace().take(2).collect())
        .collect();
    assert_eq!(rows, [["#2", "BUG"], ["#3", "TODO"]]);
    assert!(!out.contains('\u{1b}'));
    assert_eq!(
        t.out(&["next", "-n", "1"]).split_whitespace().next(),
        Some("#2")
    );
    t.out(&["start", "3"]);
    assert!(t.out(&["next"]).starts_with("in progress: 3\n"));
    let json: serde_json::Value =
        serde_json::from_str(&t.out(&["list", "--all", "--json"])).unwrap();
    assert_eq!(json.as_array().unwrap().len(), 3);
    assert!(json[0].get("score").is_some() && json[0].get("blocked").is_some());
    let done: serde_json::Value =
        serde_json::from_str(&t.out(&["list", "--status", "done", "--json"])).unwrap();
    assert_eq!(done[0]["id"], "1");
    let bugs = t.out(&["list", "--kind", "bug"]);
    assert_eq!(bugs.lines().count(), 2);
}

#[test]
fn show() {
    let t = T::new();
    let f = t.file("b.md", "## Neden\n\nmetin\n");
    t.out(&["add", "TODO", "Özgür", "--body-file", &f]);
    let s = t.file("s.md", "# Plan\n");
    t.out(&["doc", "1", "plan", "--file", &s]);
    let out = t.out(&["show", "#1"]);
    for want in [
        "#1",
        "TODO",
        "Özgür",
        "score",
        "## Neden",
        "metin",
        "items/1.plan.md",
    ] {
        assert!(out.contains(want), "{want} missing from {out}");
    }
    let json: serde_json::Value = serde_json::from_str(&t.out(&["show", "1", "--json"])).unwrap();
    assert_eq!(json["docs"][0]["kind"], "plan");
}

#[test]
fn delete_asks_unless_told() {
    let t = T::new();
    t.out(&["add", "BUG", "Gökçe"]);
    let s = t.file("s.md", "# R\n");
    t.out(&["doc", "1", "report", "--file", &s]);
    assert_eq!(
        t.run(&["delete", "1"]).status.code(),
        Some(1),
        "no terminal, no --yes"
    );
    assert_eq!(t.items(), 2);
    let out = t.out(&["delete", "1", "--yes"]);
    assert_eq!(out, "items/1.md\nitems/1.report.md\n");
    assert_eq!(t.items(), 0);
}

#[test]
fn a_workspace_holds_several_projects() {
    let t = T::new();
    let root = t.dir.path();
    let made = t.run_at(&["init", "über"], root, "");
    assert!(made.status.success(), "{made:?}");
    let list = String::from_utf8(t.run_at(&["projects"], root, "").stdout).unwrap();
    assert!(list.contains("café") && list.contains("über"), "{list}");
    // from the workspace root a command needs -p
    let o = t.run_at(&["add", "TODO", "x"], root, "");
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stderr).contains("-p NAME: café, über"));
    // inside a project, -p reaches the other one
    assert_eq!(t.out(&["-p", "über", "add", "TODO", "Işık"]).trim(), "1");
    assert!(root.join("über/items/1.md").is_file());
    assert_eq!(t.items(), 0);
    let ws = std::fs::read_to_string(root.join("primum.toml")).unwrap();
    assert_eq!(ws, "projects = [\"café\", \"über\"]\n");
    t.out(&["projects", "remove", "über"]);
    assert!(!t.out(&["projects"]).contains("über"));
    t.out(&["projects", "add", "über"]);
    assert!(t.out(&["projects"]).contains("über"));
    // the same name twice is refused
    let o = t.run_at(&["init", "copy", "--name", "café"], root, "");
    assert_eq!(o.status.code(), Some(1), "{o:?}");
}

#[test]
fn the_old_central_list_gets_one_notice() {
    let t = T::new();
    std::fs::create_dir_all(t.dir.path().join(".config/primum")).unwrap();
    std::fs::write(
        t.dir.path().join(".config/primum/projects.toml"),
        "projects = []\n",
    )
    .unwrap();
    let first = t.err(&["list"]);
    assert!(first.contains("no longer used"), "{first}");
    assert!(!t.err(&["list"]).contains("no longer used"));
}

#[test]
fn workspace_migrate_converts_earlier_boards() {
    let t = T::new();
    let old = t.dir.path().join("old");
    std::fs::create_dir_all(old.join("b/items")).unwrap();
    std::fs::write(old.join("b/primum.toml"), "name = \"eski\"\n").unwrap();
    let o = t.run_at(&["workspace", "migrate"], &old, "");
    let out = String::from_utf8_lossy(&o.stdout).to_string();
    assert!(out.contains("b: primum.toml -> project.json"), "{o:?}");
    assert!(out.contains("a workspace of 1 projects"), "{out}");
    let again = t.run_at(&["workspace", "migrate"], &old, "");
    assert_eq!(
        String::from_utf8_lossy(&again.stdout),
        "nothing to migrate\n"
    );
    let list = String::from_utf8(t.run_at(&["projects"], &old, "").stdout).unwrap();
    assert!(list.starts_with("eski\t"), "{list}");
}

#[test]
fn history_is_kept_and_off_without_git() {
    let t = T::new();
    t.out(&["add", "TODO", "Işıl"]);
    assert!(t.dir.path().join("café/.primum/history.git/HEAD").is_file());
    let o = Command::new(env!("CARGO_BIN_EXE_primum"))
        .args(["add", "BUG", "gitsiz"])
        .current_dir(t.dir.path().join("café"))
        .env_clear()
        .env("HOME", t.dir.path())
        .env("PRIMUM_GIT", "/yok/git")
        .output()
        .unwrap();
    assert!(o.status.success(), "{o:?}");
    assert_eq!(String::from_utf8_lossy(&o.stdout).trim(), "2");
    assert_eq!(
        String::from_utf8_lossy(&o.stderr).trim(),
        "primum: history is off: git not found"
    );
}

#[test]
fn log_lists_an_items_changes() {
    let t = T::new();
    t.out(&["add", "TODO", "Özgür"]);
    t.out(&["start", "1"]);
    let out = t.out(&["log", "#1"]);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 2, "{out}");
    assert!(lines[0].ends_with("#1: status doing"), "{out}");
    let json: serde_json::Value = serde_json::from_str(&t.out(&["log", "--json"])).unwrap();
    assert_eq!(json[0]["message"], "#1: status doing");
}

#[test]
fn remote_and_clone_commands() {
    let t = T::new();
    t.out(&["add", "TODO", "Özgür"]);
    let url = t.dir.path().join("remote.git");
    let made = Command::new("git")
        .args(["init", "--quiet", "--bare", "-b", "main"])
        .arg(&url)
        .status()
        .unwrap();
    assert!(made.success());
    let url = url.to_string_lossy().to_string();
    t.out(&["remote", "set", &url, "--manual"]);
    assert!(t.out(&["remote"]).contains(&url));
    t.out(&["sync"]);
    // another workspace on another machine
    let other = t.dir.path().join("elsewhere");
    std::fs::create_dir_all(&other).unwrap();
    assert!(
        t.run_at(&["workspace", "init"], &other, "")
            .status
            .success()
    );
    let o = t.run_at(&["clone", &url, "kopya"], &other, "");
    assert!(o.status.success(), "{o:?}");
    assert!(other.join("kopya/items/1.md").is_file());
    assert!(other.join("kopya/project.json").is_file());
    let list = std::fs::read_to_string(other.join("primum.toml")).unwrap();
    assert!(list.contains("kopya"), "{list}");
}

#[test]
fn changes_sync_by_themselves() {
    let t = T::new();
    let url = t.dir.path().join("remote.git");
    let made = Command::new("git")
        .args(["init", "--quiet", "--bare", "-b", "main"])
        .arg(&url)
        .status()
        .unwrap();
    assert!(made.success());
    t.out(&["remote", "set", &url.to_string_lossy()]);
    t.out(&["add", "TODO", "Gökçe"]);
    let log = Command::new("git")
        .arg("--git-dir")
        .arg(&url)
        .args(["log", "--format=%s", "main"])
        .output()
        .unwrap();
    let log = String::from_utf8_lossy(&log.stdout);
    assert!(log.contains("#1: add \"Gökçe\""), "{log}");
}

#[test]
fn add_without_a_title_uses_the_first_heading() {
    let t = T::new();
    let f = t.file("x.md", "## Ölçüm sayfası\n\nNe ve neden.\n");
    assert_eq!(t.out(&["add", "TODO", "--body-file", &f]).trim(), "1");
    assert!(t.item("1").contains("title: \"Ölçüm sayfası\""));
    let g = t.file("y.md", "no heading\n");
    let o = t.run(&["add", "TODO", "--body-file", &g]);
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&o.stderr).trim(),
        "primum: give a title, or start the text with a heading"
    );
}

#[test]
fn a_closed_pipe_ends_quietly() {
    let t = T::new();
    let items = t.dir.path().join("café/items");
    for n in 1..=2000 {
        let text = format!(
            "---\ntitle: \"Satır {n}\"\nkind: TODO\nstatus: idle\nimpact: 5\nurgency: 3\nrisk: 3\neffort: 5\nneeds: []\n---\n"
        );
        std::fs::write(items.join(format!("{n}.md")), text).unwrap();
    }
    let mut c = Command::new(env!("CARGO_BIN_EXE_primum"))
        .args(["list"])
        .current_dir(t.dir.path().join("café"))
        .env("HOME", t.dir.path())
        .env("NO_COLOR", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        use std::io::Read;
        let mut first = [0u8; 16];
        c.stdout.take().unwrap().read_exact(&mut first).unwrap();
    } // the reader goes away, as `| head` does
    let o = c.wait_with_output().unwrap();
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(!err.contains("panicked"), "{err}");
}

#[test]
fn lifecycle_commands() {
    let t = T::new();
    common::with_example(&t.dir.path().join("café"));
    let dir = t.dir.path().join("café/lifecycles");
    let out = t.out(&["lifecycle", "check"]);
    assert!(out.starts_with("enrich-ticket  "), "{out}");
    assert_eq!(out.split_whitespace().nth(1).unwrap().len(), 12);
    let list = t.out(&["lifecycle", "list"]);
    assert!(
        list.contains("manual") && list.contains("built in") && list.contains("enrich-ticket"),
        "{list}"
    );
    let show = t.out(&["lifecycle", "show", "enrich-ticket"]);
    assert!(
        show.contains("idle -> processing") && show.contains("http.request@1") && show.contains("while waiting"),
        "{show}"
    );
    let json: serde_json::Value =
        serde_json::from_str(&t.out(&["lifecycle", "show", "manual", "--json"])).unwrap();
    assert_eq!(json["file"]["start"], "idle");
    std::fs::write(dir.join("kırık.json"), "{\"name\": \"kırık\"}\n").unwrap();
    let o = t.run(&["lifecycle", "check"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stderr).contains("kırık.json"));
}

#[test]
fn attach_and_detach_on_the_command_line() {
    let t = T::new();
    common::with_example(&t.dir.path().join("café"));
    let dir = t.dir.path().join("café/lifecycles");
    t.out(&["add", "TODO", "Gölge"]);
    let out = t.out(&["lifecycle", "attach", "1", "enrich-ticket"]);
    assert!(out.starts_with("#1: enrich-ticket "), "{out}");
    let show = t.out(&["show", "1"]);
    assert!(
        show.contains("lifecycle enrich-ticket") && show.contains("step create"),
        "{show}"
    );
    assert_eq!(t.out(&["lifecycle", "detach", "#1"]).trim(), "#1: manual");
    assert!(!t.out(&["show", "1"]).contains("lifecycle "));
}

#[test]
fn context_on_the_command_line() {
    let t = T::new();
    t.out(&["add", "TODO", "Çay"]);
    t.out(&["context", "set", "start", "2026-11-01"]);
    t.out(&["context", "set", "owner", "\"Ada\""]);
    let out = t.out(&["context"]);
    assert!(out.contains("start = \"2026-11-01\""), "{out}");
    assert!(out.contains("owner = \"Ada\""), "{out}");
    assert!(
        out.contains("items = 1") && out.contains("status.idle = 1"),
        "{out}"
    );
    t.out(&["context", "set", "owner", "--unset"]);
    assert!(!t.out(&["context"]).contains("owner"));
    let json: serde_json::Value = serde_json::from_str(&t.out(&["context", "--json"])).unwrap();
    assert_eq!(json["stored"]["start"], "2026-11-01");
    assert_eq!(json["computed"]["items"], 1);
}

#[test]
fn start_on_a_lifecycle_without_doing_says_what_to_do() {
    let t = T::new();
    common::with_example(&t.dir.path().join("café"));
    let dir = t.dir.path().join("café/lifecycles");
    t.out(&["add", "TODO", "Gölge"]);
    t.out(&["lifecycle", "attach", "1", "enrich-ticket"]);
    let e = t.err(&["start", "1"]);
    assert!(
        e.contains("is not one of create, idle, waiting, processing, finished, timed-out, failed")
            && e.contains("--status"),
        "{e}"
    );
}
