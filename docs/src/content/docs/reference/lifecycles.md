---
title: Lifecycles
description: The steps a ticket goes through, from created to closed, as a JSON file in the project. Being built; nothing runs them yet.
---

:::note
A ticket on a lifecycle moves by itself while
[`primum daemon`](/reference/daemon/) runs, through every built-in
actionable and the project's own (`executable`, `python@`, `bun@`).
:::

A lifecycle is the list of steps one ticket goes through, from created to
closed: its **states**, and **transitions** from one state to another. A
transition runs a chain of **actions**. A lifecycle is a JSON file,
`lifecycles/<name>.json`, tracked in the project's history.

## The built-in one: `manual`

Every ticket without a lifecycle follows `manual`. It has no transitions:
a person moves the ticket between its states, and nothing runs by itself.

```json
{
  "name": "manual",
  "description": "A person moves the ticket: idle, doing, done or wontfix.",
  "states": ["idle", "doing", "done", "wontfix"],
  "start": "idle",
  "end": ["done", "wontfix"],
  "transitions": []
}
```

## An example

Fetch data from an API, then fill the ticket in and check it:

```json
{
  "name": "enrich-ticket",
  "description": "Fetch data from an API and fill the ticket in.",
  "states": ["create", "idle", "waiting", "processing", "finished", "timed-out"],
  "start": "create",
  "end": ["finished", "timed-out"],
  "transitions": [
    { "from": "create", "on": "ticket.created", "to": "idle" },
    { "from": "idle", "to": "processing", "while": "waiting",
      "timeout": "5m", "on_timeout": "timed-out",
      "do": [
        { "action": "http.request@1",
          "with": { "url": "https://api.example.com/items/${ticket.id}",
                    "headers": { "Authorization": "Bearer ${env.API_KEY}" } },
          "expect": "output.status == 200 && has(output.json.data)" }
      ] },
    { "from": "processing", "to": "finished",
      "do": [
        { "action": "json.pick@1", "with": { "path": "json.data" } },
        { "action": "ticket.update@1",
          "with": { "title": "${prev.name}", "note": "Filled in from the API." } },
        { "action": "checks.is-valid@1", "with": { "ticket": "${ticket}" },
          "expect": "output.ok" }
      ] }
  ]
}
```

## The file

| Key | What |
|---|---|
| `name` | the lifecycle's name; matches the file name |
| `description` | text for people; never counts toward the version |
| `states` | every state a ticket can be in, in order: lowercase letters, digits and `-` |
| `start` | the state a ticket begins in |
| `end` | the closing states: a ticket there is closed (it leaves `list`, `next` and other tickets' `needs`) |
| `transitions` | the moves between states |

`failed` is the engine's own state: never listed, and possible for any
ticket on a lifecycle that has transitions.

## Transitions

| Key | What |
|---|---|
| `from`, `to` | states |
| `on` | the event that triggers it, like `ticket.created`, `lifecycle.attached`, `ticket.changed` or `input.received`; without `on`, it starts as soon as the ticket enters `from` |
| `when` | a condition that must hold for it to be taken |
| `do` | a chain of actions, run in order |
| `while` | the status shown while the chain runs (by default, `from`) |
| `timeout`, `on_timeout` | if it has not completed in time, the ticket goes to `on_timeout` |
| `on_fail` | where a failed chain goes instead of `failed` |
| `description` | text for people |

- Transitions leaving one state are tried in file order; the first whose
  `on` and `when` match is taken.
- A state with no matching transition is a resting state: the ticket waits
  there for an event or a person.
- A transition is all or nothing: ticket changes from its chain are applied
  with the new state only when the whole chain succeeds. Later actions in
  the chain already see them.
- A person can always set any of the ticket's states; a chain running then
  is dropped.

## Actions

An action is a name, like `"json.pick@1"`, or an object:

| Key | What |
|---|---|
| `action` | `<name>@<version>`: a built-in, or the project's own (below) |
| `with` | its arguments: static values, or `${...}` expressions |
| `expect` | a condition over its `output`; false means it failed |
| `description` | text for people |

Expressions are [CEL](https://cel.dev). They see `ticket`, `project` (its
context), `args` (the ticket's lifecycle arguments), `input` (the
triggering event's data) and `prev` (the previous action's output; for the
first action, the event's data); `expect` also sees `output`. A string that
is one `${expr}` keeps the value's type (`"${prev}"` passes an object);
otherwise values are inserted as text. `${env.NAME}` is a secret, filled in
only when the action runs. Durations are like `30s`, `5m`, `2h`, `7d` or
`1h 30m`.

Without `expect`, `http.request@1` fails on a status outside 200-299 and
`command@1` on a non-zero exit; other actions fail only when they cannot
run. A check more complex than an expression is another action in the
chain.

## Built-in actionables

| Actionable | `with` | Output |
|---|---|---|
| `ticket.update@1` | any of `title`, `impact`, `urgency`, `risk`, `effort`, `needs`, `body`, `note` (appended) | the changes |
| `json.pick@1` | `path`, dotted, into `prev` | the value |
| `event.emit@1` | `type`, `target` (a ticket id or `project`), `data` | the event |
| `http.request@1` | `url`; `method` (GET), `headers`, `json` or `body`, `max_bytes` (10 MB) | `status`, `headers`, `json`, `text` |
| `command@1` | `run`: a program and its arguments | `exit`, `stdout`, `stderr` |

## A project's own actionables

Code of the project's own, in any language, lives in
`actionables/<name>/` beside its definition, `actionable.json` (tracked in
the history; `.venv/`, `node_modules/` and `__pycache__/` are not):

```json
{
  "name": "checks",
  "version": 1,
  "description": "Checks on data from outside.",
  "type": "python@3.12",
  "requirements": "requirements.txt",
  "environment": { "API_KEY": "${env.API_KEY}" },
  "limits": { "timeout": "30s" },
  "targets": {
    "is-valid": { "module": "rules", "callable": "is_valid" },
    "score": { "module": "rules", "callable": "score" }
  }
}
```

A lifecycle calls it as `<name>.<target>@<version>`: `checks.is-valid@1`.

- Every type: `name` (the folder's name), `version`, `description`,
  `type`, `working_directory` (by default the actionable's folder),
  `environment`, `limits.timeout`, `limits.heartbeat` (see
  [lost jobs](/reference/daemon/#lost-jobs-run-again)), `targets`.
- `python@<version>`: `home` (the virtual environment) and
  `requirements`; a target is a `module` and a `callable`.
- `bun@<version>`: dependencies from `package.json`; a target is a `file`
  and an `export`.
- `executable`: any program; a target is an `executable` and its
  `arguments`.

The function receives one JSON object (`with`, `input`, `prev`, `ticket`,
`project`, `args`) and returns its output as JSON.

## Versions

A lifecycle's version is a SHA-256 over its content as canonical JSON,
without descriptions, together with the definitions of the project
actionables it names. Changing a transition, an action, an argument or an
actionable's definition makes a new version; formatting, key order and
descriptions do not. A ticket keeps the version it was attached at.

## Attaching a ticket

```bash
primum lifecycle attach 12 enrich-ticket --arg region=eu   # pin the current version
primum lifecycle detach 12                                 # back to manual
```

`attach` writes `lifecycle: enrich-ticket@<version>` into the ticket, and
`args:` when given (a value that reads as JSON stays JSON: `--arg tries=3`
is a number), and sets its status to the lifecycle's `start`. `detach`
removes both; a status `manual` does not have becomes `idle`.

## Checks

`primum lifecycle check` stops at the first problem and names the file and
the transition:
`lifecycles/enrich-ticket.json: transition "idle -> processing": to "procesing" is not a state`.
It checks that:

- the JSON is valid and every key is known;
- `name` matches the file; states are unique lowercase words and `failed`
  is not one; `start` and every `end` are states;
- every `from`, `to`, `while`, `on_timeout` and `on_fail` is a state; a
  transition has `on`, `do` or both; `on` is an event type like
  `ticket.created`; `timeout` parses and comes with `on_timeout`;
- every action is known at its version, and `with` has what it needs;
- every expression and `${...}` compiles;
- a project actionable's definition is complete for its type.

It also warns, without failing, about a state no transition enters or
leaves: only a person can move a ticket there.

## Commands

```bash
primum lifecycle check                  # every file in lifecycles/
primum lifecycle check enrich-ticket    # one, by name or path
primum lifecycle show enrich-ticket     # its states and transitions
primum lifecycle list                   # names and versions
```
