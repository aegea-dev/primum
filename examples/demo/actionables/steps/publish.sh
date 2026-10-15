#!/bin/sh
# Publishes a ticket: the demo's outside effect. The outside world here is
# the project's .primum/demo/ folder (local, not in the project's history):
# runs.log gets a line for every run, effects.log one line per idempotency
# key, so a run again after a loss publishes nothing twice.
job=$(cat)
key=$(printf '%s' "$job" | sed -n 's/.*"key":"\([^"]*\)".*/\1/p')
out=../../.primum/demo
mkdir -p "$out"
echo "$key" >> "$out/runs.log"
# the work, renewing the lease as it goes (DEMO_STEP seconds a step)
for step in 1 2 3; do
  [ -n "$PRIMUM_HEARTBEAT" ] && touch "$PRIMUM_HEARTBEAT"
  sleep "${DEMO_STEP:-1}"
done
grep -qxF "$key" "$out/effects.log" 2>/dev/null || echo "$key" >> "$out/effects.log"
echo "{\"published\": \"$key\"}"
