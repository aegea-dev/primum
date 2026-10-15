# The demo

A small project for trying the lifecycle engine by hand, and the example
the kill tests (`tests/demo.rs`) run. Nothing here needs Python, Bun or a
network.

```bash
make demo            # a fresh workspace in target/demo, its daemon on :8790
make demo DEMO_PORT=9000
```

`make demo` builds primum, makes `target/demo` (the workspace) with the
project `demo`, and runs `primum daemon` in the foreground; Ctrl-C stops it.
Run it again for a fresh start. `examples/demo/setup.sh FOLDER` makes the
same workspace anywhere.

## What is in it

| | |
|---|---|
| `lifecycles/publish.json` | checks a ticket (`steps.check@1`), publishes it (`steps.publish@1`), notes where; the second step runs by itself |
| `lifecycles/hello.json` | waits for a reply; a minute without one makes it `late` |
| `actionables/steps/` | the two steps as shell scripts, with a 5 s heartbeat lease |

Tickets: #1 and #2 on `publish`, #3 on `hello`, #4 on no lifecycle.

`publish.sh` stands for an outside effect. Its outside world is the
project's `.primum/demo/` folder: `runs.log` gets a line for every run and
`effects.log` one line per idempotency key, so a job that runs again after
a loss publishes nothing twice. `DEMO_STEP` sets how long each of its three
steps takes (1 s by default).

## Things to try

In `target/demo/demo`, while the daemon runs:

```bash
primum list --all
primum events 1
cat .primum/demo/effects.log
```

Answer #3 before its minute is up, over HTTP or with a file:

```bash
curl -s localhost:8790/p/demo/api/events -H 'content-type: application/json' \
  -d '{"type": "input.received", "ticket": "3", "data": {"reply": "evet"}}'
echo evet > inbox/3.reply.txt
```

Put a new ticket on `publish`:

```bash
primum lifecycle attach "$(primum add TODO 'Yeni sayfa')" publish
```

Kill the daemon mid-publish (`kill -9`), start it again, and see the job run
again with nothing published twice.
