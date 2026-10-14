//! The lifecycle manager: given a ticket's lifecycle, its place in it and
//! one event, decide what happens next. Pure: it reads and writes nothing,
//! and the same input always gives the same answer. The daemon saves the
//! answer and carries it out.
use super::eval::{Vars, render, truth};
use super::{Answer, Chain, Event, Failed, Input, OutEvent, Runtime, Timer, builtins};
use crate::lifecycle::FAILED;
use crate::lifecycle::format::Transition;
use serde_json::{Map, Value, json};

/// More moves than this in one decision is a loop.
const MAX_MOVES: usize = 100;

/// Decides what one event does to one ticket.
pub fn decide(input: &Input) -> Result<Answer, String> {
    let now = chrono::DateTime::parse_from_rfc3339(&input.now)
        .map_err(|_| format!("now must be an RFC 3339 time, not `{}`", input.now))?;
    let status = input
        .ticket
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or(&input.runtime.state)
        .to_string();
    let mut m = Machine {
        i: input,
        now,
        rt: input.runtime.clone(),
        status,
        changes: Map::new(),
        events: vec![],
        timers: None,
        log: vec![],
        moves: 0,
    };
    m.handle()?;
    Ok(Answer {
        runtime: m.rt,
        status: m.status,
        changes: m.changes,
        events: m.events,
        timers: m.timers,
        log: m.log,
    })
}

struct Machine<'a> {
    i: &'a Input,
    now: chrono::DateTime<chrono::FixedOffset>,
    rt: Runtime,
    status: String,
    changes: Map<String, Value>,
    events: Vec<OutEvent>,
    timers: Option<Vec<Timer>>,
    log: Vec<String>,
    moves: usize,
}

/// Adds changes to others: a note is appended, anything else replaced.
fn merge(into: &mut Map<String, Value>, from: &Map<String, Value>) {
    for (k, v) in from {
        match (k.as_str(), into.get(k), v) {
            ("note", Some(Value::String(a)), Value::String(b)) => {
                let joined = format!("{a}\n\n{b}");
                into.insert(k.clone(), Value::String(joined));
            }
            _ => {
                into.insert(k.clone(), v.clone());
            }
        }
    }
}

impl Machine<'_> {
    fn transition(&self, i: usize) -> Result<&Transition, String> {
        self.i
            .lifecycle
            .transitions
            .get(i)
            .ok_or_else(|| format!("the lifecycle has no transition {i}"))
    }

    fn label(t: &Transition) -> String {
        format!("{} -> {}", t.from, t.to)
    }

    /// The ticket as expressions see it: with the running chain's held-back
    /// changes (a note is not a field).
    fn ticket(&self) -> Map<String, Value> {
        let mut t = self.i.ticket.clone();
        if let Some(c) = &self.rt.chain {
            for (k, v) in &c.changes {
                if k != "note" {
                    t.insert(k.clone(), v.clone());
                }
            }
        }
        t
    }

    fn at(&self, d: &str) -> Result<String, String> {
        let d = crate::lifecycle::expr::duration(d)?;
        let d = chrono::Duration::from_std(d).map_err(|e| e.to_string())?;
        Ok((self.now + d).to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
    }

    fn stale(&mut self, why: &str) {
        self.log.push(format!("stale: {why}"));
    }

    fn handle(&mut self) -> Result<(), String> {
        let i = self.i;
        let e = &i.event;
        match e.kind.as_str() {
            "action.finished" | "action.failed" => self.result(e),
            "timer.fired" => self.timeout(e),
            "lifecycle.resume" => self.resume(),
            "ticket.changed" if self.person_move(e).is_some() => {
                let s = self.person_move(e).expect("checked");
                self.log.push(format!("moved to {s} by a person"));
                self.rt.chain = None;
                self.enter(&s, &e.data)
            }
            _ => self.trigger(e),
        }
    }

    /// The state a person set, when it is one of the lifecycle's and not
    /// the current one.
    fn person_move(&self, e: &Event) -> Option<String> {
        let s = e.data.get("status")?.as_str()?;
        (s != self.rt.state && self.i.lifecycle.states.iter().any(|x| x == s))
            .then(|| s.to_string())
    }

    fn vars<'v>(
        &'v self,
        ticket: &'v Map<String, Value>,
        input: &'v Value,
        prev: &'v Value,
        output: Option<&'v Value>,
    ) -> Vars<'v> {
        Vars {
            ticket,
            project: &self.i.project,
            args: &self.i.args,
            input,
            prev,
            output,
        }
    }

    fn trigger(&mut self, e: &Event) -> Result<(), String> {
        if self.rt.chain.is_some() {
            self.log
                .push(format!("{} ignored: a transition is running", e.kind));
            return Ok(());
        }
        if self.rt.state == FAILED {
            self.log
                .push(format!("{} ignored: the ticket failed", e.kind));
            return Ok(());
        }
        let ticket = self.ticket();
        let mut found = None;
        for (i, t) in self.i.lifecycle.transitions.iter().enumerate() {
            if t.from != self.rt.state || t.on.as_deref() != Some(e.kind.as_str()) {
                continue;
            }
            let ok = match &t.when {
                Some(w) => truth(w, &self.vars(&ticket, &e.data, &e.data, None))?,
                None => true,
            };
            if ok {
                found = Some(i);
                break;
            }
        }
        match found {
            Some(i) => self.take(i, e.id.clone(), e.data.clone()),
            None => {
                self.log.push(format!(
                    "no transition from {} on {}",
                    self.rt.state, e.kind
                ));
                Ok(())
            }
        }
    }

    fn take(&mut self, i: usize, run: String, input: Value) -> Result<(), String> {
        self.moves += 1;
        let t = self.transition(i)?.clone();
        if self.moves > MAX_MOVES {
            return self.fail_at(
                i,
                run,
                input,
                "more than 100 moves without waiting; is there a loop?".into(),
            );
        }
        if t.actions.is_empty() {
            return self.enter(&t.to, &input);
        }
        self.timers = Some(match &t.timeout {
            Some(d) => vec![Timer {
                at: self.at(d)?,
                data: json!({"run": run, "transition": i}),
            }],
            None => vec![],
        });
        self.status = t.showing.clone().unwrap_or_else(|| t.from.clone());
        self.rt.chain = Some(Chain {
            run,
            transition: i,
            index: 0,
            prev: input.clone(),
            input,
            changes: Map::new(),
            emits: vec![],
        });
        self.advance()
    }

    /// Runs pure actions until one needs a worker or the chain ends.
    fn advance(&mut self) -> Result<(), String> {
        loop {
            let c = self.rt.chain.clone().expect("a chain is running");
            let t = self.transition(c.transition)?.clone();
            let Some(a) = t.actions.get(c.index) else {
                return self.complete();
            };
            let ticket = self.ticket();
            let with = match &a.with {
                Some(w) => match render(
                    &Value::Object(w.clone()),
                    &self.vars(&ticket, &c.input, &c.prev, None),
                ) {
                    Ok(Value::Object(o)) => o,
                    Ok(_) => Map::new(),
                    Err(m) => return self.fail(format!("{}: {m}", a.action)),
                },
                None => Map::new(),
            };
            if !builtins::is_pure(&a.action) {
                let key = format!(
                    "{}:{}:{}",
                    self.i
                        .ticket
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or(""),
                    c.run,
                    c.index
                );
                self.events.push(OutEvent {
                    kind: "action.requested".into(),
                    target: "worker".into(),
                    data: json!({
                        "run": c.run,
                        "index": c.index,
                        "action": a.action,
                        "with": with,
                        "key": key,
                        "ticket": ticket,
                        "project": self.i.project,
                        "args": self.i.args,
                        "input": c.input,
                        "prev": c.prev,
                    }),
                });
                return Ok(());
            }
            let done = match builtins::run(&a.action, &with, &c.prev) {
                Ok(d) => d,
                Err(m) => return self.fail(m),
            };
            {
                let chain = self.rt.chain.as_mut().expect("a chain is running");
                merge(&mut chain.changes, &done.changes);
                if let Some((kind, target, data)) = done.emit {
                    chain.emits.push(OutEvent { kind, target, data });
                }
            }
            if let Err(m) = self.check(&a.action, a.expect.as_deref(), &done.output) {
                return self.fail(m);
            }
            let chain = self.rt.chain.as_mut().expect("a chain is running");
            chain.prev = done.output;
            chain.index += 1;
        }
    }

    /// Whether an action's output passes its check.
    fn check(&self, action: &str, expect: Option<&str>, output: &Value) -> Result<(), String> {
        let Some(src) = expect.or(builtins::default_expect(action)) else {
            return Ok(());
        };
        let c = self.rt.chain.as_ref().expect("a chain is running");
        let ticket = self.ticket();
        match truth(src, &self.vars(&ticket, &c.input, &c.prev, Some(output))) {
            Ok(true) => Ok(()),
            Ok(false) => Err(format!("{action}: expect failed: {src}")),
            Err(m) => Err(format!("{action}: {m}")),
        }
    }

    fn complete(&mut self) -> Result<(), String> {
        let c = self.rt.chain.take().expect("a chain is running");
        let t = self.transition(c.transition)?.clone();
        merge(&mut self.changes, &c.changes);
        self.events.extend(c.emits);
        self.log.push(format!("{} done", Self::label(&t)));
        self.enter(&t.to, &c.prev)
    }

    fn enter(&mut self, s: &str, input: &Value) -> Result<(), String> {
        self.rt.state = s.to_string();
        self.rt.failed = None;
        self.status = s.to_string();
        self.timers = Some(vec![]);
        let ticket = self.ticket();
        let mut auto = None;
        let mut waits = vec![];
        for (i, t) in self.i.lifecycle.transitions.iter().enumerate() {
            if t.from != s {
                continue;
            }
            if t.on.is_none() {
                let ok = match &t.when {
                    Some(w) => truth(w, &self.vars(&ticket, input, input, None))?,
                    None => true,
                };
                if ok && auto.is_none() {
                    auto = Some(i);
                }
            } else if let Some(d) = &t.timeout {
                waits.push(Timer {
                    at: self.at(d)?,
                    data: json!({"state": s, "transition": i}),
                });
            }
        }
        match auto {
            Some(i) => {
                // a transition taken automatically: `<event id>.<move>`
                let run = format!("{}.{}", self.i.event.id, self.moves + 1);
                self.take(i, run, input.clone())
            }
            None => {
                self.timers = Some(waits);
                Ok(())
            }
        }
    }

    fn result(&mut self, e: &Event) -> Result<(), String> {
        let run = e.data.get("run").and_then(Value::as_str).unwrap_or("");
        let index = e
            .data
            .get("index")
            .and_then(Value::as_u64)
            .map(|i| i as usize);
        let Some(c) = self.rt.chain.clone() else {
            self.stale("no transition is running");
            return Ok(());
        };
        if c.run != run || Some(c.index) != index {
            self.stale(&format!(
                "result of {run} #{index:?}, running {} #{}",
                c.run, c.index
            ));
            return Ok(());
        }
        let t = self.transition(c.transition)?.clone();
        let a = t
            .actions
            .get(c.index)
            .ok_or_else(|| format!("transition {} has no action {}", c.transition, c.index))?;
        if e.kind == "action.failed" {
            let why = e
                .data
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("the action failed");
            return self.fail(format!("{}: {why}", a.action));
        }
        let output = e.data.get("output").cloned().unwrap_or(Value::Null);
        if let Err(m) = self.check(&a.action, a.expect.as_deref(), &output) {
            return self.fail(m);
        }
        let chain = self.rt.chain.as_mut().expect("a chain is running");
        chain.prev = output;
        chain.index += 1;
        self.advance()
    }

    fn timeout(&mut self, e: &Event) -> Result<(), String> {
        let i = e
            .data
            .get("transition")
            .and_then(Value::as_u64)
            .map(|i| i as usize);
        let Some(i) = i else {
            self.stale("a timer without a transition");
            return Ok(());
        };
        let t = self.transition(i)?.clone();
        let Some(to) = t.on_timeout.clone() else {
            self.stale("the transition has no on_timeout");
            return Ok(());
        };
        let fits = match (e.data.get("run").and_then(Value::as_str), &self.rt.chain) {
            (Some(run), Some(c)) => c.run == run && c.transition == i,
            (None, None) => {
                e.data.get("state").and_then(Value::as_str) == Some(self.rt.state.as_str())
            }
            _ => false,
        };
        if !fits {
            self.stale("a timer for a transition no longer waiting");
            return Ok(());
        }
        self.log.push(format!("{} timed out", Self::label(&t)));
        let input = self.rt.chain.take().map(|c| c.input).unwrap_or(Value::Null);
        self.enter(&to, &input)
    }

    fn resume(&mut self) -> Result<(), String> {
        let Some(f) = self.rt.failed.clone().filter(|_| self.rt.state == FAILED) else {
            self.log
                .push("nothing to resume: the ticket has not failed".into());
            return Ok(());
        };
        self.rt.state = f.from.clone();
        self.rt.failed = None;
        self.log.push(format!("resumed from {}", f.from));
        self.take(f.transition, f.run, f.input)
    }

    fn fail(&mut self, why: String) -> Result<(), String> {
        let c = self.rt.chain.take().expect("a chain is running");
        self.fail_at(c.transition, c.run, c.input, why)
    }

    fn fail_at(&mut self, i: usize, run: String, input: Value, why: String) -> Result<(), String> {
        let t = self.transition(i)?.clone();
        self.rt.chain = None;
        if let Some(to) = &t.on_fail {
            self.log
                .push(format!("{} failed ({why}); on_fail {to}", Self::label(&t)));
            return self.enter(to, &input);
        }
        self.log.push(format!("{} failed: {why}", Self::label(&t)));
        self.rt.state = FAILED.to_string();
        self.status = FAILED.to_string();
        self.timers = Some(vec![]);
        self.rt.failed = Some(Failed {
            from: t.from.clone(),
            transition: i,
            run,
            input,
        });
        let note = format!("Lifecycle failed at {}: {why}", Self::label(&t));
        merge(
            &mut self.changes,
            &Map::from_iter([("note".to_string(), Value::String(note))]),
        );
        Ok(())
    }
}
