use std::process::{Command, Output, Stdio};

/// A temp HOME with a project made by `primum init` in `işler/`.
struct T {
    dir: tempfile::TempDir,
}

impl T {
    fn new() -> T {
        let t = T {
            dir: tempfile::tempdir().unwrap(),
        };
        std::fs::create_dir_all(t.dir.path().join("işler")).unwrap();
        let out = t.run(&["init"]);
        assert!(out.status.success(), "{out:?}");
        t
    }

    fn run_in(&self, args: &[&str], stdin: &str) -> Output {
        use std::io::Write;
        let mut c = Command::new(env!("CARGO_BIN_EXE_primum"))
            .args(args)
            .current_dir(self.dir.path().join("işler"))
            .env_clear()
            .env("PATH", std::env::var("PATH").unwrap())
            .env("HOME", self.dir.path())
            .env("XDG_CONFIG_HOME", self.dir.path().join("config"))
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
        std::fs::read_to_string(self.dir.path().join("işler/items").join(name)).unwrap()
    }

    fn items(&self) -> usize {
        std::fs::read_dir(self.dir.path().join("işler/items"))
            .unwrap()
            .count()
    }
}

#[test]
fn init_names_the_project_after_its_folder() {
    let t = T::new();
    let out = t.out(&["projects"]);
    assert!(out.contains("işler") && out.contains("ok"), "{out}");
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
    assert_eq!(t.out(&args).trim(), "TODO:1");
    let item = t.item("TODO:1");
    assert!(item.contains("impact: 7") && item.ends_with("Çünkü.\n"));
}

#[test]
fn body_from_standard_input() {
    let t = T::new();
    t.run_in(
        &["add", "BUG", "x", "--body-file", "-"],
        "satır bir\nsatır iki\n",
    );
    assert!(t.item("BUG:1").ends_with("\nsatır bir\nsatır iki\n"));
}

#[test]
fn set_start_done_note_doc_kind() {
    let t = T::new();
    t.out(&["add", "TODO", "x", "--risk", "4"]);
    assert_eq!(
        t.out(&["set", "TODO:1", "--effort", "6", "--title", "Işıl"])
            .trim(),
        "TODO:1: title, effort"
    );
    assert_eq!(t.out(&["start", "TODO:1"]).trim(), "TODO:1: doing");
    let i = t.item("TODO:1");
    assert!(
        i.contains("title: \"Işıl\"")
            && i.contains("effort: 6")
            && i.contains("risk: 4")
            && i.contains("status: doing"),
        "{i}"
    );
    let n = t.file("n.md", "Ölçüldü.\n");
    assert!(
        t.out(&["note", "TODO:1", "--file", &n])
            .starts_with("TODO:1: note 20")
    );
    assert!(t.item("TODO:1").contains("\n## 20"));
    let s = t.file("s.md", "# Spec\n");
    assert_eq!(
        t.out(&["doc", "TODO:1", "spec", "--file", &s]).trim(),
        "items/TODO-1.spec.md"
    );
    assert_eq!(t.out(&["kind", "TODO:1", "bug"]).trim(), "TODO:1 -> BUG:1");
    assert_eq!(t.out(&["done", "bug:1"]).trim(), "BUG:1: done");
    assert!(t.item("BUG:1").contains("status: done") && t.item("BUG:1").contains("finished: "));
}

#[test]
fn needs_are_checked_and_cleared() {
    let t = T::new();
    t.out(&["add", "BUG", "a"]);
    t.out(&["add", "TODO", "b", "--needs", "bug:1, TODO:7"]);
    assert!(t.item("TODO:1").contains("needs: [BUG:1, TODO:7]"));
    assert_eq!(
        t.run(&["set", "TODO:1", "--needs", "fikir"]).status.code(),
        Some(1)
    );
    t.out(&["set", "TODO:1", "--needs", ""]);
    assert!(t.item("TODO:1").contains("needs: []"));
}

#[test]
fn errors() {
    let t = T::new();
    assert_eq!(
        t.run(&["set", "TODO:9", "--effort", "3"]).status.code(),
        Some(1)
    );
    assert_eq!(
        t.err(&["set", "TODO:9", "--effort", "3"]).trim(),
        "primum: no item TODO:9"
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
    t.out(&["done", "TODO:1"]);
    let out = t.out(&["list"]);
    let ids: Vec<&str> = out
        .lines()
        .skip(1)
        .map(|l| l.split_whitespace().next().unwrap())
        .collect();
    assert_eq!(ids, ["BUG:1", "TODO:2"]);
    assert!(!out.contains('\u{1b}'));
    assert_eq!(
        t.out(&["next", "-n", "1"]).split_whitespace().next(),
        Some("BUG:1")
    );
    t.out(&["start", "TODO:2"]);
    assert!(t.out(&["next"]).starts_with("in progress: TODO:2\n"));
    let json: serde_json::Value =
        serde_json::from_str(&t.out(&["list", "--all", "--json"])).unwrap();
    assert_eq!(json.as_array().unwrap().len(), 3);
    assert!(json[0].get("score").is_some() && json[0].get("blocked").is_some());
    let done: serde_json::Value =
        serde_json::from_str(&t.out(&["list", "--status", "done", "--json"])).unwrap();
    assert_eq!(done[0]["id"], "TODO:1");
    let bugs = t.out(&["list", "--kind", "bug"]);
    assert_eq!(bugs.lines().count(), 2);
}

#[test]
fn show() {
    let t = T::new();
    let f = t.file("b.md", "## Neden\n\nmetin\n");
    t.out(&["add", "TODO", "Özgür", "--body-file", &f]);
    let s = t.file("s.md", "# Plan\n");
    t.out(&["doc", "TODO:1", "plan", "--file", &s]);
    let out = t.out(&["show", "TODO:1"]);
    for want in [
        "TODO:1",
        "Özgür",
        "score",
        "## Neden",
        "metin",
        "items/TODO-1.plan.md",
    ] {
        assert!(out.contains(want), "{want} missing from {out}");
    }
    let json: serde_json::Value =
        serde_json::from_str(&t.out(&["show", "TODO:1", "--json"])).unwrap();
    assert_eq!(json["docs"][0]["kind"], "plan");
}

#[test]
fn delete_asks_unless_told() {
    let t = T::new();
    t.out(&["add", "BUG", "Gökçe"]);
    let s = t.file("s.md", "# R\n");
    t.out(&["doc", "BUG:1", "report", "--file", &s]);
    assert_eq!(
        t.run(&["delete", "BUG:1"]).status.code(),
        Some(1),
        "no terminal, no --yes"
    );
    assert_eq!(t.items(), 2);
    let out = t.out(&["delete", "BUG:1", "--yes"]);
    assert_eq!(out, "items/BUG-1.md\nitems/BUG-1.report.md\n");
    assert_eq!(t.items(), 0);
}

#[test]
fn project_flag_and_projects_commands() {
    let t = T::new();
    let other = t.dir.path().join("ödev");
    std::fs::create_dir_all(&other).unwrap();
    let o = other.to_string_lossy().to_string();
    // a second project, made from inside it
    let made = Command::new(env!("CARGO_BIN_EXE_primum"))
        .args(["init", "--name", "ödev"])
        .current_dir(&other)
        .env_clear()
        .env("HOME", t.dir.path())
        .env("XDG_CONFIG_HOME", t.dir.path().join("config"))
        .output()
        .unwrap();
    assert!(made.status.success(), "{made:?}");
    assert_eq!(
        t.out(&["-p", "ödev", "add", "TODO", "orada"]).trim(),
        "TODO:1"
    );
    assert!(other.join("items/TODO-1.md").is_file());
    assert_eq!(t.items(), 0);
    t.out(&["projects", "remove", "ödev"]);
    assert!(!t.out(&["projects"]).contains("ödev"));
    t.out(&["projects", "add", &o]);
    assert!(t.out(&["projects"]).contains("ödev"));
}

#[test]
fn history_is_kept_and_off_without_git() {
    let t = T::new();
    t.out(&["add", "TODO", "Işıl"]);
    assert!(
        t.dir
            .path()
            .join("işler/.primum/history.git/HEAD")
            .is_file()
    );
    let o = Command::new(env!("CARGO_BIN_EXE_primum"))
        .args(["add", "BUG", "gitsiz"])
        .current_dir(t.dir.path().join("işler"))
        .env_clear()
        .env("HOME", t.dir.path())
        .env("XDG_CONFIG_HOME", t.dir.path().join("config"))
        .env("PRIMUM_GIT", "/yok/git")
        .output()
        .unwrap();
    assert!(o.status.success(), "{o:?}");
    assert_eq!(String::from_utf8_lossy(&o.stdout).trim(), "BUG:1");
    assert_eq!(
        String::from_utf8_lossy(&o.stderr).trim(),
        "primum: history is off: git not found"
    );
}

#[test]
fn log_lists_an_items_changes() {
    let t = T::new();
    t.out(&["add", "TODO", "Özgür"]);
    t.out(&["start", "TODO:1"]);
    let out = t.out(&["log", "TODO:1"]);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 2, "{out}");
    assert!(lines[0].ends_with("TODO:1: status doing"), "{out}");
    let json: serde_json::Value = serde_json::from_str(&t.out(&["log", "--json"])).unwrap();
    assert_eq!(json[0]["message"], "TODO:1: status doing");
}
