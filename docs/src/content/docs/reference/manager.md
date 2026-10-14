---
title: The lifecycle manager
description: The decision behind every lifecycle step, for one ticket and one event. Being built; nothing calls it yet.
---

:::caution[In progress]
The manager decides; the daemon that will call it, save its answers and run
the actions it asks for is being built. Until then nothing runs lifecycles.
:::

The lifecycle manager decides what one event does to one ticket on its
[lifecycle](/reference/lifecycles/). It is pure: it reads and writes
nothing, never reads a clock, and the same input always gives the same
answer, so it can run again at any time and many can run at once.

```bash
primum manager < input.json > answer.json
```

## Input

```json
{
  "now": "2026-10-14T20:00:00Z",
  "lifecycle": { "name": "enrich-ticket", "states": ["create", "idle"], "...": "..." },
  "ticket": { "id": "12", "title": "Gölge", "status": "create" },
  "args": { "region": "eu" },
  "project": { "owner": "Ada" },
  "runtime": { "state": "create" },
  "event": { "id": "e1", "type": "ticket.created", "data": {} }
}
```

| Key | What |
|---|---|
| `now` | the time, RFC 3339 |
| `lifecycle` | the ticket's lifecycle at its pinned version |
| `ticket` | the ticket's fields, `id` and `status` included |
| `args` | the ticket's lifecycle arguments |
| `project` | the project's context |
| `runtime` | where the ticket is: `state`, the running `chain`, and `failed` (where it failed) |
| `event` | what happened: `id`, `type`, `data` |

## Answer

```json
{
  "runtime": { "state": "idle", "chain": { "run": "e1.2", "transition": 1, "index": 0, "...": "..." } },
  "status": "waiting",
  "events": [
    { "type": "action.requested", "target": "worker",
      "data": { "action": "http.request@1", "key": "12:e1.2:0", "with": { "url": "https://api.example.com/items/12" } } }
  ],
  "timers": [ { "at": "2026-10-14T20:05:00Z", "data": { "run": "e1.2", "transition": 1 } } ]
}
```

| Key | What |
|---|---|
| `runtime` | where the ticket is after the event |
| `status` | what the ticket's status shows |
| `changes` | ticket changes to apply now |
| `events` | events to send: `action.requested` for a worker, and events from `event.emit@1` |
| `timers` | the ticket's timers from now on; absent means unchanged |
| `log` | what happened, for people |

## Events it understands

- `action.finished`, `action.failed` (`run`, `index`, and `output` or
  `error`): the result of the running action. A result for another run or
  action is stale: logged, nothing else.
- `timer.fired`: a transition timed out; the ticket goes to its
  `on_timeout`.
- `lifecycle.resume`: a `failed` ticket runs its failed transition again,
  with the same idempotency keys.
- `ticket.changed` with a `status` that is one of the lifecycle's states: a
  person moved the ticket; a running chain is dropped.
- Anything else triggers the first transition from the current state whose
  `on` is the event's type and whose `when` holds.

## The rules

- A transition without actions moves at once. One with actions starts a
  chain: `ticket.update@1`, `json.pick@1` and `event.emit@1` run inside the
  manager; the first other action is sent to a worker, and the manager
  waits for its result.
- Each action's arguments are filled in from `ticket` (with the chain's
  changes so far), `project`, `args`, `input` and `prev`; `${env.NAME}`
  stays for the daemon.
- `expect` checks each output; without it, `http.request@1` expects a 2xx
  status and `command@1` exit 0.
- When the chain ends, its ticket changes and events are released together
  and the ticket enters the next state. Entering a state takes the first
  transition without `on` whose `when` holds; otherwise the ticket waits,
  with a timer for each waiting transition that has a `timeout`.
- A failure goes to the transition's `on_fail`, or leaves the ticket
  `failed` with a note saying where and why; nothing from the chain is
  applied.
- More than 100 moves in one decision is a loop: the ticket fails.
