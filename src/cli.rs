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
    /// Make this folder a project (primum.toml and items/) and list it
    Init {
        /// The project's name; default: the folder's
        #[arg(long)]
        name: Option<String>,
    },
    /// The projects this machine shows, or add or remove one
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
    /// This machine's remote for the project, or set or remove it
    Remote {
        #[command(subcommand)]
        cmd: Option<RemoteCmd>,
    },
    /// Make a project from a remote and list it
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
    /// List the project in PATH on this machine
    Add { path: PathBuf },
    /// Take a project off this machine's list (the project is not touched)
    Remove { name: String },
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
    );
    match cli.cmd {
        Cmd::Init { name } => {
            let p = projects::init(&cwd, name.as_deref())?;
            println!("primum project {} in {}", p.name, p.root.display());
        }
        Cmd::Projects { cmd: None } => {
            for e in projects::list()? {
                match e.project {
                    Ok(p) => println!("{}\t{}\tok", p.name, e.path.display()),
                    Err(why) => println!("-\t{}\t{why}", e.path.display()),
                }
            }
        }
        Cmd::Projects {
            cmd: Some(ProjectsCmd::Add { path }),
        } => {
            let p = projects::add(&path)?;
            println!("{}\t{}", p.name, p.root.display());
        }
        Cmd::Projects {
            cmd: Some(ProjectsCmd::Remove { name }),
        } => projects::remove(&name)?,
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
            let p = projects::add(&folder)?;
            println!("primum project {} in {}", p.name, p.root.display());
        }
        Cmd::Serve { port, no_open } => crate::server::serve(port, !no_open)?,
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
    }
    Ok(())
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
        " {} {:>3} {:>3} {:>3} {:>3} {:>5.1} {} {}",
        pad(&truncate(&v.title, title_width(s)), title_width(s)),
        v.impact,
        v.urgency,
        v.risk,
        v.effort,
        v.score,
        pad(&truncate(&needs, 16), 16),
        v.status
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
