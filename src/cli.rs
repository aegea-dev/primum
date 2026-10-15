//! The command line. Writes go through `store`, reads through `store::load`
//! and the same `ItemView` the web page gets, so the terminal and the page
//! always agree.
use crate::model::{KINDS, id_of, label, parse_id};
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
        /// Default: the text's first heading
        title: Option<String>,
        #[command(flatten)]
        scores: Scores,
        /// Ids it waits for, like 3,5
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
        /// Ids it waits for, like 3,5 ("" for none)
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
    /// Bring a board made by an earlier version up to date, once
    Migrate,
    /// The project's context variables, stored and computed, or set one
    Context {
        #[command(subcommand)]
        cmd: Option<ContextCmd>,
        #[arg(long)]
        json: bool,
    },
    /// The project's lifecycles: check, show or list them
    Lifecycle {
        #[command(subcommand)]
        cmd: LifecycleCmd,
    },
    /// The lifecycle manager: one decision, JSON in on stdin, JSON out on stdout
    #[command(hide = true)]
    Manager,
    /// Keeps `primum daemon` running: restarts it when it exits, stops
    /// answering or stops making passes (what the service runs)
    #[command(hide = true)]
    Watchdog {
        #[arg(long, default_value_t = 8777)]
        port: u16,
        /// Seconds between the daemon's passes
        #[arg(long, default_value_t = 1)]
        interval: u64,
        /// Seconds between checks of the daemon's /health
        #[arg(long, default_value_t = 10)]
        check_every: u64,
        /// Checks in a row without an answer before a restart
        #[arg(long, default_value_t = 3)]
        misses: u32,
        /// Seconds without a pass before the daemon counts as hung
        #[arg(long, default_value_t = 600)]
        stale_after: u64,
        /// Where its lines and the daemon's stderr go
        #[arg(long)]
        log: Option<PathBuf>,
    },
    /// A worker: one job, JSON in on stdin, its output as JSON on stdout
    #[command(hide = true)]
    Worker,
    /// Run tickets' lifecycles, for every project here, pass after pass
    #[command(args_conflicts_with_subcommands = true)]
    Daemon {
        #[command(subcommand)]
        cmd: Option<DaemonCmd>,
        /// One pass, then stop
        #[arg(long)]
        once: bool,
        /// Seconds between passes
        #[arg(long, default_value_t = 1)]
        interval: u64,
        /// The port it serves the board, /health and outside events on
        #[arg(long, default_value_t = 8777)]
        port: u16,
        /// Run the passes only: no board, no events from outside
        #[arg(long)]
        no_serve: bool,
        /// Run projects another machine runs, and become their runner
        #[arg(long)]
        take_over: bool,
        /// The time to run at, with --once (tests)
        #[arg(long, hide = true, requires = "once")]
        now: Option<String>,
    },
    /// The engine's journal: a ticket's events in order, or every event
    Events {
        id: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Secrets in the OS secret store, for `${env.NAME}` in lifecycles
    Secret {
        #[command(subcommand)]
        cmd: SecretCmd,
    },
}

#[derive(Subcommand)]
pub enum DaemonCmd {
    /// Run the daemon as a service (per user, at login), kept up by a watchdog
    Install {
        #[arg(long, default_value_t = 8777)]
        port: u16,
        /// Seconds between passes
        #[arg(long, default_value_t = 1)]
        interval: u64,
    },
    /// Stop and remove the service
    Uninstall,
    /// The service's state and the daemon's health
    Status {
        #[arg(long, default_value_t = 8777)]
        port: u16,
    },
}

#[derive(Subcommand)]
pub enum SecretCmd {
    /// Keep a secret; its value is read from stdin, or asked for on a terminal
    Set { name: String },
    /// Remove a secret
    Unset { name: String },
    /// The secrets' names (never their values)
    List,
}

#[derive(Subcommand)]
pub enum ContextCmd {
    /// Store KEY (VALUE as JSON, else as text), or remove it with --unset
    Set {
        key: String,
        value: Option<String>,
        #[arg(long)]
        unset: bool,
    },
}

#[derive(Subcommand)]
pub enum LifecycleCmd {
    /// Validate a lifecycle (a name or a file), or every one in lifecycles/
    Check { target: Option<String> },
    /// A lifecycle's steps in order, and its versions
    Show {
        name: String,
        #[arg(long)]
        json: bool,
    },
    /// The project's lifecycles and their versions
    List,
    /// Make a ticket follow a lifecycle, pinned at its current version
    Attach {
        id: String,
        name: String,
        /// An argument its actions read as `args`; a value that reads as
        /// JSON stays JSON
        #[arg(long = "arg", value_name = "KEY=VALUE")]
        args: Vec<String>,
    },
    /// Make a ticket follow the built-in manual lifecycle again
    Detach { id: String },
    /// Run a failed ticket's failed transition again (the daemon runs it)
    Resume { id: String },
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

/// Lists a command's subcommands, and theirs, by name in help: one display
/// order for all, which clap breaks by name.
fn by_name(c: clap::Command) -> clap::Command {
    c.mut_subcommands(|s| by_name(s.display_order(0)))
}

/// Runs the command line; returns the exit code.
pub fn run(args: Vec<String>) -> i32 {
    use clap::{CommandFactory, FromArgMatches};
    let parsed = by_name(Cli::command())
        .try_get_matches_from(args)
        .and_then(|m| Cli::from_arg_matches(&m));
    let cli = match parsed {
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
        Cmd::Lifecycle { cmd } => lifecycle_cmd(&project()?, cmd)?,
        Cmd::Context { cmd, json } => context_cmd(&project()?, cmd, json)?,
        Cmd::Daemon { cmd: Some(cmd), .. } => {
            use crate::engine::service;
            // the service runs in the workspace, or in the project when
            // there is no workspace
            let root = match projects::workspace(&cwd) {
                Some(ws) => ws.root,
                None => project()?.root,
            };
            match cmd {
                DaemonCmd::Install { port, interval } => {
                    let exe = std::env::current_exe()?;
                    let path = std::env::var("PATH").ok();
                    service::install(service::install_ctx(&root, &exe, port, interval, path))?;
                    println!(
                        "installed and started: {} runs the daemon in {}",
                        service::label(&root).to_qualified_name(),
                        root.display()
                    );
                    println!("board: http://127.0.0.1:{port}/");
                    println!("log: {}", service::log_file(&root).display());
                    println!(
                        "it keeps this shell's PATH (for git, uv, bun); install again after changing it"
                    );
                }
                DaemonCmd::Uninstall => {
                    if service::uninstall(&root)? {
                        println!("stopped and removed");
                    } else {
                        println!("no service for {}", root.display());
                    }
                }
                DaemonCmd::Status { port } => {
                    println!("service: {}", service::status(&root)?);
                    let health = reqwest::blocking::Client::builder()
                        .timeout(std::time::Duration::from_secs(3))
                        .build()?
                        .get(format!("http://127.0.0.1:{port}/health"))
                        .send()
                        .and_then(|r| r.json::<serde_json::Value>());
                    match health {
                        Ok(h) => println!(
                            "daemon: pid {}, {} passes, last {}{}",
                            h["pid"],
                            h["passes"],
                            h["last_pass"].as_str().unwrap_or("none yet"),
                            h["last_error"]
                                .as_str()
                                .map(|e| format!(", last error: {e}"))
                                .unwrap_or_default()
                        ),
                        Err(_) => println!("daemon: no answer on port {port}"),
                    }
                }
            }
        }
        Cmd::Daemon {
            cmd: None,
            once,
            interval,
            port,
            no_serve,
            take_over,
            now,
        } => {
            use crate::engine::daemon::{self, Health};
            let manager = std::env::current_exe()?;
            // the board shows the projects the daemon runs: -p NAME, or all
            let entries: Vec<projects::Entry> = projects::entries(&cwd)
                .into_iter()
                .filter(|e| {
                    cli.project
                        .as_deref()
                        .is_none_or(|name| e.project.as_ref().is_ok_and(|p| p.name == name))
                })
                .collect();
            let projects: Vec<Project> = match cli.project.as_deref() {
                Some(_) => vec![project()?],
                None => entries
                    .iter()
                    .filter_map(|e| e.project.clone().ok())
                    .collect(),
            };
            if projects.is_empty() {
                bail!("no primum project here: cd into a board or a workspace");
            }
            // one machine runs a project's lifecycles at a time
            let host = projects::host();
            let mut mine = vec![];
            for p in projects {
                match projects::claim(&p, &host, take_over)? {
                    None => mine.push(p),
                    Some(other) => eprintln!(
                        "{}: runs on {other}; skipped (--take-over runs it here)",
                        p.name
                    ),
                }
            }
            let projects = mine;
            if projects.is_empty() {
                bail!("every project here runs on another machine");
            }
            if once {
                let at = match &now {
                    Some(t) => t
                        .parse::<chrono::DateTime<chrono::Utc>>()
                        .map_err(|_| anyhow!("--now takes an RFC 3339 time, not {t}"))?,
                    None => chrono::Utc::now(),
                };
                for p in &projects {
                    report_pass(p, &daemon::run_once(p, at, &manager));
                }
                return Ok(());
            }
            let (wake, woken) = std::sync::mpsc::channel();
            let engine = std::sync::Arc::new(crate::engine::http::Engine {
                health: std::sync::Mutex::new(Health::starting(&daemon::stamp(chrono::Utc::now()))),
                wake,
            });
            let looping = {
                let engine = engine.clone();
                let projects = projects.clone();
                let interval = std::time::Duration::from_secs(interval);
                std::thread::spawn(move || {
                    daemon::run_loop(
                        &projects,
                        interval,
                        &manager,
                        &woken,
                        &engine.health,
                        report_pass,
                    )
                })
            };
            if no_serve {
                looping
                    .join()
                    .map_err(|_| anyhow!("the daemon's loop stopped unexpectedly"))?;
            } else {
                let app = crate::engine::http::router(entries, engine);
                crate::server::serve_app(app, projects, port, false)?;
            }
        }
        Cmd::Secret { cmd } => {
            use crate::engine::secrets;
            match cmd {
                SecretCmd::Set { name } => {
                    secrets::ready(&name)?;
                    let value = read_secret(&format!("value for {name}: "))?;
                    if value.is_empty() {
                        bail!("{name}: an empty value; nothing kept");
                    }
                    secrets::set(&name, &value)?;
                    println!("{name}: kept in the OS secret store");
                }
                SecretCmd::Unset { name } => {
                    if secrets::unset(&name)? {
                        println!("{name}: removed");
                    } else {
                        println!("{name}: not in the OS secret store");
                    }
                }
                SecretCmd::List => {
                    for n in secrets::names()? {
                        println!("{n}");
                    }
                }
            }
        }
        Cmd::Events { id, json } => {
            let p = project()?;
            let j = crate::engine::journal::Journal::open(&p)?;
            let subject = id
                .map(|i| norm_id(&i))
                .transpose()?
                .map(|i| crate::engine::journal::subject(&p.name, &i));
            let events = j.events(subject.as_deref())?;
            if json {
                println!("{}", serde_json::to_string_pretty(&events)?);
            } else {
                for r in events {
                    let state = match (&r.handled, &r.error) {
                        (_, Some(e)) => format!("error: {e}"),
                        (Some(_), None) => "handled".to_string(),
                        (None, None) => "waiting".to_string(),
                    };
                    println!(
                        "{:>5}  {}  {:<20} {:<36} {state}",
                        r.seq, r.event.time, r.event.kind, r.event.subject
                    );
                }
            }
        }
        Cmd::Worker => {
            let mut text = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut text)?;
            let job: crate::engine::Job =
                serde_json::from_str(&text).map_err(|e| anyhow!("the worker's job: {e}"))?;
            use crate::engine::worker::{Failure, LOST_EXIT, run};
            #[cfg(unix)]
            crate::engine::worker::stop_with_parent();
            match run(&job) {
                Ok(output) => println!("{}", serde_json::json!({ "output": output })),
                Err(Failure::Error(why)) => bail!(why),
                Err(Failure::Lost(why)) => {
                    eprintln!("error: {why}");
                    std::process::exit(LOST_EXIT);
                }
            }
        }
        Cmd::Watchdog {
            port,
            interval,
            check_every,
            misses,
            stale_after,
            log,
        } => {
            use crate::engine::watchdog::{Options, Rules, run};
            let mut daemon_args = vec![
                "--port".to_string(),
                port.to_string(),
                "--interval".to_string(),
                interval.to_string(),
            ];
            if let Some(name) = &cli.project {
                daemon_args.extend(["-p".to_string(), name.clone()]);
            }
            run(&Options {
                exe: std::env::current_exe()?,
                daemon_args,
                port,
                rules: Rules {
                    check_every: std::time::Duration::from_secs(check_every),
                    misses,
                    stale_after: std::time::Duration::from_secs(stale_after),
                    ..Rules::default()
                },
                log,
            })?;
        }
        Cmd::Manager => {
            let mut text = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut text)?;
            let input: crate::engine::Input =
                serde_json::from_str(&text).map_err(|e| anyhow!("the manager's input: {e}"))?;
            let answer = crate::engine::manager::decide(&input).map_err(|e| anyhow!(e))?;
            println!("{}", serde_json::to_string(&answer)?);
        }
        Cmd::Migrate => {
            let p = project()?;
            let moved = crate::migrate::migrate(&p)?;
            let statuses = crate::migrate::migrate_statuses(&p)?;
            if moved.is_empty() && statuses == 0 {
                println!("nothing to migrate");
            }
            for (old, new) in moved {
                println!("{old} -> {}", label(&new));
            }
            if statuses > 0 {
                println!("{statuses} items: open -> idle");
            }
        }
        Cmd::Workspace {
            cmd: WorkspaceCmd::Migrate,
        } => {
            let mut done = projects::migrate(&cwd)?;
            for e in projects::entries(&cwd) {
                if let Ok(p) = e.project {
                    let n = crate::migrate::migrate(&p)?.len();
                    if n > 0 {
                        done.push(format!("{}: numbered {n} items", p.name));
                    }
                    let s = crate::migrate::migrate_statuses(&p)?;
                    if s > 0 {
                        done.push(format!("{}: {s} items open -> idle", p.name));
                    }
                }
            }
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
                store::insert(
                    &project()?,
                    &kind.to_uppercase(),
                    title.as_deref().unwrap_or(""),
                    c
                )?
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
            println!("{}: {}", label(&id), named.join(", "));
        }
        Cmd::Start { id } => set_status(&project()?, &id, "doing")?,
        Cmd::Done { id } => set_status(&project()?, &id, "done")?,
        Cmd::Note { id, file } => {
            let id = norm_id(&id)?;
            store::append_note(&project()?, &id, &read_text(&file)?)?;
            println!(
                "{}: note {}",
                label(&id),
                chrono::Local::now().format("%Y-%m-%d")
            );
        }
        Cmd::Doc { id, doc_type, file } => {
            let path = store::set_doc(&project()?, &norm_id(&id)?, &doc_type, &read_text(&file)?)?;
            println!("{path}");
        }
        Cmd::Kind { id, kind } => {
            let id = norm_id(&id)?;
            let kind = kind.to_uppercase();
            store::set_kind(&project()?, &id, &kind)?;
            println!("{}: kind {kind}", label(&id));
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
        println!("{}: resolved {f}", label(id));
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
        println!("{} {what}", label(id));
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
    if r.no_remote {
        if verbose {
            println!("no remote: set one with primum remote set URL");
        }
        return;
    }
    if let Some(why) = &r.offline {
        eprintln!("primum: offline, changes stay local until the next sync ({why})");
    }
    for (old, new) in &r.renumbered {
        eprintln!(
            "primum: {} is now {} (the number was taken on the remote)",
            label(old),
            label(new)
        );
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
    store::update(p, &id, c)
        .map_err(|e| anyhow!("{e:#}; for another state use primum set {id} --status STATE"))?;
    println!("{}: {status}", label(&id));
    Ok(())
}

fn context_cmd(p: &Project, cmd: Option<ContextCmd>, json: bool) -> Result<()> {
    match cmd {
        Some(ContextCmd::Set { key, value, unset }) => {
            let value = match (value, unset) {
                (_, true) => None,
                (Some(v), false) => {
                    Some(serde_json::from_str(&v).unwrap_or(serde_json::Value::String(v)))
                }
                (None, false) => bail!("give a value, or --unset"),
            };
            projects::set_context(p, &key, value)?;
        }
        None => {
            let stored = projects::context(p)?;
            let computed = store::computed_context(&store::load(p)?);
            if json {
                let v = serde_json::json!({
                    "stored": stored,
                    "computed": computed,
                    "lifecycle": projects::project_lifecycle(p)?,
                });
                println!("{}", serde_json::to_string_pretty(&v)?);
                return Ok(());
            }
            for (k, v) in stored.iter().chain(computed.iter()) {
                println!("{k} = {v}");
            }
        }
    }
    Ok(())
}

/// A project's lifecycle by name: a file in lifecycles/, else a built-in one.
fn find_lifecycle(p: &Project, name: &str) -> Result<crate::lifecycle::Lifecycle> {
    Ok(crate::lifecycle::resolve(p, name).ok_or_else(|| {
        anyhow!(
            "no lifecycle {name} in {}",
            crate::lifecycle::dir(p).display()
        )
    })??)
}

/// The project's lifecycle files, sorted.
fn lifecycle_files(p: &Project) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(crate::lifecycle::dir(p))
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|f| f.extension().is_some_and(|x| x == "json"))
        .collect();
    v.sort();
    v
}

/// How `lifecycle show` prints a transition: `from -> to`, what starts it
/// and what it does, then its extras.
fn transition_line(t: &crate::lifecycle::format::Transition) -> String {
    let mut what = vec![];
    if let Some(on) = &t.on {
        what.push(format!("on {on}"));
    }
    if let Some(w) = &t.when {
        what.push(format!("when {w}"));
    }
    if !t.actions.is_empty() {
        let names: Vec<&str> = t.actions.iter().map(|a| a.action.as_str()).collect();
        what.push(format!("do {}", names.join(", ")));
    }
    let mut extra = vec![];
    if let Some(s) = &t.showing {
        extra.push(format!("while {s}"));
    }
    if let (Some(d), Some(to)) = (&t.timeout, &t.on_timeout) {
        extra.push(format!("timeout {d} -> {to}"));
    }
    if let Some(f) = &t.on_fail {
        extra.push(format!("on fail -> {f}"));
    }
    let extra = if extra.is_empty() {
        String::new()
    } else {
        format!("  ({})", extra.join("; "))
    };
    format!(
        "  {:<28} {}{extra}",
        format!("{} -> {}", t.from, t.to),
        what.join(" ")
    )
}

fn lifecycle_cmd(p: &Project, cmd: LifecycleCmd) -> Result<()> {
    use crate::lifecycle::{definitions, load_file, short, validate};
    match cmd {
        LifecycleCmd::Check { target } => {
            let files = match target {
                Some(t) if t.ends_with(".json") => vec![PathBuf::from(t)],
                Some(name) => vec![crate::lifecycle::dir(p).join(format!("{name}.json"))],
                None => lifecycle_files(p),
            };
            if files.is_empty() {
                println!("no lifecycles in {}", crate::lifecycle::dir(p).display());
            }
            let defs = definitions::load_all(p)?;
            for f in files {
                let l = load_file(&f, &defs)?;
                println!("{}  {}", l.file.name, short(&l.version));
                for w in validate::warnings(&l.file) {
                    eprintln!("warning: {}: {w}", f.display());
                }
            }
        }
        LifecycleCmd::Show { name, json } => {
            let l = find_lifecycle(p, &name)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&l)?);
                return Ok(());
            }
            println!("{}  {}", l.file.name, short(&l.version));
            if !l.file.description.is_empty() {
                println!("{}", l.file.description);
            }
            println!("states  {}", l.file.states.join(", "));
            println!("start   {}", l.file.start);
            println!("end     {}", l.file.end.join(", "));
            for t in &l.file.transitions {
                println!("{}", transition_line(t));
            }
        }
        LifecycleCmd::Attach { id, name, args } => {
            let id = norm_id(&id)?;
            let v = crate::store::attach(p, &id, &name, lifecycle_args(&args)?)?;
            println!("{}: {name} {}", label(&id), short(&v));
        }
        LifecycleCmd::Detach { id } => {
            let id = norm_id(&id)?;
            crate::store::detach(p, &id)?;
            println!("{}: manual", label(&id));
        }
        LifecycleCmd::Resume { id } => {
            use crate::engine::journal::{Envelope, Journal, subject};
            let id = norm_id(&id)?;
            let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
            let e = Envelope::new(
                "cli",
                "lifecycle.resume",
                &subject(&p.name, &id),
                &now,
                None,
                serde_json::json!({}),
            );
            Journal::open(p)?.publish(&e)?;
            println!("{}: resume queued; the daemon runs it", label(&id));
        }
        LifecycleCmd::List => {
            let manual = find_lifecycle(p, "manual")?;
            println!("manual  {}  built in", short(&manual.version));
            let defs = definitions::load_all(p)?;
            for f in lifecycle_files(p) {
                match load_file(&f, &defs) {
                    Ok(l) => println!(
                        "{}  {}  {}",
                        l.file.name,
                        short(&l.version),
                        l.file.description
                    ),
                    Err(e) => println!("{}  -  {e}", f.display()),
                }
            }
        }
    }
    Ok(())
}

/// What a daemon pass did, on stderr: only when it did something, or failed.
fn report_pass(p: &Project, pass: &Result<crate::engine::daemon::Pass>) {
    match pass {
        Ok(pass) if pass.delivered + pass.written + pass.worked + pass.lost + pass.deferred > 0 => {
            eprintln!(
                "{}: {} events, {} jobs, {} lost (run again), {} changes written, {} waiting",
                p.name, pass.delivered, pass.worked, pass.lost, pass.written, pass.deferred
            )
        }
        Ok(_) => {}
        Err(e) => eprintln!("{}: {e:#}", p.name),
    }
}

/// `--arg key=value` pairs as a JSON object: a value that reads as JSON
/// stays JSON (`3`, `true`, `["a"]`), anything else is text.
fn lifecycle_args(pairs: &[String]) -> Result<Option<serde_json::Map<String, serde_json::Value>>> {
    if pairs.is_empty() {
        return Ok(None);
    }
    let mut m = serde_json::Map::new();
    for p in pairs {
        let (k, v) = p
            .split_once('=')
            .ok_or_else(|| anyhow!("--arg {p}: give KEY=VALUE"))?;
        let v =
            serde_json::from_str(v).unwrap_or_else(|_| serde_json::Value::String(v.to_string()));
        m.insert(k.to_string(), v);
    }
    Ok(Some(m))
}

/// The terminal's echo, off while this lives: it is restored when dropped,
/// on every path out (an error, a panic).
#[cfg(unix)]
struct EchoOff(libc::termios);

#[cfg(unix)]
impl EchoOff {
    /// Turns stdin's echo off, or says why it cannot.
    fn new() -> Result<EchoOff> {
        // SAFETY: tcgetattr writes stdin's settings into `t`, which is only
        // used when it succeeds; tcsetattr reads a valid termios
        unsafe {
            let mut t: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(libc::STDIN_FILENO, &mut t) != 0 {
                bail!(
                    "cannot hide what is typed: {}",
                    std::io::Error::last_os_error()
                );
            }
            let old = t;
            t.c_lflag &= !libc::ECHO;
            if libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &t) != 0 {
                bail!(
                    "cannot hide what is typed: {}",
                    std::io::Error::last_os_error()
                );
            }
            Ok(EchoOff(old))
        }
    }
}

#[cfg(unix)]
impl Drop for EchoOff {
    fn drop(&mut self) {
        // SAFETY: restores the settings tcgetattr returned
        unsafe {
            libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &self.0);
        }
    }
}

#[cfg(not(unix))]
struct EchoOff;

#[cfg(not(unix))]
impl EchoOff {
    fn new() -> Result<EchoOff> {
        bail!("cannot hide what is typed here")
    }
}

/// A secret's value: one line from stdin, asked for (without echo) on a
/// terminal. Never an argument: command lines are visible to others. A
/// value that cannot be hidden while typed is not asked for at all.
fn read_secret(prompt: &str) -> Result<String> {
    use std::io::{BufRead, IsTerminal};
    let mut line = String::new();
    if std::io::stdin().is_terminal() {
        let echo = EchoOff::new().map_err(|e| anyhow!("{e:#}; pipe the value on stdin instead"))?;
        eprint!("{prompt}");
        let read = std::io::stdin().lock().read_line(&mut line);
        drop(echo);
        eprintln!();
        read?;
    } else {
        std::io::stdin().lock().read_line(&mut line)?;
    }
    Ok(line.trim_end_matches(['\n', '\r']).to_string())
}

/// `12` and `#12` are `12`; anything that is not an id is an error.
fn norm_id(id: &str) -> Result<String> {
    parse_id(id)
        .map(id_of)
        .ok_or_else(|| anyhow!("{id} is not an item id, like 12 or #12"))
}

/// `3, #5` is `[3, 5]`; an empty string is no needs.
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
    let statuses: Option<Vec<String>> = status
        .as_ref()
        .map(|s| s.split(',').map(|x| x.trim().to_lowercase()).collect());
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
        .filter(|v| match &statuses {
            Some(s) => s.contains(&v.status),
            None => all || !v.closed,
        })
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
    if let Some(pin) = &v.lifecycle {
        let (name, version) = crate::lifecycle::split_pin(pin);
        let gone = match crate::lifecycle::resolve(p, name) {
            Some(Ok(l)) if l.version == version => "",
            _ => "  (that version is no longer in lifecycles/)",
        };
        println!(
            "lifecycle {name} {}{gone}",
            crate::lifecycle::short(version)
        );
        if let Some(a) = &v.args {
            println!("args {}", serde_json::to_string(a)?);
        }
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
            bail!(
                "deleting {} needs --yes when not run from a terminal",
                label(id)
            );
        }
        print!("Delete {} and {own} documents? [y/N] ", label(id));
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
        .filter(|v| v.states.first() == Some(&v.status) && !v.blocked)
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
    /// `#12 BUG` in the lane's colour, 13 columns wide.
    fn badge(&self, kind: &str, id: &str) -> String {
        let (r, g, b) = kind_rgb(kind);
        self.paint(
            &format!(" {:<5}{kind:<7}", label(id)),
            &format!("1;97;48;2;{r};{g};{b}"),
        )
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
        "{}{} {} imp urg rsk eff score {} status",
        pad("id", 6),
        pad("kind", 7),
        pad("title", title_width(s)),
        pad("needs", 16)
    );
    s.paint(&line, "2")
}

fn row_line(v: &ItemView, s: &Style) -> String {
    let id = if s.color {
        s.badge(&v.kind, &v.id)
    } else {
        format!("{}{}", pad(&label(&v.id), 6), pad(&v.kind, 7))
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
