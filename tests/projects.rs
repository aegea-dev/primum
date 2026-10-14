use primum::projects::{self, Settings, Workspace};
use std::fs;
use std::path::Path;

#[test]
fn project_json_round_trip_keeps_unknown_keys() {
    let d = tempfile::tempdir().unwrap();
    fs::create_dir_all(d.path().join("items")).unwrap();
    fs::write(
        d.path().join("project.json"),
        "{\"name\": \"café\", \"later\": {\"x\": 1}}\n",
    )
    .unwrap();
    let mut s = Settings::read(d.path()).unwrap().unwrap();
    assert_eq!(s.name(), Some("café"));
    s.set_github_repo(Some("owner/cafe"));
    s.write(d.path()).unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(d.path().join("project.json")).unwrap()).unwrap();
    assert_eq!(v["github"]["repo"], "owner/cafe");
    assert_eq!(v["later"]["x"], 1, "unknown keys survive");
    let p = projects::open(d.path()).unwrap();
    assert_eq!(
        projects::github_repo(&p).unwrap().as_deref(),
        Some("owner/cafe")
    );
}

#[test]
fn github_settings_live_in_project_json() {
    let d = tempfile::tempdir().unwrap();
    let p = projects::init(&d.path().join("über"), None).unwrap();
    assert_eq!(projects::github_repo(&p).unwrap(), None);
    projects::set_github(&p, Some("owner/uber")).unwrap();
    assert_eq!(
        projects::github_repo(&p).unwrap().as_deref(),
        Some("owner/uber")
    );
    assert!(
        projects::set_github(&p, Some("uber")).is_err(),
        "owner/repo"
    );
    // ids is kept when the repository changes, and read as a style
    let json = p.root.join("project.json");
    let mut v: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&json).unwrap()).unwrap();
    v["github"]["ids"] = "title".into();
    fs::write(&json, v.to_string()).unwrap();
    projects::set_github(&p, Some("owner/other")).unwrap();
    assert!(projects::github_ids(&p).unwrap().title());
    v["github"]["ids"] = "badge".into();
    v["github"]["repo"] = "owner/other".into();
    fs::write(&json, v.to_string()).unwrap();
    assert!(projects::github_ids(&p).is_err(), "none or title");
    projects::set_github(&p, None).unwrap();
    assert_eq!(projects::github_repo(&p).unwrap(), None);
    assert_eq!(
        fs::read_to_string(&json).unwrap(),
        "{\n  \"name\": \"über\"\n}\n"
    );
}

#[test]
fn a_legacy_primum_toml_reads_as_a_project() {
    let d = tempfile::tempdir().unwrap();
    fs::create_dir_all(d.path().join("items")).unwrap();
    fs::write(
        d.path().join("primum.toml"),
        "name = \"ışık\"\n\n[github]\nrepo = \"owner/isik\"\nids = \"title\"\n",
    )
    .unwrap();
    let p = projects::open(d.path()).unwrap();
    assert_eq!(p.name, "ışık");
    assert_eq!(
        projects::github_repo(&p).unwrap().as_deref(),
        Some("owner/isik")
    );
    assert!(projects::github_ids(&p).unwrap().title());
    // writing a setting moves it to project.json; the legacy file goes
    projects::set_github(&p, None).unwrap();
    assert!(d.path().join("project.json").is_file());
    assert!(!d.path().join("primum.toml").exists());
    assert_eq!(projects::open(d.path()).unwrap().name, "ışık");
}

#[test]
fn a_workspace_file_or_a_broken_project_json_is_not_a_project() {
    let d = tempfile::tempdir().unwrap();
    fs::create_dir_all(d.path().join("items")).unwrap();
    fs::write(d.path().join("primum.toml"), "projects = [\"a\"]\n").unwrap();
    assert!(projects::open(d.path()).is_err());
    fs::write(d.path().join("project.json"), "[1]").unwrap();
    let e = projects::open(d.path()).unwrap_err();
    assert!(e.contains("project.json"), "{e}");
}

/// A workspace with projects `café` and `über`.
fn workspace(d: &Path) {
    for name in ["café", "über"] {
        projects::init(&d.join(name), None).unwrap();
    }
    Workspace::create(d).unwrap();
}

fn names(ws: &Workspace) -> Vec<String> {
    ws.entries()
        .iter()
        .map(|e| {
            e.project
                .as_ref()
                .map(|p| p.name.clone())
                .unwrap_or_else(|e| e.clone())
        })
        .collect()
}

#[test]
fn the_project_is_found_from_any_folder_inside_it() {
    let d = tempfile::tempdir().unwrap();
    workspace(d.path());
    let deep = d.path().join("café/items");
    assert_eq!(projects::resolve(None, &deep).unwrap().name, "café");
    assert_eq!(projects::resolve(Some("über"), &deep).unwrap().name, "über");
    assert_eq!(
        projects::resolve(Some("über"), d.path()).unwrap().name,
        "über"
    );
    let e = projects::resolve(None, d.path()).unwrap_err().to_string();
    assert!(
        e.contains("-p") && e.contains("café") && e.contains("über"),
        "{e}"
    );
    assert!(projects::resolve(Some("none"), d.path()).is_err());
}

#[test]
fn the_workspace_file_orders_tabs_and_lists_what_is_missing() {
    let d = tempfile::tempdir().unwrap();
    workspace(d.path());
    assert_eq!(
        fs::read_to_string(d.path().join("primum.toml")).unwrap(),
        "projects = [\"café\", \"über\"]\n"
    );
    fs::write(
        d.path().join("primum.toml"),
        "projects = [\"über\", \"café\", \"gone\"]\n",
    )
    .unwrap();
    let ws = Workspace::find(&d.path().join("über")).unwrap();
    let n = names(&ws);
    assert_eq!(n[..2], ["über", "café"]);
    assert!(
        n[2].contains("gone"),
        "a missing project is listed, not fatal"
    );
}

#[test]
fn without_a_list_every_project_subfolder_counts() {
    let d = tempfile::tempdir().unwrap();
    workspace(d.path());
    fs::write(d.path().join("primum.toml"), "").unwrap();
    fs::create_dir_all(d.path().join("notes")).unwrap();
    let ws = Workspace::find(d.path()).unwrap();
    assert_eq!(names(&ws), ["café", "über"], "a plain folder is skipped");
}

#[test]
fn a_project_with_no_workspace_works_alone() {
    let d = tempfile::tempdir().unwrap();
    let p = projects::init(&d.path().join("solo"), Some("solo")).unwrap();
    assert_eq!(
        projects::resolve(None, &p.root.join("items")).unwrap().name,
        "solo"
    );
    assert_eq!(projects::entries(&p.root).len(), 1);
    assert!(
        projects::init(&p.root, None).is_err(),
        "init twice is refused"
    );
}

#[test]
fn two_projects_with_one_name_are_refused() {
    let d = tempfile::tempdir().unwrap();
    workspace(d.path());
    let ws = Workspace::find(d.path()).unwrap();
    let p = ws.init_project("ışık", None).unwrap();
    assert_eq!(p.name, "ışık");
    assert!(
        fs::read_to_string(d.path().join("primum.toml"))
            .unwrap()
            .contains("ışık")
    );
    let ws = Workspace::find(d.path()).unwrap();
    assert!(
        ws.init_project("copy", Some("café")).is_err(),
        "the name is taken"
    );
    assert!(!d.path().join("copy").exists(), "nothing is made");
    // a copied folder with a taken name is shown as unavailable
    fs::create_dir_all(d.path().join("copy/items")).unwrap();
    fs::write(d.path().join("copy/project.json"), "{\"name\": \"café\"}\n").unwrap();
    assert!(ws.add("copy").is_err());
    fs::write(d.path().join("primum.toml"), "").unwrap();
    let n = names(&Workspace::find(d.path()).unwrap());
    assert!(n.iter().any(|x| x.contains("another project")), "{n:?}");
}

#[test]
fn projects_are_added_and_removed_by_subfolder_and_name() {
    let d = tempfile::tempdir().unwrap();
    workspace(d.path());
    let ws = Workspace::find(d.path()).unwrap();
    ws.remove("über").unwrap();
    let ws = Workspace::find(d.path()).unwrap();
    assert_eq!(names(&ws), ["café"]);
    assert!(
        d.path().join("über/project.json").is_file(),
        "never touched"
    );
    assert!(ws.remove("über").is_err());
    ws.add("über").unwrap();
    assert_eq!(names(&Workspace::find(d.path()).unwrap()), ["café", "über"]);
    assert!(ws.add("../elsewhere").is_err(), "subfolders only");
}

#[test]
fn the_old_central_list_is_never_read() {
    let d = tempfile::tempdir().unwrap();
    let cfg = d.path().join("home/.config/primum");
    fs::create_dir_all(&cfg).unwrap();
    projects::init(&d.path().join("other"), None).unwrap();
    fs::write(
        cfg.join("projects.toml"),
        format!("projects = [\"{}\"]\n", d.path().join("other").display()),
    )
    .unwrap();
    let elsewhere = d.path().join("empty");
    fs::create_dir_all(&elsewhere).unwrap();
    assert!(projects::resolve(None, &elsewhere).is_err());
    assert!(projects::entries(&elsewhere).is_empty());
}

#[test]
fn migrate_converts_legacy_boards_once() {
    let d = tempfile::tempdir().unwrap();
    for sub in ["primum", "videre"] {
        let dir = d.path().join(sub);
        fs::create_dir_all(dir.join("items")).unwrap();
        fs::write(dir.join("primum.toml"), format!("name = \"{sub}\"\n")).unwrap();
        let p = projects::open(&dir).unwrap();
        primum::store::insert(&p, "TODO", "Işık ve gölge", Default::default()).unwrap();
    }
    let done = projects::migrate(d.path()).unwrap();
    assert_eq!(done.len(), 3, "{done:?}"); // two projects and the workspace file
    for sub in ["primum", "videre"] {
        let dir = d.path().join(sub);
        assert!(dir.join("project.json").is_file());
        assert!(!dir.join("primum.toml").exists());
        let p = projects::open(&dir).unwrap();
        let h = primum::history::History::open(&p).unwrap().unwrap();
        let last = h.git(&["log", "-1", "--format=%s"]).unwrap();
        assert_eq!(last.trim(), "move settings to project.json");
        let files = h.git(&["ls-files"]).unwrap();
        assert!(
            files.contains("project.json") && !files.contains("primum.toml"),
            "{files}"
        );
    }
    let ws = Workspace::find(d.path()).unwrap();
    assert_eq!(ws.listed.unwrap(), ["primum", "videre"]);
    assert!(
        projects::migrate(d.path()).unwrap().is_empty(),
        "twice changes nothing"
    );
}
