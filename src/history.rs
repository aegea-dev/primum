//! A project's history: a bare git repository at `.primum/history.git`
//! whose work tree is the project folder. Every `git` call primum makes is
//! in this module.
use crate::store::Project;
use anyhow::{Context, Result, bail};
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

/// What the history tracks: primum's files and nothing else.
const EXCLUDE: &str = "/*\n!/project.json\n!/primum.toml\n!/BOARD.md\n!/items/\n/items/*\n!/items/*.md\n!/conflicts/\n";

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
        } else if std::fs::read_to_string(h.git_dir.join("info/exclude"))
            .ok()
            .as_deref()
            != Some(EXCLUDE)
        {
            // a history made by an earlier version tracks what this one does
            h.write_exclude()?;
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

/// A path the history API may name: project.json, primum.toml, BOARD.md, or a file
/// directly in items/ or conflicts/.
fn tracked_path(path: &str) -> bool {
    let in_dir = |d: &str| {
        path.strip_prefix(d)
            .is_some_and(|n| !n.is_empty() && !n.contains('/') && !n.contains(".."))
    };
    path == "project.json"
        || path == "primum.toml"
        || path == "BOARD.md"
        || in_dir("items/")
        || in_dir("conflicts/")
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

/// This machine's remote for the project, from the history's git config.
#[derive(Debug, Clone, PartialEq)]
pub struct Remote {
    pub url: String,
    pub branch: String,
    /// Sync only with `primum sync`.
    pub manual: bool,
}

pub enum Fetched {
    Ok,
    /// The remote has no such branch yet.
    Empty,
    Offline(String),
}

pub enum Pushed {
    Ok,
    /// The remote moved since the fetch.
    Rejected,
    Offline(String),
}

const NETWORK_SECS: u64 = 10;

/// Runs `c`, killing it after `secs`; None on timeout. Never prompts.
fn timed(mut c: Command, secs: u64) -> Result<Option<Output>> {
    if std::env::var_os("GIT_SSH_COMMAND").is_none() {
        c.env(
            "GIT_SSH_COMMAND",
            "ssh -o BatchMode=yes -o ConnectTimeout=10",
        );
    }
    let mut child = c
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("cannot run git")?;
    let start = std::time::Instant::now();
    loop {
        if child.try_wait()?.is_some() {
            return Ok(Some(child.wait_with_output()?));
        }
        if start.elapsed() > std::time::Duration::from_secs(secs) {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(None);
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

fn stderr_of(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).trim().to_string()
}

impl History {
    pub fn run_timed(&self, args: &[&str], secs: u64) -> Result<Option<Output>> {
        let mut c = self.command();
        c.args(args);
        timed(c, secs)
    }

    fn config(&self, key: &str) -> Result<Option<String>> {
        let out = self.run(&["config", "--get", key])?;
        Ok(out
            .status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string()))
    }

    pub fn remote(&self) -> Result<Option<Remote>> {
        let Some(url) = self.config("remote.origin.url")? else {
            return Ok(None);
        };
        Ok(Some(Remote {
            url,
            branch: self
                .config("primum.branch")?
                .unwrap_or_else(|| "main".into()),
            manual: self.config("primum.sync")?.as_deref() == Some("manual"),
        }))
    }

    /// Sets this machine's remote; the branch keeps its current setting
    /// (default `main`) unless given, and the local branch takes its name.
    pub fn set_remote(&self, url: &str, branch: Option<&str>, manual: bool) -> Result<Remote> {
        let branch = match branch {
            Some(b) => b.to_string(),
            None => self
                .config("primum.branch")?
                .unwrap_or_else(|| "main".into()),
        };
        if self.config("remote.origin.url")?.is_some() {
            self.git(&["remote", "set-url", "origin", url])?;
        } else {
            self.git(&["remote", "add", "origin", url])?;
        }
        let refspec = format!("+refs/heads/{branch}:refs/remotes/origin/{branch}");
        self.git(&["config", "remote.origin.fetch", &refspec])?;
        self.git(&["config", "primum.branch", &branch])?;
        let mode = if manual { "manual" } else { "auto" };
        self.git(&["config", "primum.sync", mode])?;
        let current = self.git(&["symbolic-ref", "--short", "HEAD"])?;
        if current.trim() != branch {
            if self.head()?.is_some() {
                self.git(&["branch", "-m", current.trim(), &branch])?;
            } else {
                self.git(&["symbolic-ref", "HEAD", &format!("refs/heads/{branch}")])?;
            }
        }
        Ok(Remote {
            url: url.into(),
            branch,
            manual,
        })
    }

    pub fn remove_remote(&self) -> Result<()> {
        if self.config("remote.origin.url")?.is_some() {
            self.git(&["remote", "remove", "origin"])?;
        }
        let _ = self.run(&["config", "--unset", "primum.sync"])?;
        Ok(())
    }

    pub fn fetch(&self, r: &Remote) -> Result<Fetched> {
        let refspec = format!("+refs/heads/{0}:refs/remotes/origin/{0}", r.branch);
        let args = ["fetch", "--quiet", "origin", &refspec];
        let Some(out) = self.run_timed(&args, NETWORK_SECS)? else {
            return Ok(Fetched::Offline("the remote did not answer".into()));
        };
        if out.status.success() {
            return Ok(Fetched::Ok);
        }
        let err = stderr_of(&out);
        if err.contains("couldn't find remote ref") {
            return Ok(Fetched::Empty);
        }
        Ok(Fetched::Offline(err))
    }

    pub fn push(&self, r: &Remote) -> Result<Pushed> {
        let refspec = format!("HEAD:refs/heads/{}", r.branch);
        let Some(out) = self.run_timed(&["push", "--quiet", "origin", &refspec], NETWORK_SECS)?
        else {
            return Ok(Pushed::Offline("the remote did not answer".into()));
        };
        if out.status.success() {
            return Ok(Pushed::Ok);
        }
        let err = stderr_of(&out);
        if err.contains("rejected")
            || err.contains("fetch first")
            || err.contains("non-fast-forward")
        {
            return Ok(Pushed::Rejected);
        }
        Ok(Pushed::Offline(err))
    }
}

/// Makes `folder` a project from a remote: a bare clone of its history and
/// the files written from it. The caller lists the project.
pub fn clone(url: &str, branch: Option<&str>, folder: &std::path::Path) -> Result<()> {
    if folder.join(".primum").exists()
        || folder.join("project.json").exists()
        || folder.join("primum.toml").exists()
    {
        bail!("{} is already a primum project", folder.display());
    }
    std::fs::create_dir_all(folder.join(".primum"))?;
    let h = History {
        git_dir: folder.join(".primum/history.git"),
        work_tree: folder.to_path_buf(),
    };
    let mut c = Command::new(git_binary());
    c.args(["clone", "--quiet", "--bare"]);
    if let Some(b) = branch {
        c.args(["--branch", b]);
    }
    c.arg(url).arg(&h.git_dir);
    let out = timed(c, 60)?.ok_or_else(|| anyhow::anyhow!("the remote did not answer"))?;
    if let Err(e) = check(out, "clone") {
        let _ = std::fs::remove_dir_all(folder.join(".primum"));
        return Err(e);
    }
    std::fs::write(folder.join(".primum/.gitignore"), "*\n")?;
    h.write_exclude()?;
    let branch = h
        .git(&["symbolic-ref", "--short", "HEAD"])?
        .trim()
        .to_string();
    let refspec = format!("+refs/heads/{branch}:refs/remotes/origin/{branch}");
    h.git(&["config", "remote.origin.fetch", &refspec])?;
    h.git(&["config", "primum.branch", &branch])?;
    h.git(&["config", "primum.sync", "auto"])?;
    h.git(&["fetch", "--quiet", "origin"])?;
    h.git(&["read-tree", "-u", "--reset", "HEAD"])?;
    if !folder.join("project.json").is_file() && !folder.join("primum.toml").is_file() {
        bail!("{url} has no project.json on {branch}: not a primum project");
    }
    Ok(())
}

impl History {
    /// A path's text at merge stage `n` (1 base, 2 ours, 3 theirs), None
    /// when that side has no such file.
    pub fn stage(&self, n: u8, path: &str) -> Result<Option<String>> {
        let out = self.run(&["show", &format!(":{n}:{path}")])?;
        Ok(out
            .status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).to_string()))
    }

    /// git merge-file on three texts: the merge, or None on a conflict.
    pub fn merge_text(&self, base: &str, ours: &str, theirs: &str) -> Result<Option<String>> {
        let dir = self.git_dir.join("primum-merge");
        std::fs::create_dir_all(&dir)?;
        let (o, b, t) = (dir.join("ours"), dir.join("base"), dir.join("theirs"));
        std::fs::write(&o, ours)?;
        std::fs::write(&b, base)?;
        std::fs::write(&t, theirs)?;
        let out = Command::new(git_binary())
            .args(["merge-file", "-p", "--quiet"])
            .args([&o, &b, &t])
            .stdin(Stdio::null())
            .output()?;
        let _ = std::fs::remove_dir_all(&dir);
        match out.status.code() {
            Some(0) => Ok(Some(String::from_utf8_lossy(&out.stdout).to_string())),
            Some(n) if n > 0 => Ok(None),
            _ => bail!("git merge-file failed: {}", stderr_of(&out)),
        }
    }

    /// Who last changed `path` on `rev`, and when.
    pub fn side(&self, rev: &str, path: &str) -> Result<crate::conflicts::Side> {
        let out = self.git(&["log", "-1", "--format=%an%x1f%aI", rev, "--", path])?;
        let (author, date) = out.trim().split_once('\x1f').unwrap_or(("", ""));
        Ok(crate::conflicts::Side {
            author: author.into(),
            date: date.into(),
        })
    }
}
