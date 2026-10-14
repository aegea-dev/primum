---
title: The daemon
description: Runs tickets' lifecycles - turns ledger changes into events, asks the manager, and writes the results back. Being built.
---

:::caution[In progress]
The daemon runs lifecycles whose actions are built-ins that need no worker
(`ticket.update@1`, `json.pick@1`, `event.emit@1`), waits and timers.
Actions that need a worker (`http.request@1`, `command@1`, a project's own
actionables) wait in the journal until workers exist. Running as a service,
the watchdog and an HTTP port for outside events come later.
:::

```bash
primum daemon            # every project here, a pass every second
primum daemon --once     # one pass, then stop
primum daemon --interval 5
```

Run in a workspace, the daemon runs every project in it; `-p NAME` runs
one. It decides nothing itself: every change to a ticket comes from the
[lifecycle manager](/reference/manager/)'s answer, which the daemon saves
and carries out.

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
   and saves the answer in one transaction.
5. **Writes the ticket changes** the answer asks for to the ledger, each
   as a commit in the project's history.

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
| `ledger` | ticket changes waiting to be written |
| `seen` | what the daemon last saw of each item |

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
