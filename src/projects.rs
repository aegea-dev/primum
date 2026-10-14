//! Where projects are, found from the folder primum runs in: nothing is
//! machine-wide.
//!
//! - A **project** is a folder with `project.json` (its name and settings,
//!   written by primum, tracked in its history) and `items/`.
//! - A **workspace** is a folder with `primum.toml` (written by people)
//!   whose subfolders are projects, optionally listed in tab order.
//!
//! Projects made by earlier versions keep their settings in a `primum.toml`
//! holding `name`; they are read as projects until `primum workspace
//! migrate` (or the next settings change) moves them to `project.json`.
use crate::store::Project;
use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

/// A project's settings: `project.json`, a JSON object whose unknown keys
/// are kept, so a newer primum's settings survive an older one.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings(Map<String, Value>);

impl Settings {
    pub fn new(name: &str) -> Settings {
        let mut m = Map::new();
        m.insert("name".into(), name.into());
        Settings(m)
    }

    /// `project.json`, else a legacy `primum.toml` with `name` (converted),
    /// else None.
    pub fn read(dir: &Path) -> Result<Option<Settings>> {
        let json = dir.join("project.json");
        if json.is_file() {
            let text = std::fs::read_to_string(&json)?;
            let v: Value = serde_json::from_str(&text)
                .with_context(|| format!("{} is not valid JSON", json.display()))?;
            let Value::Object(m) = v else {
                bail!("{} must hold a JSON object", json.display());
            };
            return Ok(Some(Settings(m)));
        }
        let legacy = dir.join("primum.toml");
        let Ok(text) = std::fs::read_to_string(&legacy) else {
            return Ok(None);
        };
        let t: toml::Table =
            toml::from_str(&text).with_context(|| format!("{} is not valid", legacy.display()))?;
        let Some(name) = t.get("name").and_then(|v| v.as_str()) else {
            return Ok(None); // a workspace file, not a project
        };
        Ok(Some(Settings::new(name)))
    }

    /// Writes `project.json`, and removes a legacy `primum.toml` holding
    /// `name`, so there is one source.
    pub fn write(&self, dir: &Path) -> Result<()> {
        let text = serde_json::to_string_pretty(&self.0)? + "\n";
        crate::store::write_atomic(&dir.join("project.json"), &text)?;
        if is_legacy(dir) {
            std::fs::remove_file(dir.join("primum.toml"))?;
        }
        Ok(())
    }

    pub fn name(&self) -> Option<&str> {
        self.0.get("name").and_then(|v| v.as_str())
    }

    /// The project's stored context variables.
    pub fn context(&self) -> Map<String, Value> {
        match self.0.get("context") {
            Some(Value::Object(m)) => m.clone(),
            _ => Map::new(),
        }
    }

    /// Stores a context variable, or removes it with None.
    pub fn set_context(&mut self, key: &str, value: Option<Value>) {
        let mut c = self.context();
        match value {
            Some(v) => {
                c.insert(key.to_string(), v);
            }
            None => {
                c.remove(key);
            }
        }
        if c.is_empty() {
            self.0.remove("context");
        } else {
            self.0.insert("context".into(), Value::Object(c));
        }
    }

    /// The lifecycle the project itself follows, `<name>@<version>`.
    pub fn lifecycle(&self) -> Option<String> {
        self.0
            .get("lifecycle")
            .and_then(|v| v.as_str())
            .map(str::to_string)
    }
}

/// A project's stored context variables (`project.json`'s `context`).
pub fn context(p: &Project) -> Result<Map<String, Value>> {
    Ok(Settings::read(&p.root)?
        .map(|s| s.context())
        .unwrap_or_default())
}

/// Stores (or with None removes) a context variable, as one commit.
pub fn set_context(p: &Project, key: &str, value: Option<Value>) -> Result<()> {
    let message = match &value {
        Some(_) => format!("context: set {key}"),
        None => format!("context: unset {key}"),
    };
    crate::store::change_settings(p, &message, || {
        let mut s = Settings::read(&p.root)?.unwrap_or_else(|| Settings::new(&p.name));
        s.set_context(key, value);
        s.write(&p.root)
    })
}

/// The lifecycle the project follows, if any.
pub fn project_lifecycle(p: &Project) -> Result<Option<String>> {
    Ok(Settings::read(&p.root)?.and_then(|s| s.lifecycle()))
}

/// `primum.toml` in `dir` is a project's file from an earlier version.
fn is_legacy(dir: &Path) -> bool {
    std::fs::read_to_string(dir.join("primum.toml"))
        .ok()
        .and_then(|t| toml::from_str::<toml::Table>(&t).ok())
        .is_some_and(|t| t.contains_key("name"))
}

/// A workspace's subfolder, and its project or why it is unavailable.
pub struct Entry {
    pub path: PathBuf,
    pub project: Result<Project, String>,
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
}

/// A path as the board shows it: under the home folder as `~/...`.
pub fn contract(p: &Path) -> String {
    let home = home();
    match p.strip_prefix(&home) {
        Ok(rest) if !home.as_os_str().is_empty() => format!("~/{}", rest.display()),
        _ => p.display().to_string(),
    }
}

/// The project in `dir`, or why there is none.
pub fn open(dir: &Path) -> Result<Project, String> {
    let s = Settings::read(dir)
        .map_err(|e| format!("{e:#}"))?
        .ok_or_else(|| format!("no project.json in {}", dir.display()))?;
    if !dir.join("items").is_dir() {
        return Err(format!("no items/ in {}", dir.display()));
    }
    let name = s
        .name()
        .filter(|n| !n.trim().is_empty())
        .ok_or_else(|| format!("{} has no name", dir.join("project.json").display()))?;
    Ok(Project {
        root: dir.to_path_buf(),
        name: name.to_string(),
    })
}

fn is_project_dir(dir: &Path) -> bool {
    dir.join("items").is_dir() && Settings::read(dir).ok().flatten().is_some()
}

/// The subfolders of `root` holding a project, by name.
fn project_subfolders(root: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter(|e| is_project_dir(&e.path()))
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    v.sort();
    v
}

fn check_name(name: &str) -> Result<()> {
    if name.trim().is_empty() || name.contains('/') || name.contains('\\') || name == ".." {
        bail!("a project name is not empty and has no / or \\");
    }
    Ok(())
}

/// A folder holding `primum.toml` (without `name`): its subfolders are
/// projects, optionally listed in tab order.
#[derive(Debug, Clone)]
pub struct Workspace {
    pub root: PathBuf,
    pub listed: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct WorkspaceFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    projects: Option<Vec<String>>,
}

impl Workspace {
    fn read(root: &Path) -> Option<Workspace> {
        let text = std::fs::read_to_string(root.join("primum.toml")).ok()?;
        if is_legacy(root) {
            return None;
        }
        let wf: WorkspaceFile = toml::from_str(&text).ok()?;
        Some(Workspace {
            root: root.to_path_buf(),
            listed: wf.projects,
        })
    }

    /// The nearest workspace at or above `from`.
    pub fn find(from: &Path) -> Option<Workspace> {
        let from = from.canonicalize().unwrap_or_else(|_| from.to_path_buf());
        from.ancestors().find_map(Workspace::read)
    }

    /// Makes `root` a workspace listing the projects in its subfolders.
    pub fn create(root: &Path) -> Result<Workspace> {
        if Workspace::read(root).is_some() {
            bail!("{} is already a primum workspace", root.display());
        }
        if is_project_dir(root) {
            bail!(
                "{} is a project; a workspace holds projects in its subfolders",
                root.display()
            );
        }
        let ws = Workspace {
            root: root.to_path_buf(),
            listed: Some(project_subfolders(root)),
        };
        ws.save()?;
        Ok(ws)
    }

    fn save(&self) -> Result<()> {
        let wf = WorkspaceFile {
            projects: self.listed.clone(),
        };
        crate::store::write_atomic(&self.root.join("primum.toml"), &toml::to_string(&wf)?)
    }

    /// The projects in tab order: the list, else every project subfolder by
    /// name. A listed folder that is missing or broken, or a second project
    /// with a name already shown, is an `Err` entry.
    pub fn entries(&self) -> Vec<Entry> {
        let subs = match &self.listed {
            Some(l) => l.clone(),
            None => project_subfolders(&self.root),
        };
        let mut seen = std::collections::HashSet::new();
        subs.iter()
            .map(|s| {
                let path = self.root.join(s);
                let project = open(&path).and_then(|p| {
                    if seen.insert(p.name.clone()) {
                        Ok(p)
                    } else {
                        Err(format!("another project is named {}; rename one", p.name))
                    }
                });
                Entry { path, project }
            })
            .collect()
    }

    fn taken(&self, name: &str, except: &Path) -> Option<Project> {
        self.entries()
            .into_iter()
            .filter_map(|e| e.project.ok())
            .find(|q| q.name == name && q.root != except)
    }

    /// Adds the subfolder `sub` to the list (when the workspace has one).
    pub fn add(&self, sub: &str) -> Result<Project> {
        check_name(sub)
            .map_err(|_| anyhow!("a workspace lists its subfolders by name, not {sub}"))?;
        let p = open(&self.root.join(sub)).map_err(|e| anyhow!(e))?;
        if let Some(q) = self.taken(&p.name, &p.root) {
            bail!(
                "a project named {} is already in this workspace, in {}",
                p.name,
                q.root.display()
            );
        }
        if let Some(mut l) = self.listed.clone()
            && !l.iter().any(|x| x == sub)
        {
            l.push(sub.to_string());
            Workspace {
                listed: Some(l),
                ..self.clone()
            }
            .save()?;
        }
        Ok(p)
    }

    /// Makes the subfolder `sub` a project named `name` (default: `sub`) and
    /// adds it.
    pub fn init_project(&self, sub: &str, name: Option<&str>) -> Result<Project> {
        check_name(sub)?;
        let name = name.unwrap_or(sub);
        let dir = self.root.join(sub);
        if let Some(q) = self.taken(name, &dir) {
            bail!(
                "a project named {name} is already in this workspace, in {}; choose another with --name",
                q.root.display()
            );
        }
        init(&dir, Some(name))?;
        self.add(sub)
    }

    /// Takes a project off the list, by name or subfolder; the project itself
    /// is never touched.
    pub fn remove(&self, name: &str) -> Result<()> {
        let Some(l) = self.listed.clone() else {
            bail!("this workspace lists no projects: every subfolder with project.json counts");
        };
        let kept: Vec<String> = l
            .iter()
            .filter(|s| {
                let named = open(&self.root.join(s)).is_ok_and(|p| p.name == name);
                !(named || s.as_str() == name)
            })
            .cloned()
            .collect();
        if kept.len() == l.len() {
            bail!("no project named {name} in this workspace");
        }
        Workspace {
            listed: Some(kept),
            ..self.clone()
        }
        .save()
    }
}

/// The nearest project at or above `from`.
fn project_above(from: &Path) -> Option<Project> {
    let from = from.canonicalize().unwrap_or_else(|_| from.to_path_buf());
    from.ancestors()
        .find(|d| is_project_dir(d))
        .and_then(|d| open(d).ok())
}

/// The workspace `cwd` is in: above its project, else above `cwd`.
pub fn workspace(cwd: &Path) -> Option<Workspace> {
    match project_above(cwd) {
        Some(p) => p.root.parent().and_then(Workspace::find),
        None => Workspace::find(cwd),
    }
}

/// The projects a command or the board sees from `cwd`: the workspace's,
/// else the one project `cwd` is in, else none.
pub fn entries(cwd: &Path) -> Vec<Entry> {
    if let Some(ws) = workspace(cwd) {
        return ws.entries();
    }
    project_above(cwd)
        .map(|p| {
            vec![Entry {
                path: p.root.clone(),
                project: Ok(p),
            }]
        })
        .unwrap_or_default()
}

/// The project a command works on: the one named (in the workspace), else
/// the one `cwd` is in, else the workspace's only one.
pub fn resolve(name: Option<&str>, cwd: &Path) -> Result<Project> {
    let all: Vec<Project> = entries(cwd)
        .into_iter()
        .filter_map(|e| e.project.ok())
        .collect();
    if let Some(n) = name {
        return all.into_iter().find(|p| p.name == n).ok_or_else(|| {
            anyhow!("no project named {n} here: primum looks in the workspace you are in")
        });
    }
    if let Some(p) = project_above(cwd) {
        return Ok(p);
    }
    match all.len() {
        1 => Ok(all.into_iter().next().expect("one project")),
        0 => bail!("no primum project here: cd into a board or a workspace, or run `primum init`"),
        _ => {
            let names: Vec<&str> = all.iter().map(|p| p.name.as_str()).collect();
            bail!("choose a project with -p NAME: {}", names.join(", "))
        }
    }
}

/// Makes `dir` a project: `project.json`, `items/`, its history. The name
/// defaults to the folder's.
pub fn init(dir: &Path, name: Option<&str>) -> Result<Project> {
    if Settings::read(dir)?.is_some() {
        bail!("{} is already a primum project", dir.display());
    }
    if Workspace::read(dir).is_some() {
        bail!(
            "{} is a workspace; make a project in it with `primum init NAME`",
            dir.display()
        );
    }
    std::fs::create_dir_all(dir.join("items"))?;
    let name = match name {
        Some(n) => n.to_string(),
        None => dir
            .canonicalize()?
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .ok_or_else(|| anyhow!("name the project with --name"))?,
    };
    check_name(&name)?;
    Settings::new(&name).write(dir)?;
    let p = open(dir).map_err(|e| anyhow!(e))?;
    crate::history::History::open_with(&p, &format!("primum project {}", p.name))?;
    Ok(p)
}

/// The order of the machine's old central list, to order a migrated
/// workspace the way its tabs were. Read by `migrate` only.
fn old_list_order() -> Vec<PathBuf> {
    let file = home().join(".config/primum/projects.toml");
    #[derive(Deserialize)]
    struct Old {
        #[serde(default)]
        projects: Vec<String>,
    }
    let Some(old) = std::fs::read_to_string(file)
        .ok()
        .and_then(|t| toml::from_str::<Old>(&t).ok())
    else {
        return vec![];
    };
    old.projects
        .iter()
        .map(|p| match p.strip_prefix("~/") {
            Some(rest) => home().join(rest),
            None => PathBuf::from(p),
        })
        .map(|p| p.canonicalize().unwrap_or(p))
        .collect()
}

/// Converts boards made by earlier versions in the subfolders of `root`:
/// each legacy `primum.toml` becomes `project.json` (committed in that
/// project's history), and `root` becomes a workspace listing them, in the
/// old central list's order where it has them, else by name. Returns what
/// changed; nothing when run twice.
pub fn migrate(root: &Path) -> Result<Vec<String>> {
    let mut done = vec![];
    let mut subs: Vec<String> = std::fs::read_dir(root)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().join("items").is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    subs.sort();
    for sub in &subs {
        let dir = root.join(sub);
        if !is_legacy(&dir) || dir.join("project.json").is_file() {
            continue;
        }
        let Some(s) = Settings::read(&dir)? else {
            continue;
        };
        let p = open(&dir).map_err(|e| anyhow!(e))?;
        crate::store::change_settings(&p, "move settings to project.json", || s.write(&dir))?;
        done.push(format!("{sub}: primum.toml -> project.json"));
    }
    if Workspace::read(root).is_none() {
        let order = old_list_order();
        let rank = |s: &String| {
            let path = root.join(s).canonicalize().unwrap_or_else(|_| root.join(s));
            order.iter().position(|o| *o == path).unwrap_or(usize::MAX)
        };
        let mut projects: Vec<String> = subs
            .into_iter()
            .filter(|s| is_project_dir(&root.join(s)))
            .collect();
        projects.sort_by_key(|s| (rank(s), s.clone()));
        let n = projects.len();
        Workspace {
            root: root.to_path_buf(),
            listed: Some(projects),
        }
        .save()?;
        done.push(format!("primum.toml: a workspace of {n} projects"));
    }
    Ok(done)
}
