//! Which projects exist (each one's `primum.toml`) and which this machine
//! shows (`~/.config/primum/projects.toml`, paths only, in tab order).
use crate::store::Project;
use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectFile {
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    github: Option<GithubTable>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GithubTable {
    repo: String,
    /// How ids show on issues: none, title, label or both.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ids: Option<String>,
}

#[derive(Serialize, Deserialize, Default)]
struct ListFile {
    #[serde(default)]
    projects: Vec<String>,
}

/// A listed path, and its project or why it is unavailable.
pub struct Entry {
    pub path: PathBuf,
    pub project: Result<Project, String>,
}

pub fn config_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".config"));
    base.join("primum").join("projects.toml")
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
}

fn expand(p: &str) -> PathBuf {
    match p.strip_prefix("~/") {
        Some(rest) => home().join(rest),
        None => PathBuf::from(p),
    }
}

/// A path as the list writes it and the board shows it: under the home
/// folder as `~/...`.
pub fn contract(p: &Path) -> String {
    let home = home();
    match p.strip_prefix(&home) {
        Ok(rest) if !home.as_os_str().is_empty() => format!("~/{}", rest.display()),
        _ => p.display().to_string(),
    }
}

fn read_list() -> Result<ListFile> {
    let path = config_path();
    if !path.exists() {
        return Ok(ListFile::default());
    }
    let text = std::fs::read_to_string(&path)?;
    toml::from_str(&text).with_context(|| format!("{} is not valid", path.display()))
}

fn write_list(l: &ListFile) -> Result<()> {
    let path = config_path();
    std::fs::create_dir_all(path.parent().expect("the config path has a folder"))?;
    std::fs::write(&path, toml::to_string(l)?)?;
    Ok(())
}

/// The project in `dir`, or why there is none.
pub fn open(dir: &Path) -> Result<Project, String> {
    let file = dir.join("primum.toml");
    let text = std::fs::read_to_string(&file)
        .map_err(|_| format!("no primum.toml in {}", dir.display()))?;
    let pf: ProjectFile = toml::from_str(&text).map_err(|e| format!("{}: {e}", file.display()))?;
    if !dir.join("items").is_dir() {
        return Err(format!("no items/ in {}", dir.display()));
    }
    Ok(Project {
        root: dir.to_path_buf(),
        name: pf.name,
    })
}

/// The listed projects in tab order. Two available projects with one name
/// are refused: the web UI and `-p` tell projects apart by name.
pub fn list() -> Result<Vec<Entry>> {
    let entries: Vec<Entry> = read_list()?
        .projects
        .iter()
        .map(|p| {
            let path = expand(p);
            Entry {
                project: open(&path),
                path,
            }
        })
        .collect();
    let mut seen = std::collections::HashSet::new();
    for p in entries.iter().filter_map(|e| e.project.as_ref().ok()) {
        if !seen.insert(p.name.clone()) {
            bail!(
                "two listed projects are both named {}; rename one in its primum.toml",
                p.name
            );
        }
    }
    Ok(entries)
}

/// Lists the project in `dir` on this machine.
pub fn add(dir: &Path) -> Result<Project> {
    let dir = dir
        .canonicalize()
        .with_context(|| format!("no folder {}", dir.display()))?;
    let p = open(&dir).map_err(|e| anyhow!(e))?;
    let mut l = read_list()?;
    if l.projects.iter().any(|x| expand(x) == dir) {
        return Ok(p);
    }
    for e in list()? {
        if let Ok(q) = e.project
            && q.name == p.name
        {
            bail!(
                "a project named {} is already listed, in {}",
                p.name,
                q.root.display()
            );
        }
    }
    l.projects.push(contract(&dir));
    write_list(&l)?;
    Ok(p)
}

/// Makes `dir` a project (`primum.toml` and an empty `items/`) and lists it.
/// The name defaults to the folder's.
pub fn init(dir: &Path, name: Option<&str>) -> Result<Project> {
    if dir.join("primum.toml").exists() {
        bail!("{} is already a primum project", dir.display());
    }
    let name = match name {
        Some(n) => n.to_string(),
        None => dir
            .canonicalize()?
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .ok_or_else(|| anyhow!("name the project with --name"))?,
    };
    if name.trim().is_empty() || name.contains('/') {
        bail!("a project name is not empty and has no /");
    }
    if let Some(q) = list()?
        .into_iter()
        .filter_map(|e| e.project.ok())
        .find(|q| q.name == name)
    {
        bail!(
            "a project named {name} is already listed, in {}; choose another with --name",
            q.root.display()
        );
    }
    std::fs::create_dir_all(dir.join("items"))?;
    std::fs::write(
        dir.join("primum.toml"),
        toml::to_string(&ProjectFile { name, github: None })?,
    )?;
    let p = open(dir).map_err(|e| anyhow!(e))?;
    crate::history::History::open_with(&p, &format!("primum project {}", p.name))?;
    add(dir)
}

/// Takes a project off this machine's list, by name or by its listed path.
/// The project itself is never touched.
pub fn remove(name_or_path: &str) -> Result<()> {
    let mut l = read_list()?;
    let before = l.projects.len();
    let as_path = expand(name_or_path);
    l.projects.retain(|p| {
        let path = expand(p);
        let named = open(&path).map(|q| q.name == name_or_path).unwrap_or(false);
        !(named || path == as_path)
    });
    if l.projects.len() == before {
        bail!("no listed project named {name_or_path}");
    }
    write_list(&l)
}

/// The project a command works on: the one named, else the one containing
/// `cwd`, else the only one listed.
pub fn resolve(name: Option<&str>, cwd: &Path) -> Result<Project> {
    let ok: Vec<Project> = list()?.into_iter().filter_map(|e| e.project.ok()).collect();
    if let Some(n) = name {
        return ok
            .into_iter()
            .find(|p| p.name == n)
            .ok_or_else(|| anyhow!("no listed project named {n}"));
    }
    let cwd = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
    let inside = ok
        .iter()
        .filter(|p| cwd.starts_with(p.root.canonicalize().unwrap_or_else(|_| p.root.clone())))
        .max_by_key(|p| p.root.components().count());
    if let Some(p) = inside {
        return Ok(p.clone());
    }
    match ok.len() {
        1 => Ok(ok.into_iter().next().expect("one project")),
        0 => bail!("no projects yet: run `primum init` in a folder"),
        _ => bail!("several projects are listed; choose one with -p NAME"),
    }
}

fn read_project_file(p: &Project) -> Result<ProjectFile> {
    let file = p.root.join("primum.toml");
    let text = std::fs::read_to_string(&file)?;
    toml::from_str(&text).with_context(|| format!("{} is not valid", file.display()))
}

/// The GitHub repository the project is kept in step with, if any.
pub fn github_repo(p: &Project) -> Result<Option<String>> {
    if !p.root.join("primum.toml").exists() {
        return Ok(None);
    }
    Ok(read_project_file(p)?.github.map(|g| g.repo))
}

/// Connects the project to `owner/repo`, or disconnects it with None.
pub fn set_github(p: &Project, repo: Option<&str>) -> Result<()> {
    if let Some(r) = repo {
        let ok = r
            .split_once('/')
            .is_some_and(|(o, n)| !o.is_empty() && !n.is_empty() && !n.contains('/'));
        if !ok {
            bail!("name the repository as owner/repo, not {r}");
        }
    }
    let mut pf = read_project_file(p)?;
    let ids = pf.github.as_ref().and_then(|g| g.ids.clone());
    pf.github = repo.map(|r| GithubTable {
        repo: r.to_string(),
        ids,
    });
    std::fs::write(p.root.join("primum.toml"), toml::to_string(&pf)?)?;
    Ok(())
}

/// How the project's ids show on GitHub issues (`[github] ids`).
pub fn github_ids(p: &Project) -> Result<crate::github::IdStyle> {
    if !p.root.join("primum.toml").exists() {
        return Ok(Default::default());
    }
    let ids = read_project_file(p)?.github.and_then(|g| g.ids);
    match ids {
        None => Ok(Default::default()),
        Some(s) => crate::github::IdStyle::parse(&s)
            .ok_or_else(|| anyhow!("[github] ids must be none, title, label or both, not {s}")),
    }
}
