//! A project's history: a bare git repository at `.primum/history.git`
//! whose work tree is the project folder. Every `git` call primum makes is
//! in this module.
use crate::store::Project;
use anyhow::{Context, Result, bail};
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

/// What the history tracks: primum's files and nothing else.
const EXCLUDE: &str =
    "/*\n!/primum.toml\n!/BOARD.md\n!/items/\n/items/*\n!/items/*.md\n!/conflicts/\n";

pub fn git_binary() -> String {
    std::env::var("PRIMUM_GIT").unwrap_or_else(|_| "git".into())
}

pub fn git_available() -> bool {
    Command::new(git_binary())
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[derive(Debug, Clone)]
pub struct History {
    pub git_dir: PathBuf,
    pub work_tree: PathBuf,
}

pub(crate) fn check(out: Output, what: &str) -> Result<String> {
    if !out.status.success() {
        bail!(
            "git {what} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

impl History {
    pub fn dir(p: &Project) -> PathBuf {
        p.root.join(".primum").join("history.git")
    }

    /// The project's history, created when missing with a first commit
    /// (`start history`) of what is already there. None when git is not
    /// installed.
    pub fn open(p: &Project) -> Result<Option<History>> {
        History::open_with(p, "start history")
    }

    pub fn open_with(p: &Project, first_message: &str) -> Result<Option<History>> {
        if !git_available() {
            return Ok(None);
        }
        let h = History {
            git_dir: History::dir(p),
            work_tree: p.root.clone(),
        };
        if !h.git_dir.join("HEAD").exists() {
            h.create()?;
            h.commit(first_message)?;
        }
        Ok(Some(h))
    }

    fn create(&self) -> Result<()> {
        std::fs::create_dir_all(&self.git_dir)?;
        std::fs::write(self.work_tree.join(".primum/.gitignore"), "*\n")?;
        let out = Command::new(git_binary())
            .args(["init", "--quiet", "--bare", "-b", "main"])
            .arg(&self.git_dir)
            .stdin(Stdio::null())
            .output()
            .context("cannot run git")?;
        check(out, "init")?;
        self.write_exclude()
    }

    pub(crate) fn write_exclude(&self) -> Result<()> {
        std::fs::create_dir_all(self.git_dir.join("info"))?;
        std::fs::write(self.git_dir.join("info/exclude"), EXCLUDE)?;
        Ok(())
    }

    /// A git command on this history, with primum's fixed settings: a
    /// person's hooks and signing never run on primum's commits.
    pub fn command(&self) -> Command {
        let mut c = Command::new(git_binary());
        c.arg("--git-dir")
            .arg(&self.git_dir)
            .arg("--work-tree")
            .arg(&self.work_tree)
            .args([
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.quotePath=false",
                "-c",
                "core.autocrlf=false",
            ])
            .current_dir(&self.work_tree)
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(Stdio::null());
        c
    }

    /// Runs git; any exit code is returned to the caller.
    pub fn run(&self, args: &[&str]) -> Result<Output> {
        self.command().args(args).output().context("cannot run git")
    }

    /// Runs git and returns its output; a failure is an error.
    pub fn git(&self, args: &[&str]) -> Result<String> {
        check(self.run(args)?, args.first().copied().unwrap_or(""))
    }

    /// Runs a git command that makes a commit, as the person's configured
    /// identity, else as `primum <primum@localhost>`.
    pub fn git_as_author(&self, args: &[&str]) -> Result<String> {
        let mut c = self.command();
        if !self.has_identity() {
            c.args([
                "-c",
                "user.name=primum",
                "-c",
                "user.email=primum@localhost",
            ]);
        }
        let out = c.args(args).output().context("cannot run git")?;
        check(out, args.first().copied().unwrap_or(""))
    }

    fn has_identity(&self) -> bool {
        self.run(&["config", "user.email"])
            .map(|o| o.status.success() && !o.stdout.is_empty())
            .unwrap_or(false)
    }

    pub fn head(&self) -> Result<Option<String>> {
        let out = self.run(&["rev-parse", "--verify", "--quiet", "HEAD"])?;
        Ok(out
            .status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string()))
    }

    /// Commits everything primum tracks that changed. Returns false when
    /// there was nothing to commit.
    pub fn commit(&self, message: &str) -> Result<bool> {
        self.git(&["add", "--all", "--", "."])?;
        if self.run(&["diff", "--cached", "--quiet"])?.status.success() {
            return Ok(false);
        }
        self.git_as_author(&["commit", "--quiet", "--no-verify", "-m", message])?;
        Ok(true)
    }

    /// Commits edits made outside primum since the last commit, apart from
    /// the change that follows.
    pub fn record_outside(&self) -> Result<bool> {
        self.commit("edited outside primum")
    }
}

/// One commit, as `primum log` and the History tab show it.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Entry {
    pub commit: String,
    pub short: String,
    pub author: String,
    pub date: String,
    pub message: String,
    /// The item's file at that commit, when the log is of one item.
    pub path: Option<String>,
}

/// A path the history API may name: primum.toml, BOARD.md, or a file
/// directly in items/ or conflicts/.
fn tracked_path(path: &str) -> bool {
    let in_dir = |d: &str| {
        path.strip_prefix(d)
            .is_some_and(|n| !n.is_empty() && !n.contains('/') && !n.contains(".."))
    };
    path == "primum.toml" || path == "BOARD.md" || in_dir("items/") || in_dir("conflicts/")
}

impl History {
    /// Commits newest first: all of them, or those touching `path`,
    /// following renames.
    pub fn log(&self, path: Option<&str>, limit: usize) -> Result<Vec<Entry>> {
        if self.head()?.is_none() {
            return Ok(vec![]);
        }
        let n = format!("-n{limit}");
        let mut args = vec![
            "log",
            n.as_str(),
            "--format=%x1e%H%x1f%h%x1f%an%x1f%aI%x1f%s",
        ];
        if let Some(p) = path {
            if !tracked_path(p) {
                bail!("{p} is not a file primum tracks");
            }
            args.extend(["--name-only", "--follow", "--", p]);
        }
        let out = self.git(&args)?;
        Ok(out
            .split('\x1e')
            .filter(|r| !r.trim().is_empty())
            .filter_map(|r| {
                let mut lines = r.lines();
                let f: Vec<&str> = lines.next()?.split('\x1f').collect();
                let path = lines
                    .map(str::trim)
                    .find(|l| !l.is_empty())
                    .map(str::to_string);
                Some(Entry {
                    commit: f.first()?.to_string(),
                    short: f.get(1)?.to_string(),
                    author: f.get(2)?.to_string(),
                    date: f.get(3)?.to_string(),
                    message: f.get(4)?.to_string(),
                    path,
                })
            })
            .collect())
    }

    /// What `commit` changed in `path` (the path as it was at that commit).
    pub fn diff(&self, commit: &str, path: &str) -> Result<String> {
        if commit.len() < 7 || !commit.bytes().all(|b| b.is_ascii_hexdigit()) {
            bail!("{commit} is not a commit");
        }
        if !tracked_path(path) {
            bail!("{path} is not a file primum tracks");
        }
        self.git(&["show", "--format=", "--find-renames", commit, "--", path])
    }
}
