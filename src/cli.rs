//! The command line. Writes go through `store`, reads through `store::load`
//! and the same `ItemView` the web page gets, so the terminal and the page
//! always agree.
use crate::model::{KINDS, STATUSES, parse_id};
use crate::projects;
use crate::server::{ItemView, views};
use crate::store::{self, Changes, Project};
use anyhow::{Context, Result, anyhow, bail};
use clap::{Args, Parser, Subcommand};
use std::io::{IsTerminal, Read, Write};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "primum",
    version,
    about = "Prioritized work items as plain markdown files"
)]
pub struct Cli {
    /// The project, by name; default: the one containing this folder, or the only one
    #[arg(short, long, global = true)]
    pub project: Option<String>,
    #[command(subcommand)]
    pub cmd: Cmd,
}

#[derive(Args, Default)]
pub struct Scores {
    /// How much it matters, 1-10
    #[arg(long, value_parser = clap::value_parser!(i64).range(1..=10))]
    pub impact: Option<i64>,
    /// How soon it matters, 1-10
    #[arg(long, value_parser = clap::value_parser!(i64).range(1..=10))]
    pub urgency: Option<i64>,
    /// What goes wrong while it waits, 1-10
    #[arg(long, value_parser = clap::value_parser!(i64).range(1..=10))]
    pub risk: Option<i64>,
    /// How much work it is, 1-10
    #[arg(long, value_parser = clap::value_parser!(i64).range(1..=10))]
    pub effort: Option<i64>,
}

#[derive(Subcommand)]
pub enum Cmd {
    /// Make a project: FOLDER in this workspace, or this folder
    Init {
        /// A subfolder of the workspace to make the project in
        folder: Option<String>,
        /// The project's name; default: the folder's
        #[arg(long)]
        name: Option<String>,
    },
    /// Make this folder a workspace, or migrate boards made by earlier versions
    Workspace {
        #[command(subcommand)]
        cmd: WorkspaceCmd,
    },
    /// The workspace's projects, or add or remove one
    Projects {
        #[command(subcommand)]
        cmd: Option<ProjectsCmd>,
    },
    /// Add an item; prints its id
    Add {
        /// URGENT, BUG, DEBT, KILLER or TODO
        kind: String,
        title: String,
        #[command(flatten)]
        scores: Scores,
        /// Ids it waits for, like TODO:1,BUG:2
        #[arg(long)]
        needs: Option<String>,
        /// Its text, from a markdown file (- for standard input)
        #[arg(long)]
        body_file: Option<PathBuf>,
    },
    /// Items in priority order; open and in progress unless told otherwise
    List {
        #[arg(long)]
        kind: Option<String>,
        /// Statuses to show, like open,doing
        #[arg(long)]
        status: Option<String>,
        /// Every item, closed ones too
        #[arg(long)]
        all: bool,
        #[arg(long)]
        json: bool,
    },
    /// One item: scores, needs, text and documents
    Show {
        id: String,
        #[arg(long)]
        json: bool,
    },
    /// Change an item's fields
    Set {
        id: String,
        #[arg(long)]
        title: Option<String>,
        #[command(flatten)]
        scores: Scores,
        /// open, doing, done or wontfix
        #[arg(long)]
        status: Option<String>,
        /// Ids it waits for, like TODO:1,BUG:2 ("" for none)
        #[arg(long)]
        needs: Option<String>,
        /// Replace its text with a markdown file (- for standard input)
        #[arg(long)]
        body_file: Option<PathBuf>,
    },
    /// Mark an item in progress
    Start { id: String },
    /// Mark an item done
    Done { id: String },
    /// Append a dated note to an item's text
    Note {
        id: String,
        /// The note, from a markdown file (- for standard input)
        #[arg(long)]
        file: PathBuf,
    },
    /// Add or replace an item's document, items/<ID>.<TYPE>.md
    Doc {
        id: String,
        /// The document's type, like spec, plan or report
        doc_type: String,
        #[arg(long)]
        file: PathBuf,
    },
    /// Move an open item to another lane; it gets that lane's next id
    Kind { id: String, kind: String },
    /// Delete an item and its documents
    Delete {
        id: String,
        /// Do not ask
        #[arg(long)]
        yes: bool,
    },
    /// What to work on now
    Next {
        #[arg(short, default_value_t = 3)]
        n: usize,
    },
    /// Open the board in the browser
    Serve {
        #[arg(long, default_value_t = 8777)]
        port: u16,
        /// Do not open a browser
        #[arg(long)]
        no_open: bool,
    },
    /// Changes to the project, or to one item, newest first
    Log {
        id: Option<String>,
        #[arg(short, default_value_t = 20)]
        n: usize,
        #[arg(long)]
        json: bool,
    },
    /// Pull, merge and push now
    Sync,
    /// Keep this project's items and a GitHub repository's issues in step
    Github {
        #[command(subcommand)]
        cmd: GithubCmd,
    },
    /// Settle a conflict a sync left: asks per field on a terminal
    Resolve {
        id: String,
        /// A field (status, impact, ...), `body`, or a document's path
        #[arg(long)]
        field: Option<String>,
        /// 1, 2, or `both` (documents)
        #[arg(long)]
        keep: Option<String>,
    },
    /// This machine's remote for the project, or set or remove it
    Remote {
        #[command(subcommand)]
        cmd: Option<RemoteCmd>,
    },
    /// Make a project from a remote, in this workspace when there is one
    Clone {
        url: String,
        /// Default: the remote's name without .git
        folder: Option<PathBuf>,
        #[arg(long)]
        branch: Option<String>,
    },
    /// Import a videre-board board into this (empty) project, once
    Import { board_dir: PathBuf },
}

#[derive(Subcommand)]
pub enum GithubCmd {
    /// Connect to OWNER/REPO: open items become issues, open issues become items
    Connect {
        repo: String,
        /// Also file closed items, and take in closed issues
        #[arg(long)]
        all: bool,
    },
    /// Stop syncing with GitHub; links in items stay
    Disconnect,
    /// The repository, whether gh is ready, and the last sync
    Status,
}

#[derive(Subcommand)]
pub enum RemoteCmd {
    /// Sync with URL; the branch defaults to main
    Set {
        url: String,
        #[arg(long)]
        branch: Option<String>,
        /// Sync only with `primum sync`
        #[arg(long)]
        manual: bool,
    },
    /// Stop syncing; the history stays
    Remove,
}

#[derive(Subcommand)]
pub enum ProjectsCmd {
    /// Add the project in the workspace's subfolder FOLDER to its list
    Add { folder: String },
    /// Take a project off the workspace's list (the project is not touched)
    Remove { name: String },
}

#[derive(Subcommand)]
pub enum WorkspaceCmd {
    /// Make this folder a workspace: primum.toml listing the projects below it
    Init,
    /// Convert boards made by earlier versions in this folder's subfolders
    Migrate,
}

const NOTICE: &str = "primum: ~/.config/primum/projects.toml is no longer used; boards are found from the folder you are in (see primum workspace)";

/// Says once that the old central list is no longer read: a marker beside
/// it remembers that the notice was shown.
fn central_list_notice() {
    let Some(home) = std::env::var_os("HOME").filter(|h| !h.is_empty()) else {
        return;
    };
    let dir = PathBuf::from(home).join(".config/primum");
    let marker = dir.join(".workspace-notice");
    if dir.join("projects.toml").is_file() && !marker.exists() {
        eprintln!("{NOTICE}");
        let _ = std::fs::write(marker, "");
    }
}

/// The workspace `cwd` is in, or an error saying how to make one.
fn workspace(cwd: &std::path::Path) -> Result<projects::Workspace> {
    projects::workspace(cwd)
        .ok_or_else(|| anyhow::anyhow!("not in a workspace: run `primum workspace init`"))
}

/// Runs the command line; returns the exit code.
pub fn run(args: Vec<String>) -> i32 {
    let cli = match Cli::try_parse_from(args) {
        Ok(c) => c,
        Err(e) => {
            let _ = e.print();
            return e.exit_code();
        }
    };
    match dispatch(cli) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("primum: {e:#}");
            1
        }
    }
}

fn dispatch(cli: Cli) -> Result<()> {
    central_list_notice();
    let cwd = std::env::current_dir().context("no working directory")?;
    let project = || projects::resolve(cli.project.as_deref(), &cwd);
    let changes = matches!(
        cli.cmd,
        Cmd::Add { .. }
            | Cmd::Set { .. }
            | Cmd::Start { .. }
            | Cmd::Done { .. }
            | Cmd::Note { .. }
            | Cmd::Doc { .. }
            | Cmd::Kind { .. }
            | Cmd::Delete { .. }
            | Cmd::Import { .. }
            | Cmd::Resolve { .. }
    );
    match cli.cmd {
        Cmd::Init { folder, name } => {
            let p = match folder {
                Some(f) => workspace(&cwd)?.init_project(&f, name.as_deref())?,
                None => projects::init(&cwd, name.as_deref())?,
            };
            println!("primum project {} in {}", p.name, p.root.display());
        }
        Cmd::Workspace {
            cmd: WorkspaceCmd::Init,
        } => {
            let ws = projects::Workspace::create(&cwd)?;
            let found = ws.listed.unwrap_or_default();
            println!("primum workspace in {}", ws.root.display());
            for f in found {
                println!("  {f}");
            }
        }
        Cmd::Workspace {
            cmd: WorkspaceCmd::Migrate,
        } => {
            let done = projects::migrate(&cwd)?;
            if done.is_empty() {
                println!("nothing to migrate");
            }
            for line in done {
                println!("{line}");
            }
        }
        Cmd::Projects { cmd: None } => {
            for e in projects::entries(&cwd) {
                match e.project {
                    Ok(p) => println!("{}\t{}\tok", p.name, e.path.display()),
                    Err(why) => println!("-\t{}\t{why}", e.path.display()),
                }
            }
        }
        Cmd::Projects {
            cmd: Some(ProjectsCmd::Add { folder }),
        } => {
            let p = workspace(&cwd)?.add(folder.trim_end_matches('/'))?;
            println!("{}\t{}", p.name, p.root.display());
        }
        Cmd::Projects {
            cmd: Some(ProjectsCmd::Remove { name }),
        } => workspace(&cwd)?.remove(&name)?,
        Cmd::Add {
            kind,
            title,
            scores,
            needs,
            body_file,
        } => {
            let c = Changes {
                needs: needs.as_deref().map(parse_needs).transpose()?,
                body: body_file.as_ref().map(read_text).transpose()?,
                ..scores_changes(&scores)
            };
            println!(
                "{}",
                store::insert(&project()?, &kind.to_uppercase(), &title, c)?
            );
        }
        Cmd::List {
            kind,
            status,
            all,
            json,
        } => list(&project()?, kind, status, all, json)?,
        Cmd::Show { id, json } => show(&project()?, &norm_id(&id)?, json)?,
        Cmd::Set {
            id,
            title,
            scores,
            status,
            needs,
            body_file,
        } => {
            let id = norm_id(&id)?;
            let c = Changes {
                title,
                status,
                needs: needs.as_deref().map(parse_needs).transpose()?,
                body: body_file.as_ref().map(read_text).transpose()?,
                ..scores_changes(&scores)
            };
            let named = c.fields();
            if named.is_empty() {
                bail!("nothing to change: name a field, like --status done");
            }
            store::update(&project()?, &id, c)?;
            println!("{id}: {}", named.join(", "));
        }
        Cmd::Start { id } => set_status(&project()?, &id, "doing")?,
        Cmd::Done { id } => set_status(&project()?, &id, "done")?,
        Cmd::Note { id, file } => {
            let id = norm_id(&id)?;
            store::append_note(&project()?, &id, &read_text(&file)?)?;
            println!("{id}: note {}", chrono::Local::now().format("%Y-%m-%d"));
        }
        Cmd::Doc { id, doc_type, file } => {
            let path = store::set_doc(&project()?, &norm_id(&id)?, &doc_type, &read_text(&file)?)?;
            println!("{path}");
        }
        Cmd::Kind { id, kind } => {
            let id = norm_id(&id)?;
            let new = store::set_kind(&project()?, &id, &kind.to_uppercase())?;
            println!("{id} -> {new}");
        }
        Cmd::Delete { id, yes } => delete(&project()?, &norm_id(&id)?, yes)?,
        Cmd::Next { n } => next(&project()?, n)?,
        Cmd::Log { id, n, json } => {
            let p = project()?;
            let Some(h) = crate::history::History::open(&p)? else {
                bail!("history is off: git not found");
            };
            let path = id
                .as_deref()
                .map(|i| norm_id(i).map(|i| format!("items/{}", crate::item_file::file_name(&i))))
                .transpose()?;
            let log = h.log(path.as_deref(), n)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&log)?);
            } else {
                for e in &log {
                    let date = e.date.get(..16).unwrap_or(&e.date).replace('T', " ");
                    println!(
                        "{}  {date}  {}  {}",
                        e.short,
                        pad(&truncate(&e.author, 12), 12),
                        e.message
                    );
                }
            }
        }
        Cmd::Sync => print_sync(&crate::sync::sync(&project()?)?, true),
        Cmd::Resolve { id, field, keep } => resolve(&project()?, &norm_id(&id)?, field, keep)?,
        Cmd::Github { cmd } => github(&project()?, cmd)?,
        Cmd::Remote { cmd } => {
            let p = project()?;
            let Some(h) = crate::history::History::open(&p)? else {
                bail!("history is off: git not found");
            };
            match cmd {
                None => match h.remote()? {
                    Some(r) => {
                        let mode = if r.manual { "manual" } else { "auto" };
                        println!("{}  {}  {mode}", r.url, r.branch);
                    }
                    None => println!("no remote"),
                },
                Some(RemoteCmd::Set {
                    url,
                    branch,
                    manual,
                }) => {
                    let r = h.set_remote(&url, branch.as_deref(), manual)?;
                    println!("{}  {}", r.url, r.branch);
                }
                Some(RemoteCmd::Remove) => h.remove_remote()?,
            }
        }
        Cmd::Clone {
            url,
            folder,
            branch,
        } => {
            let folder = folder.unwrap_or_else(|| {
                let name = url.trim_end_matches('/').trim_end_matches(".git");
                let name = name.rsplit(['/', ':']).next().unwrap_or("primum");
                PathBuf::from(name)
            });
            let folder = cwd.join(folder);
            crate::history::clone(&url, branch.as_deref(), &folder)?;
            let p = projects::open(&folder).map_err(|e| anyhow::anyhow!(e))?;
            // a clone directly inside a workspace joins it
            if let (Some(ws), Some(sub)) = (
                projects::Workspace::find(&cwd),
                folder.file_name().and_then(|n| n.to_str()),
            ) && folder.parent().and_then(|d| d.canonicalize().ok())
                == ws.root.canonicalize().ok()
            {
                ws.add(sub)?;
            }
            println!("primum project {} in {}", p.name, p.root.display());
        }
        Cmd::Serve { port, no_open } => {
            crate::server::serve(projects::entries(&cwd), port, !no_open)?
        }
        Cmd::Import { board_dir } => {
            let p = project()?;
            let r = crate::import::run(&board_dir, &p)?;
            println!("{} items into {}", r.items, p.items_dir().display());
            for (from, to) in &r.docs {
                println!("  {from} -> {to}");
            }
            for (id, path) in &r.shared {
                println!("  {id} also shows {path}");
            }
            if !r.left.is_empty() {
                println!("left in {} (linked to no item):", board_dir.display());
                for f in &r.left {
                    println!("  {f}");
                }
            }
        }
    }
    if changes {
        history_notice();
        auto_sync(&project()?);
    }
    Ok(())
}

fn github(p: &Project, cmd: GithubCmd) -> Result<()> {
    let h = crate::history::History::open(p)?;
    let config = |key: &str| {
        h.as_ref()
            .and_then(|h| h.run(&["config", "--get", key]).ok())
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    };
    match cmd {
        GithubCmd::Connect { repo, all } => {
            crate::github::Gh::available().map_err(|why| anyhow!("{why}"))?;
            projects::set_github(p, Some(&repo))?;
            crate::github::Api::ensure_labels(&crate::github::Gh { repo: repo.clone() })?;
            if let Some(h) = &h {
                if all {
                    h.git(&["config", "primum.github-all", "true"])?;
                }
                h.commit(&format!("connect GitHub {repo}"))?;
            }
            println!("{} is connected to {repo}", p.name);
            print_sync(&crate::sync::sync(p)?, true);
        }
        GithubCmd::Disconnect => {
            projects::set_github(p, None)?;
            if let Some(h) = &h {
                let _ = h.run(&["config", "--unset", "primum.github-since"]);
                let _ = h.run(&["config", "--unset", "primum.github-all"]);
                h.commit("disconnect GitHub")?;
            }
            println!("{} is no longer synced with GitHub", p.name);
        }
        GithubCmd::Status => {
            let Some(repo) = projects::github_repo(p)? else {
                println!("not connected: primum github connect OWNER/REPO");
                return Ok(());
            };
            let gh = match crate::github::Gh::available() {
                Ok(()) => "gh ready".to_string(),
                Err(why) => why,
            };
            let linked = store::load(p)?
                .items
                .iter()
                .filter(|i| i.github.is_some())
                .count();
            let since = config("primum.github-since").unwrap_or_else(|| "never".into());
            println!("{repo}  {gh}  {linked} linked items  last sync {since}");
        }
    }
    Ok(())
}

fn parse_keep(k: &str) -> Result<store::Keep> {
    match k {
        "1" => Ok(store::Keep::First),
        "2" => Ok(store::Keep::Second),
        "both" | "b" => Ok(store::Keep::Both),
        _ => bail!("keep 1, 2 or both, not {k}"),
    }
}

/// `primum resolve`: one choice from the flags, else a question per
/// conflicting field, the text and each document on a terminal.
fn resolve(p: &Project, id: &str, field: Option<String>, keep: Option<String>) -> Result<()> {
    if let (Some(f), Some(k)) = (&field, &keep) {
        store::resolve(p, id, f, parse_keep(k)?)?;
        println!("{id}: resolved {f}");
        return Ok(());
    }
    let board = store::load(p)?;
    let c = board
        .conflicts
        .get(id)
        .ok_or_else(|| anyhow!("{id} has no conflict"))?
        .clone();
    let who = |i: usize| {
        let date = c.sides[i].date.get(..16).unwrap_or(&c.sides[i].date);
        format!("{}, {}", c.sides[i].author, date.replace('T', " "))
    };
    let mut questions: Vec<(String, [String; 2], bool)> = c
        .fields
        .iter()
        .map(|f| (f.field.clone(), f.values.clone(), false))
        .collect();
    if let Some(b) = &c.body {
        questions.push(("body".into(), b.values.clone(), false));
    }
    for d in &c.documents {
        questions.push((d.path.clone(), [d.path.clone(), d.other.clone()], true));
    }
    let interactive = std::io::stdin().is_terminal();
    for (what, values, both) in questions {
        println!("{id} {what}");
        for (i, v) in values.iter().enumerate() {
            let shown = if what == "body" {
                format!("\n{}", v.trim_end())
            } else {
                v.clone()
            };
            println!("  {}) {shown}  ({})", i + 1, who(i));
        }
        if !interactive {
            continue;
        }
        print!("keep [1/2{}]: ", if both { "/both" } else { "" });
        std::io::stdout().flush()?;
        let mut answer = String::new();
        std::io::stdin().read_line(&mut answer)?;
        store::resolve(p, id, &what, parse_keep(answer.trim())?)?;
    }
    if !interactive {
        bail!("choose with --field F --keep 1|2");
    }
    Ok(())
}

/// Says what a sync did. `verbose` for `primum sync`; automatic syncs say
/// only what needs attention.
fn print_sync(r: &crate::sync::Report, verbose: bool) {
    if r.no_remote && r.github.is_none() && r.github_off.is_none() {
        if verbose {
            println!("no remote: set one with primum remote set URL");
        }
        return;
    }
    if let Some(why) = &r.offline {
        eprintln!("primum: offline, changes stay local until the next sync ({why})");
    }
    if let Some(why) = &r.github_off {
        eprintln!("primum: GitHub sync is off: {why}");
    }
    if let Some(g) = &r.github {
        for (id, pr) in &g.closed_by {
            eprintln!("primum: {id} closed by #{pr}");
        }
        if verbose {
            for id in &g.created {
                println!("{id}: filed as an issue");
            }
            for id in &g.added {
                println!("{id}: added from an issue");
            }
            let changed = g.updated_items.len() + g.updated_issues.len();
            if g.created.is_empty() && g.added.is_empty() && changed == 0 {
                println!("GitHub: up to date");
            } else if changed > 0 {
                println!("GitHub: {changed} updated");
            }
        }
    }
    for (old, new) in &r.renumbered {
        eprintln!("primum: {old} is now {new} (the number was taken on the remote)");
    }
    for k in &r.kept {
        eprintln!("primum: {k}");
    }
    for id in &r.conflicts {
        eprintln!("primum: {id} has a conflict: primum resolve {id}");
    }
    if verbose && r.offline.is_none() && !r.no_remote {
        let what = match (r.pulled, r.pushed) {
            (true, true) => "pulled and pushed",
            (true, false) => "pulled",
            (false, true) => "pushed",
            (false, false) => "up to date",
        };
        println!("{what}");
    }
}

/// Syncs after a change when the project has a remote and sync is not
/// manual. Never fails the command: problems are said on stderr.
fn auto_sync(p: &Project) {
    if !crate::sync::is_auto(p) {
        return;
    }
    match crate::sync::sync(p) {
        Ok(r) => print_sync(&r, false),
        Err(e) => eprintln!("primum: sync: {e:#}"),
    }
}

/// Says once per command that history is off, when git is missing.
fn history_notice() {
    if !crate::history::git_available() {
        eprintln!("primum: history is off: git not found");
    }
}

fn scores_changes(s: &Scores) -> Changes {
    Changes {
        impact: s.impact,
        urgency: s.urgency,
        risk: s.risk,
        effort: s.effort,
        ..Default::default()
    }
}

fn set_status(p: &Project, id: &str, status: &str) -> Result<()> {
    let id = norm_id(id)?;
    let c = Changes {
        status: Some(status.into()),
        ..Default::default()
    };
    store::update(p, &id, c)?;
    println!("{id}: {status}");
    Ok(())
}

/// `todo:3` is `TODO:3`; anything that is not an id is an error.
fn norm_id(id: &str) -> Result<String> {
    let up = id.trim().to_uppercase();
    parse_id(&up).ok_or_else(|| anyhow!("{id} is not an item id, like TODO:3"))?;
    Ok(up)
}

/// `TODO:1, bug:2` is `[TODO:1, BUG:2]`; an empty string is no needs.
pub fn parse_needs(s: &str) -> Result<Vec<String>> {
    s.split(',')
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .map(norm_id)
        .collect()
}

/// A file's text; `-` is standard input.
fn read_text(path: &PathBuf) -> Result<String> {
    if path.as_os_str() == "-" {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s)?;
        return Ok(s);
    }
    std::fs::read_to_string(path).map_err(|_| anyhow!("no such file: {}", path.display()))
}

fn list(
    p: &Project,
    kind: Option<String>,
    status: Option<String>,
    all: bool,
    json: bool,
) -> Result<()> {
    let statuses: Vec<String> = match (&status, all) {
        (Some(s), _) => s.split(',').map(|x| x.trim().to_lowercase()).collect(),
        (None, true) => STATUSES.iter().map(|s| s.to_string()).collect(),
        (None, false) => vec!["open".into(), "doing".into()],
    };
    if let Some(bad) = statuses.iter().find(|s| !STATUSES.contains(&s.as_str())) {
        bail!("unknown status {bad}");
    }
    let kind = kind.map(|k| k.to_uppercase());
    if let Some(k) = &kind
        && !KINDS.contains(&k.as_str())
    {
        bail!("unknown kind {k}");
    }
    let board = store::load(p)?;
    let rows: Vec<ItemView> = views(&board)
        .into_iter()
        .filter(|v| kind.as_ref().is_none_or(|k| &v.kind == k))
        .filter(|v| statuses.contains(&v.status))
        .collect();
    if json {
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }
    let style = Style::detect();
    println!("{}", header(&style));
    for v in &rows {
        println!("{}", row_line(v, &style));
    }
    Ok(())
}

fn show(p: &Project, id: &str, json: bool) -> Result<()> {
    let board = store::load(p)?;
    let it = board.get(id).ok_or_else(|| anyhow!("no item {id}"))?;
    let v = ItemView::new(it, &board);
    if json {
        println!("{}", serde_json::to_string_pretty(&v)?);
        return Ok(());
    }
    let s = Style::detect();
    println!("{} {}", s.badge(&v.kind, &v.id), s.paint(&v.title, "1"));
    println!(
        "impact {}  urgency {}  risk {}  effort {}  score {:.1}  {}",
        v.impact, v.urgency, v.risk, v.effort, v.score, v.status
    );
    if !v.needs.is_empty() {
        let blocked = if v.blocked {
            format!(" (blocked by {})", v.blockers.join(", "))
        } else {
            String::new()
        };
        println!("needs {}{blocked}", v.needs.join(", "));
    }
    if let Some(f) = &v.finished {
        println!("finished {f}");
    }
    println!(
        "{}",
        s.paint(&format!("{}/{}", p.root.display(), v.file), "2")
    );
    if let (Some(n), Some(repo)) = (it.github, projects::github_repo(p)?) {
        println!("issue #{n}  https://github.com/{repo}/issues/{n}");
    }
    if !v.body.trim().is_empty() {
        println!();
        print!("{}", render_markdown(&v.body, &s));
    }
    if !v.docs.is_empty() {
        println!();
        println!("{}", s.paint("documents", "1"));
        for d in &v.docs {
            println!("  {:<8} {}", d.kind, d.path);
        }
    }
    Ok(())
}

fn delete(p: &Project, id: &str, yes: bool) -> Result<()> {
    if !yes {
        let board = store::load(p)?;
        board.get(id).ok_or_else(|| anyhow!("no item {id}"))?;
        let own = board
            .docs
            .get(id)
            .map(|d| {
                let prefix = format!("items/{}.", crate::item_file::stem(id));
                d.iter().filter(|x| x.path.starts_with(&prefix)).count()
            })
            .unwrap_or(0);
        if !std::io::stdin().is_terminal() {
            bail!("deleting {id} needs --yes when not run from a terminal");
        }
        print!("Delete {id} and {own} documents? [y/N] ");
        std::io::stdout().flush()?;
        let mut answer = String::new();
        std::io::stdin().read_line(&mut answer)?;
        if !matches!(answer.trim(), "y" | "Y" | "yes") {
            bail!("not deleted");
        }
    }
    for f in store::delete(p, id)? {
        println!("{f}");
    }
    Ok(())
}

/// Finish what is started before starting more: the items in progress
/// first, then the top open, unblocked ones, as the page's "next up".
fn next(p: &Project, n: usize) -> Result<()> {
    let board = store::load(p)?;
    let rows = views(&board);
    let doing: Vec<&str> = rows
        .iter()
        .filter(|v| v.status == "doing")
        .map(|v| v.id.as_str())
        .collect();
    if !doing.is_empty() {
        println!("in progress: {}", doing.join(", "));
    }
    let style = Style::detect();
    let open: Vec<&ItemView> = rows
        .iter()
        .filter(|v| v.status == "open" && !v.blocked)
        .take(n)
        .collect();
    if open.is_empty() {
        println!("nothing unblocked and open.");
    }
    for v in open {
        println!("{}", row_line(v, &style));
    }
    Ok(())
}

// ---------------------------------------------------------------- terminal

/// ANSI styling, off when stdout is not a terminal or NO_COLOR is set.
pub struct Style {
    color: bool,
    width: usize,
}

impl Style {
    fn detect() -> Style {
        let color = std::io::stdout().is_terminal()
            && std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty());
        let width = std::env::var("COLUMNS")
            .ok()
            .and_then(|c| c.parse().ok())
            .unwrap_or(120);
        Style { color, width }
    }

    fn paint(&self, text: &str, codes: &str) -> String {
        if self.color {
            format!("\x1b[{codes}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    /// The id on its lane's color, as on the page.
    fn badge(&self, kind: &str, id: &str) -> String {
        let (r, g, b) = kind_rgb(kind);
        self.paint(&format!(" {id:<9}"), &format!("1;97;48;2;{r};{g};{b}"))
    }
}

fn kind_rgb(kind: &str) -> (u8, u8, u8) {
    match kind {
        "URGENT" => (0xB6, 0x3F, 0x3F),
        "BUG" => (0x9A, 0x6A, 0x16),
        "DEBT" => (0x4F, 0x6F, 0x99),
        "KILLER" => (0x3F, 0x7E, 0x69),
        _ => (0x75, 0x68, 0x9A),
    }
}

fn title_width(s: &Style) -> usize {
    s.width.saturating_sub(60).max(20)
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    let mut t: String = s.chars().take(n.saturating_sub(1)).collect();
    t.push('…');
    t
}

fn pad(s: &str, n: usize) -> String {
    let len = s.chars().count();
    format!("{s}{}", " ".repeat(n.saturating_sub(len)))
}

fn header(s: &Style) -> String {
    let line = format!(
        "{} {} imp urg rsk eff score {} status",
        pad("id", 10),
        pad("title", title_width(s)),
        pad("needs", 16)
    );
    s.paint(&line, "2")
}

fn row_line(v: &ItemView, s: &Style) -> String {
    let id = if s.color {
        s.badge(&v.kind, &v.id)
    } else {
        pad(&v.id, 10)
    };
    let needs = if v.blocked {
        v.blockers.join(",")
    } else {
        v.needs.join(",")
    };
    let rest = format!(
        " {} {:>3} {:>3} {:>3} {:>3} {:>5.1} {} {}{}",
        pad(&truncate(&v.title, title_width(s)), title_width(s)),
        v.impact,
        v.urgency,
        v.risk,
        v.effort,
        v.score,
        pad(&truncate(&needs, 16), 16),
        v.status,
        // `!`: a sync left a conflict to resolve
        if v.conflict.is_some() { "!" } else { "" }
    );
    let rest = if v.status == "doing" {
        s.paint(&rest, "1")
    } else if v.blocked || v.status == "done" || v.status == "wontfix" {
        s.paint(&rest, "2")
    } else {
        rest
    };
    format!("{id}{rest}")
}

/// Markdown for the terminal: headings bold, fenced code dim, nothing
/// reflowed.
fn render_markdown(md: &str, s: &Style) -> String {
    let mut out = String::new();
    let mut fenced = false;
    for line in md.lines() {
        let styled = if line.trim_start().starts_with("```") {
            fenced = !fenced;
            s.paint(line, "2")
        } else if fenced {
            s.paint(line, "2")
        } else if line.starts_with('#') {
            s.paint(line, "1")
        } else {
            line.to_string()
        };
        out += &styled;
        out.push('\n');
    }
    out
}
