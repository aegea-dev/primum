---
title: The daemon
description: Runs tickets' lifecycles - turns ledger changes into events, asks the manager, and writes the results back.
---

```bash
primum daemon               # every project here; serves the board on 8777
primum daemon --port 9000
primum daemon --no-serve    # the passes only
primum daemon --once        # one pass, then stop
primum daemon --interval 5  # seconds between passes (1 by default)
```

Run in a workspace, the daemon runs every project in it; `-p NAME` runs
one. It decides nothing itself: every change to a ticket comes from the
[lifecycle manager](/reference/manager/)'s answer, which the daemon saves
and carries out.

## As a service

```bash
primum daemon install       # start it now and at every login (--port, --interval)
primum daemon status        # the service's state and the daemon's health
primum daemon uninstall
```

`install` registers a per-user service in the workspace (a launchd agent
on macOS, a systemd user unit on Linux), one per workspace. It runs
`primum watchdog`, which starts the daemon and checks its `/health` every
10 seconds, and restarts it when:

- it has exited;
- three checks in a row get no answer (after 30 seconds to start);
- its last pass is more than 10 minutes old (a pass that hangs).

It waits 1 second before a restart, doubling up to a minute while the
daemon keeps failing, and back to 1 second once one has run 5 minutes. If
the watchdog itself ends, the OS starts it again. Its lines, and the
daemon's stderr, go to `.primum-daemon.log` in the workspace.

A service starts with a bare `PATH`, without the folders where `git`, `uv`
or `bun` live: `install` keeps the `PATH` of the shell that ran it. Install
again after changing it.

## What it serves

On 127.0.0.1 only (outside webhooks need a relay or a tunnel to reach it):

- **the board**, as [`primum serve`](/commands/serve/) serves it;
- **`GET /health`**: `{"ok": true, "pid", "projects": [...], "started",
  "passes", "last_pass", "last_error"}`;
- **`POST /p/<project>/api/events`**: an event from outside, saved in the
  journal; the daemon runs a pass at once.

```bash
curl -s localhost:8777/p/videre/api/events \
  -H 'content-type: application/json' \
  -d '{"type": "input.received", "ticket": "12", "data": {"reply": "yes"}}'
# {"id": "0192f3c4-..."}   (202 Accepted)
```

| Field | What |
|---|---|
| `type` | lowercase words joined by dots: `input.received`, `lifecycle.resume`, `campaign.started` |
| `ticket` | a ticket on a lifecycle (`12` or `#12`); leave it out for the project |
| `data` | a JSON object (`{}` when left out) |

Refused: the engine's own types (`action.*`, `timer.*`, `ticket.*`,
`lifecycle.attached`, so nothing outside can forge a worker's result, a
timer or a change to the ledger), a ticket not on a lifecycle (409), an
unknown ticket or project (404), and anything malformed (400). Errors come
as `{"error": "..."}`.

## The inbox

A file dropped in a project's `inbox/` becomes an `input.received` event on
the next pass, with `{"file": "inbox/done/<name>", "name": "<name>"}`, and
moves to `inbox/done/` (a name already there gets `-2`, `-3`, ...). A file
named `<id>.<anything>` (`12.reply.txt`) goes to that ticket when it is on
a lifecycle; any other goes to the project. Files starting with `.` are left
alone, so write a file under a dot-name and rename it when it is complete
(what `rsync` does). The inbox is not in the project's history.

## A pass

1. **Writes ticket changes left from before**, if a pass stopped early.
2. **Reads the ledger.** Every item is compared with what the daemon saw
   last. A ticket newly on a lifecycle (attached, or attached again) starts
   at the lifecycle's `start` and gets `lifecycle.attached`; any other
   change to a ticket on a lifecycle is a `ticket.changed` event. A person
   setting a status counts as moving the ticket. Tickets on `manual` make
   no events.
3. **Fires due timers** as `timer.fired` events.
4. **Delivers every waiting event** to its ticket's manager, with the
   pinned lifecycle, the ticket, its arguments and the project's context,
   and saves the answer in one transaction. If the ticket or the project
   changed while the manager ran, it is asked again; if they keep
   changing, the event waits for the next pass (the pass reports it as
   waiting) rather than saving an answer made from what is no longer there.
5. **Writes the ticket changes** the answer asks for to the ledger, each
   as a commit in the project's history. A change the ledger refuses is
   tried again on the next passes, and the ticket's later changes wait
   behind it; after three tries it is dropped, and a note on the ticket
   says what was dropped and why.
6. **Runs the jobs** the answers asked for, each in a
   [worker](/reference/worker/): up to 8 side by side, one per ticket (a
   ticket's jobs run in order), and delivers each result to its ticket
   (back to step 4) until nothing is left.

## One machine per project

A project synced to several machines runs its lifecycles on one of them,
so no action runs twice. The first daemon to run a project writes
`"runner": "<this machine's name>"` to its `project.json` (a commit that
syncs like any other). A daemon on another machine skips that project:

```
videre: runs on studio-mac; skipped (--take-over runs it here)
```

`primum daemon --take-over` runs it anyway and makes this machine its
runner. Take over only once the other machine's daemon has stopped.

## Workers, secrets and time limits

Each job runs as its own process, `primum worker`, in its own process
group: whatever it starts goes with it. At its time limit the group is
killed and the action fails (`timed out after 60s`). Built-ins have 60
seconds; a project actionable's `limits.timeout` sets its own. Nothing
extends it.

## Lost jobs run again

A job is lost when its worker ends without an answer: it was killed from
outside, its program died from a signal, its lease ran out, or the daemon
stopped while it ran (a worker stops when its daemon does). A lost job runs
again with the same idempotency key, up to 3 runs in all, counted in the
journal across restarts; the third loss fails the action (`lost 3 times:
no heartbeat for 30s`). An error is not a loss: an answer saying it failed,
a failed `expect`, a missing secret or the time limit fails the action at
once.

A long job can hold a lease: an actionable's `limits.heartbeat` (`"30s"`)
is how long the job may go without touching the file named in its
`PRIMUM_HEARTBEAT` (see [the worker](/reference/worker/#heartbeats)). One
that does not is killed and counted lost. `limits.timeout` still caps it,
so set both for a long job:

```json
"limits": { "timeout": "2h", "heartbeat": "30s" }
```

`${env.NAME}` in an action's arguments, or in an actionable's
`environment`, is filled in only when the job starts: from the OS secret
store ([`primum secret`](/commands/secret/)), else from the daemon's own
environment. It reaches only the worker: never the journal, the events or a
log. A missing one fails the action (`secret API_KEY is not set`).

An action that ran comes back as `action.finished` with its output, and its
`expect` decides whether it worked; one that could not run (a missing
secret, a connection refused, a time limit) comes back as `action.failed`
with the reason. Either way the ticket's lifecycle carries on from there.

A ticket keeps the lifecycle version it was attached at: the daemon keeps a
copy of every pinned version. A version no longer in `lifecycles/` when the
ticket starts cannot run: the ticket is set `failed` with a note to attach
it again.

## The journal

The daemon's own record, `.primum/journal.sqlite` in each project: local to
the machine, never in the project's history or synced.

| Table | Holds |
|---|---|
| `events` | every event, in order: its type, subject, time, data, what caused it, and when it was handled (or the error) |
| `tickets` | each running ticket's place: state, the running transition, where it failed |
| `lifecycles` | every pinned lifecycle version |
| `timers` | timers waiting to fire |
| `ledger` | ticket changes waiting to be written, with failed attempts and why |
| `seen` | what the daemon last saw of each item |
| `jobs` | how many times each job has started, and why its last run was lost |
| `meta` | the journal's schema version |

An older journal is brought up to date when a newer primum opens it; a
journal from a newer primum is refused rather than misread.

Events are saved before they are handled, and an answer is saved all at
once or not at all. After a crash, the daemon carries on: waiting events are
delivered again and unwritten ticket changes are written (a note already in
the ticket is not added twice).

## Commands

```bash
primum events 12            # ticket 12's events, in order
primum events --json        # every event
primum lifecycle resume 12  # run a failed ticket's failed transition again
```

`resume` puts a `lifecycle.resume` event into the journal; the next pass
delivers it.
