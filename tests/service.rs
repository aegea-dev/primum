use primum::engine::service::{install_ctx, label, log_file};
use std::path::Path;

#[test]
fn the_service_runs_the_watchdog_in_the_workspace() {
    let root = Path::new("/Users/ada/işler");
    let exe = Path::new("/opt/primum/bin/primum");
    let ctx = install_ctx(
        root,
        exe,
        9000,
        2,
        Some("/opt/homebrew/bin:/usr/bin".into()),
    );
    assert_eq!(ctx.program, exe);
    let args: Vec<String> = ctx
        .args
        .iter()
        .map(|a| a.to_string_lossy().into())
        .collect();
    assert_eq!(
        args,
        [
            "watchdog",
            "--port",
            "9000",
            "--interval",
            "2",
            "--log",
            "/Users/ada/işler/.primum-daemon.log"
        ]
    );
    assert_eq!(ctx.working_directory.as_deref(), Some(root));
    assert_eq!(
        ctx.environment,
        Some(vec![(
            "PATH".to_string(),
            "/opt/homebrew/bin:/usr/bin".to_string()
        )])
    );
    assert!(ctx.autostart);
    assert_eq!(log_file(root), root.join(".primum-daemon.log"));
}

#[test]
fn each_workspace_has_its_own_stable_label() {
    let a = label(Path::new("/Users/ada/işler")).to_qualified_name();
    assert!(a.starts_with("dev.primum.daemon-"), "{a}");
    assert_eq!(a, label(Path::new("/Users/ada/işler")).to_qualified_name());
    assert_ne!(a, label(Path::new("/Users/ada/ödev")).to_qualified_name());
}
