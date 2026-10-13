use primum::projects;
use std::sync::Mutex;

static LOCK: Mutex<()> = Mutex::new(());

/// Runs `f` with XDG_CONFIG_HOME in a fresh temp dir. Environment variables
/// are process-wide, so these tests run one at a time.
fn with_config<T>(f: impl FnOnce(&std::path::Path) -> T) -> T {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let d = tempfile::tempdir().unwrap();
    unsafe {
        std::env::set_var("XDG_CONFIG_HOME", d.path().join("config"));
    }
    f(d.path())
}

#[test]
fn init_writes_the_project_and_lists_it() {
    with_config(|d| {
        let dir = d.join("videre-board");
        std::fs::create_dir_all(&dir).unwrap();
        let p = projects::init(&dir, Some("videre")).unwrap();
        assert_eq!(p.name, "videre");
        assert!(dir.join("items").is_dir());
        assert_eq!(
            std::fs::read_to_string(dir.join("primum.toml")).unwrap(),
            "name = \"videre\"\n"
        );
        let l = projects::list().unwrap();
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].project.as_ref().unwrap().name, "videre");
        let h = primum::history::History::open(&p).unwrap().unwrap();
        let log = h.git(&["log", "--format=%s"]).unwrap();
        assert_eq!(log.trim(), "primum project videre");
        assert!(projects::init(&dir, None).is_err(), "init twice is refused");
        assert_eq!(projects::list().unwrap().len(), 1);
    });
}

#[test]
fn name_defaults_to_the_folder_and_resolve_follows_the_working_directory() {
    with_config(|d| {
        let a = d.join("işler");
        let b = d.join("ödev");
        for x in [&a, &b] {
            std::fs::create_dir_all(x).unwrap();
            projects::init(x, None).unwrap();
        }
        std::fs::create_dir_all(a.join("items/alt")).unwrap();
        assert_eq!(
            projects::resolve(None, &a.join("items/alt")).unwrap().name,
            "işler"
        );
        assert_eq!(projects::resolve(Some("ödev"), &a).unwrap().name, "ödev");
        assert!(
            projects::resolve(None, d).is_err(),
            "two projects, outside both, no -p"
        );
        assert!(projects::resolve(Some("yok"), &a).is_err());
        projects::remove("ödev").unwrap();
        assert_eq!(projects::resolve(None, d).unwrap().name, "işler");
        assert!(
            b.join("primum.toml").is_file(),
            "remove never touches the project"
        );
    });
}

#[test]
fn a_missing_project_is_unavailable_not_fatal_and_duplicates_are_refused() {
    with_config(|d| {
        let a = d.join("a");
        std::fs::create_dir_all(&a).unwrap();
        projects::init(&a, Some("ortak")).unwrap();
        let listed = projects::list().unwrap()[0].path.clone();
        std::fs::remove_dir_all(&a).unwrap();
        let l = projects::list().unwrap();
        assert!(l[0].project.is_err());
        assert!(
            projects::remove("ortak").is_err(),
            "its name cannot be read"
        );
        projects::remove(&listed.to_string_lossy()).unwrap();
        assert!(projects::list().unwrap().is_empty(), "removed by its path");

        let b = d.join("b");
        let c = d.join("c");
        for x in [&b, &c] {
            std::fs::create_dir_all(x).unwrap();
        }
        projects::init(&b, Some("aynı")).unwrap();
        std::fs::create_dir_all(c.join("items")).unwrap();
        std::fs::write(c.join("primum.toml"), "name = \"aynı\"\n").unwrap();
        assert!(
            projects::add(&c).is_err(),
            "a second project named aynı is refused"
        );
    });
}

#[test]
fn a_bad_primum_toml_is_unavailable_with_its_reason() {
    with_config(|d| {
        let a = d.join("a");
        std::fs::create_dir_all(&a).unwrap();
        projects::init(&a, Some("iyi")).unwrap();
        std::fs::write(a.join("primum.toml"), "isim = 3\n").unwrap();
        let l = projects::list().unwrap();
        assert!(l[0].project.as_ref().unwrap_err().contains("primum.toml"));
    });
}
