//! The worker: runs one job and reports its output. It is dumb: the HTTP
//! request, the command, or the program, as the job says; nothing else.
use super::Job;
use serde_json::{Map, Value, json};
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;

/// Runs a job: its output, or why it could not run.
pub fn run(job: &Job) -> Result<Value, String> {
    match job.action.as_str() {
        "http.request@1" => http(job).map_err(|e| format!("http.request@1: {e}")),
        "command@1" => command(job).map_err(|e| format!("command@1: {e}")),
        a if job.program.is_some() => program(job).map_err(|e| format!("{a}: {e}")),
        a => Err(format!("{a} does not run in a worker")),
    }
}

fn text(job: &Job, k: &str) -> Option<String> {
    job.with.get(k).and_then(Value::as_str).map(str::to_string)
}

fn http(job: &Job) -> Result<Value, String> {
    let url = text(job, "url").ok_or("needs with.url")?;
    let method = text(job, "method")
        .unwrap_or_else(|| "GET".into())
        .to_uppercase();
    let method = reqwest::Method::from_bytes(method.as_bytes()).map_err(|e| e.to_string())?;
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(job.timeout))
        .build()
        .map_err(|e| e.to_string())?;
    let mut req = client.request(method, &url);
    let headers = job
        .with
        .get("headers")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut has_key = false;
    for (k, v) in &headers {
        has_key |= k.eq_ignore_ascii_case("idempotency-key");
        let v = v
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| v.to_string());
        req = req.header(k.as_str(), v);
    }
    if !has_key {
        req = req.header("Idempotency-Key", &job.key);
    }
    if let Some(j) = job.with.get("json") {
        req = req.json(j);
    } else if let Some(b) = job.with.get("body") {
        req = req.body(
            b.as_str()
                .map(str::to_string)
                .unwrap_or_else(|| b.to_string()),
        );
    }
    let res = req.send().map_err(|e| e.to_string())?;
    let status = res.status().as_u16();
    let headers: Map<String, Value> = res
        .headers()
        .iter()
        .map(|(k, v)| (k.as_str().to_string(), json!(v.to_str().unwrap_or(""))))
        .collect();
    let body = res.text().map_err(|e| e.to_string())?;
    let mut out = json!({"status": status, "headers": headers, "text": body});
    if let Ok(j) = serde_json::from_str::<Value>(&body) {
        out["json"] = j;
    }
    Ok(out)
}

fn command(job: &Job) -> Result<Value, String> {
    let run: Vec<String> = job
        .with
        .get("run")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let (program, args) = run.split_first().ok_or("needs with.run")?;
    let out = Command::new(program)
        .args(args)
        .current_dir(&job.root)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cannot run {program}: {e}"))?;
    Ok(json!({
        "exit": out.status.code().unwrap_or(-1),
        "stdout": String::from_utf8_lossy(&out.stdout),
        "stderr": String::from_utf8_lossy(&out.stderr),
    }))
}

fn program(job: &Job) -> Result<Value, String> {
    let p = job.program.as_ref().expect("checked");
    let exe = if p.executable.contains('/') {
        p.folder.join(&p.executable)
    } else {
        p.executable.clone().into()
    };
    let mut c = Command::new(&exe)
        .args(&p.arguments)
        .current_dir(&p.folder)
        .envs(&p.environment)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot run {}: {e}", exe.display()))?;
    let given = json!({
        "with": job.with,
        "input": job.input,
        "prev": job.prev,
        "ticket": job.ticket,
        "project": job.project,
        "args": job.args,
        "key": job.key,
    });
    let mut stdin = c.stdin.take().expect("piped");
    // a program that reads nothing closes its end early: not an error
    let _ = writeln!(stdin, "{given}");
    drop(stdin);
    let out = c.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        let why = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let code = out
            .status
            .code()
            .map_or("a signal".into(), |c| format!("exit {c}"));
        return Err(if why.is_empty() {
            code
        } else {
            format!("{code}: {why}")
        });
    }
    serde_json::from_slice(&out.stdout).map_err(|_| {
        format!(
            "printed no JSON: {}",
            String::from_utf8_lossy(&out.stdout).trim()
        )
    })
}
