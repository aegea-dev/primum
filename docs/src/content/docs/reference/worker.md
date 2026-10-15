---
title: The worker
description: Runs one action and reports its output - an HTTP request, a command, or a project's own program.
---

A worker runs one action for the [daemon](/reference/daemon/) and reports
its output; it decides nothing. The daemon starts one `primum worker` per
job, gives it the job as JSON on stdin, and reads `{"output": ...}` from
its stdout. A worker that cannot run the action exits with the reason on
stderr.

## The job

```json
{
  "action": "http.request@1",
  "with": { "url": "https://api.example.com/items/12", "headers": { "Authorization": "Bearer ..." } },
  "key": "12:e1.2:0",
  "ticket": { "id": "12", "title": "Gölge" },
  "project": {}, "args": {}, "input": {}, "prev": {},
  "root": "/home/ada/boards/videre",
  "timeout": 60
}
```

`with` arrives with its secrets filled in. `key` is the idempotency key:
the same when the action runs again, so an outside system can tell a rerun
from a new request.

## Built-ins

| Actionable | Does | Output |
|---|---|---|
| `http.request@1` | sends `method` (GET) to `url` with `headers`, and `json` or `body`; adds an `Idempotency-Key` header with the job's key unless one is given | `status`, `headers`, `text`, and `json` when the body is JSON |
| `command@1` | runs `run` (a program and its arguments) in the project's folder | `exit`, `stdout`, `stderr` |

An HTTP error status or a command's non-zero exit is still an output: the
action's `expect` (by default a 2xx status, or exit 0) decides whether it
worked. A request that cannot connect, or a program that cannot start, is a
failure.

An HTTP response is read up to 10 MB, or `with.max_bytes`: a larger one
fails the action (`the response is larger than ... bytes`). The body is read
as UTF-8.

## A project's `executable` actionable

The worker runs the target's `executable` (relative to the actionable's
folder when it has a `/`) with its `arguments`, in the actionable's folder
(or its `working_directory`), with its `environment`. The program gets one
line of JSON on stdin:

```json
{ "with": {}, "input": {}, "prev": {}, "ticket": {}, "project": {}, "args": {}, "key": "12:e1.2:0" }
```

and prints its output as JSON on stdout. A non-zero exit is a failure, with
the end of its stderr as the reason; so is output that is not JSON.

## A project's `python@` actionable

The function gets the same object as its one argument and returns its
output; it may be `async`:

```python
# actionables/checks/rules.py
def is_valid(job):
    title = job["ticket"]["title"]
    return {"ok": len(title) > 3, "length": len(title)}
```

The worker needs [`uv`](https://docs.astral.sh/uv/). The first time, it
makes the actionable's virtual environment at its `home` (`.venv` by
default) with `uv venv --python <version>`; when the actionable has
`requirements`, it installs them with `uv pip install -r`, and again
whenever the file changes (`.venv/.primum-requirements` holds what was
installed). Then it imports `module` from the actionable's folder and calls
`callable`. An exception fails the action, with its last lines as the
reason (`ValueError: ...`).

## A project's `bun@` actionable

```ts
// actionables/formatters/text.ts
export async function slug(job: { ticket: { title: string } }) {
  return { slug: job.ticket.title.toLowerCase().replace(/\s+/g, "-") };
}
```

The worker needs [`bun`](https://bun.sh), which runs TypeScript directly.
When the actionable has a `package.json`, it runs `bun install` the first
time and whenever `package.json` or `bun.lock` change. Then it imports
`file` and calls `export`, awaiting it. A thrown error fails the action;
so does a file with no such export.

A missing `uv` or `bun` fails the action with a message naming it and where
to get it. Neither `.venv` nor `node_modules` is in the project's history.
