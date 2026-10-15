use primum::engine::{Job, Program, worker};
use serde_json::{Map, Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;

fn job(action: &str, with: Value, root: PathBuf) -> Job {
    Job {
        action: action.into(),
        with: with.as_object().cloned().unwrap_or_default(),
        key: "12:e1.2:0".into(),
        input: Value::Null,
        prev: Value::Null,
        ticket: json!({"id": "12", "title": "Gölge"})
            .as_object()
            .unwrap()
            .clone(),
        project: Map::new(),
        args: Map::new(),
        root,
        program: None,
        timeout: 10,
    }
}

/// A one-request HTTP server: answers with JSON naming what it was sent.
fn server() -> (String, std::thread::JoinHandle<()>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    let h = std::thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        let mut r = BufReader::new(s.try_clone().unwrap());
        let mut first = String::new();
        r.read_line(&mut first).unwrap();
        let mut headers = Map::new();
        let mut length = 0;
        loop {
            let mut line = String::new();
            r.read_line(&mut line).unwrap();
            let line = line.trim_end();
            if line.is_empty() {
                break;
            }
            let (k, v) = line.split_once(": ").unwrap();
            if k.eq_ignore_ascii_case("content-length") {
                length = v.parse().unwrap();
            }
            headers.insert(k.to_lowercase(), json!(v));
        }
        let mut body = vec![0; length];
        r.read_exact(&mut body).unwrap();
        let answer = json!({
            "data": {"name": "Ürün"},
            "line": first.trim_end(),
            "headers": headers,
            "body": String::from_utf8_lossy(&body),
        })
        .to_string();
        write!(s, "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{answer}", answer.len()).unwrap();
    });
    (url, h)
}

#[test]
fn http_request_sends_and_reads_json() {
    let (url, h) = server();
    let with = json!({"url": format!("{url}/items/12"), "method": "POST", "headers": {"Authorization": "Bearer gizli"}, "json": {"söz": "ışık"}});
    let out = worker::run(&job("http.request@1", with, std::env::temp_dir())).unwrap();
    h.join().unwrap();
    assert_eq!(out["status"], 201);
    assert_eq!(out["json"]["data"]["name"], "Ürün");
    assert_eq!(out["json"]["line"], "POST /items/12 HTTP/1.1");
    assert_eq!(out["json"]["headers"]["authorization"], "Bearer gizli");
    assert_eq!(out["json"]["headers"]["idempotency-key"], "12:e1.2:0");
    assert_eq!(
        serde_json::from_str::<Value>(out["json"]["body"].as_str().unwrap()).unwrap(),
        json!({"söz": "ışık"})
    );
    assert_eq!(out["headers"]["content-type"], "application/json");
}

#[test]
fn http_request_reads_no_more_than_its_cap() {
    let (url, h) = server();
    let with = json!({"url": format!("{url}/büyük"), "max_bytes": 10});
    let e = worker::run(&job("http.request@1", with, std::env::temp_dir())).unwrap_err();
    h.join().unwrap();
    assert!(
        e.contains("larger than 10 bytes") && e.contains("max_bytes"),
        "{e}"
    );
    // a cap the answer fits in reads it whole
    let (url, h) = server();
    let with = json!({"url": url, "max_bytes": 100_000});
    let out = worker::run(&job("http.request@1", with, std::env::temp_dir())).unwrap();
    h.join().unwrap();
    assert_eq!(out["json"]["data"]["name"], "Ürün");
}

#[test]
fn http_request_that_cannot_connect_fails() {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/", l.local_addr().unwrap());
    drop(l);
    let e = worker::run(&job(
        "http.request@1",
        json!({"url": url}),
        std::env::temp_dir(),
    ))
    .unwrap_err();
    assert!(e.contains("http.request@1"), "{e}");
}

#[test]
fn command_runs_in_the_project_and_reports_its_exit() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("notlar.txt"), "çay\n").unwrap();
    let with = json!({"run": ["sh", "-c", "cat notlar.txt; echo hata 1>&2; exit 3"]});
    let out = worker::run(&job("command@1", with, d.path().to_path_buf())).unwrap();
    assert_eq!(
        out,
        json!({"exit": 3, "stdout": "çay\n", "stderr": "hata\n"})
    );
    let e = worker::run(&job(
        "command@1",
        json!({"run": ["yok-böyle-bir-program"]}),
        d.path().to_path_buf(),
    ))
    .unwrap_err();
    assert!(e.contains("yok-böyle-bir-program"), "{e}");
}

#[cfg(unix)]
#[test]
fn a_program_gets_the_job_on_stdin_and_prints_its_output() {
    use std::os::unix::fs::PermissionsExt;
    let d = tempfile::tempdir().unwrap();
    let script = d.path().join("araç.sh");
    std::fs::write(
        &script,
        "#!/bin/sh\nread line\nprintf '{\"got\": %s, \"bölge\": \"%s\"}' \"$line\" \"$BOLGE\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut j = job("araç.run@1", json!({"n": 3}), d.path().to_path_buf());
    j.program = Some(Program {
        folder: d.path().to_path_buf(),
        executable: "./araç.sh".into(),
        arguments: vec![],
        environment: [("BOLGE".to_string(), "eu".to_string())].into(),
        interpreter: None,
    });
    let out = worker::run(&j).unwrap();
    assert_eq!(out["bölge"], "eu");
    assert_eq!(out["got"]["with"], json!({"n": 3}));
    assert_eq!(out["got"]["ticket"]["title"], "Gölge");
    assert_eq!(out["got"]["key"], "12:e1.2:0");
    std::fs::write(&script, "#!/bin/sh\necho kırık 1>&2\nexit 2\n").unwrap();
    assert!(worker::run(&j).unwrap_err().contains("kırık"));
    std::fs::write(&script, "#!/bin/sh\necho 'not json'\n").unwrap();
    assert!(worker::run(&j).unwrap_err().contains("JSON"));
}
