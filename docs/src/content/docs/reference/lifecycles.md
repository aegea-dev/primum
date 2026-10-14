---
title: Lifecycles
description: What a ticket goes through, step by step, as a file in the project. Being built; nothing runs them yet.
---

:::caution[In progress]
primum reads, checks and versions lifecycle files today. Running them (the
lifecycle engine) is being built; until then every ticket is moved by a
person, as the built-in `manual` lifecycle says.
:::

A lifecycle is a TOML file, `lifecycles/<name>.toml`, in the project. It
says what a ticket goes through: its steps in order, what each does, what
completes it, where it goes next, how it fails, and the ticket's state
meanwhile. Lifecycle files are tracked in the project's history.

## The built-in one: `manual`

Every ticket without a lifecycle follows `manual`: a person moves it
between `idle`, `doing`, `done` and `wontfix`. A ticket's status is always
one of its lifecycle's states; it is closed in the states of the
lifecycle's closing steps.

```toml
name = "manual"
description = "A person moves the ticket: idle, doing, done or wontfix."
start = "idle"
states = ["idle", "doing", "done", "wontfix"]

[[step]]
id = "idle"
state = "idle"
wait = { for = "input", when = "input.source == 'ledger' && input.field == 'state'" }
next = [
  { when = "input.value == 'doing'", go = "doing" },
  { when = "input.value == 'done'", go = "done" },
  { when = "input.value == 'wontfix'", go = "wontfix" },
  { go = "idle" },
]

[[step]]
id = "doing"
state = "doing"
wait = { for = "input", when = "input.source == 'ledger' && input.field == 'state'" }
next = [
  { when = "input.value == 'done'", go = "done" },
  { when = "input.value == 'wontfix'", go = "wontfix" },
  { when = "input.value == 'idle'", go = "idle" },
  { go = "doing" },
]

[[step]]
id = "done"
state = "done"
end = true
wait = { for = "input", when = "input.source == 'ledger' && input.field == 'state'" }
next = [ { when = "input.value != 'done'", go = "idle" }, { go = "done" } ]

[[step]]
id = "wontfix"
state = "wontfix"
end = true
wait = { for = "input", when = "input.source == 'ledger' && input.field == 'state'" }
next = [ { when = "input.value != 'wontfix'", go = "idle" }, { go = "wontfix" } ]
```

## An automated example

```toml
# lifecycles/stakeholder-update.toml
name = "stakeholder-update"
description = "Tell the stakeholders, wait for their answer, call the API, report back, close."
start = "notify"

# what the lifecycle reads, so a ticket or project can be checked before it starts
[needs]
stakeholders = "project.context.stakeholders"   # a list of addresses
api_url = "project.context.api_url"

[[step]]
id = "notify"
state = "effects"
do = "command@1"                                 # an actionable and its version
with = { run = ["./notify.sh", "${ticket.id}", "${ticket.title}"] }
target = "output.exit == 0"
next = "await-reply"

[[step]]
id = "await-reply"
state = "waiting"
wait = { for = "input", when = "input.source == 'http' && input.ticket == ticket.id", timeout = "7d" }
on_timeout = "notify"                            # remind: send again
next = "call-api"

[[step]]
id = "call-api"
state = "processing"
do = "http@1"
with = { method = "POST", url = "${needs.api_url}/updates", json = { ticket = "${ticket.id}", reply = "${steps['await-reply'].input.text}" } }
target = "output.status >= 200 && output.status < 300"
retry = { attempts = 3, backoff = "30s" }
next = "mark-doing"

[[step]]
id = "mark-doing"
do = "ledger@1"
with = { state = "doing", note = "Stakeholders answered; API updated." }
next = "await-input"

[[step]]
id = "await-input"
state = "waiting"
wait = { for = "input", when = "input.source == 'ledger' && input.ticket == ticket.id && input.field == 'note'" }
next = "report"

[[step]]
id = "report"
state = "effects"
do = "command@1"
with = { run = ["./notify.sh", "${ticket.id}", "${steps['await-input'].input.text}"] }
target = "output.exit == 0"
next = "approve"

[[step]]
id = "approve"
state = "waiting"
wait = { for = "approval", recommend = "finish", timeout = "2d" }   # continue with the recommendation after 2 days
next = [
  { when = "input.approved", go = "finish" },
  { go = "await-input" },
]

[[step]]
id = "finish"
do = "ledger@1"
with = { state = "done" }
end = true
```

## Steps

- `start` names the first step. Each step has a unique `id` (lowercase
  letters, digits and `-`), and `next`, or `end = true`, or both.
- `next` is a step, or a list of branches tried in order:
  `{ when = <expression>, go = <step> }`, the last without `when`.
- `end = true` marks a closing step: the ticket is closed there. A closing
  step may still wait and go back (reopening).
- A step does one thing, or waits:
  - `do = "<actionable>@<version>"` with `with = { ... }`. Known now:
    `command@1` (`with.run`, a list of strings), `http@1` (`with.method`,
    `with.url`), `ledger@1` (one or more of `state`, `note`, `field`,
    `add`).
  - `wait = { for = "input" | "approval", when, timeout, recommend }`.
- `target` is the condition that completes a step. Without one, a step is
  complete when its action finishes without error.
- Errors: `retry = { attempts, backoff }`, `timeout`, `on_error`,
  `on_timeout`. Without them: the error is logged and the lifecycle stops
  in `failed`.
- `state` is the ticket's state while the step runs: by default `idle`,
  `waiting`, `processing`, `effects`, `done` or `wontfix`, or the
  lifecycle's own `states`.

## Expressions and durations

Conditions are [CEL](https://cel.dev) expressions. They see `ticket`,
`project`, `needs`, `steps` (each finished step's `output` or `input`) and
the current `output` or `input`. Inside strings, `${...}` inserts a value:
`"${ticket.id}"`. Durations are like `30s`, `5m`, `2h`, `7d` or `1h 30m`.

## Versions

Every step has a version (a SHA-256 of its fields), and so does the whole
lifecycle (of its header and its steps' versions). Changing any step, or
an actionable's version, makes a new lifecycle version; comments and
formatting do not. A ticket will finish on the version it started with.

## Attaching a ticket

```bash
primum lifecycle attach 12 stakeholder-update   # pin the current version
primum lifecycle detach 12                      # back to manual
```

`attach` writes two fields into the ticket, `lifecycle:
stakeholder-update@<version>` and `step: notify` (the lifecycle's start),
and sets its status to the start's state. The ticket keeps that version
even when the file changes. `detach` removes both; a status `manual` does
not have becomes `idle`.

## Commands

```bash
primum lifecycle check                     # every file in lifecycles/
primum lifecycle check stakeholder-update  # one, by name or path
primum lifecycle show manual               # its steps, in order
primum lifecycle list                      # names and versions
```

`check` stops at the first problem and names the file and the step:
`lifecycles/stakeholder-update.toml: step "call-api": next "mark-doin" is
not a step`.
