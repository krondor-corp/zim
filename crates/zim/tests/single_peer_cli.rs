//! End-to-end CLI smoke against a single daemon.
//!
//! Spawns `zim daemon run` in a temp data dir with a known port
//! written into `config.toml`, polls until the HTTP port is up, then
//! runs the full CLI surface (`zim vaults create`, `zim id`,
//! `zim vault <target> head`, etc.) as child processes and asserts
//! on stdout.
//!
//! The daemon is killed on test exit via a Drop guard.

use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use tempfile::TempDir;

struct DaemonGuard {
    child: Child,
}

impl Drop for DaemonGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_zim")
}

fn write_config(home: &TempDir, port: u16) {
    let path = home.path().join("config.toml");
    let body = format!("api_port = {port}\nlog_level = \"info\"\n");
    std::fs::write(&path, body).expect("write config.toml");
}

fn spawn_daemon(home: &TempDir, port: u16) -> DaemonGuard {
    let child = Command::new(bin())
        .env("ZIM_HOME", home.path())
        .env("ZIM_LOG", "zim=info,zim_peer=info")
        .arg("daemon")
        .arg("run")
        .arg("--port")
        .arg(port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn daemon");
    DaemonGuard { child }
}

fn wait_for_port(port: u16, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if std::net::TcpStream::connect_timeout(
            &format!("127.0.0.1:{port}").parse().unwrap(),
            Duration::from_millis(100),
        )
        .is_ok()
        {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("daemon never came up on port {port}");
}

fn run(home: &TempDir, args: &[&str]) -> (String, String) {
    let output = Command::new(bin())
        .env("ZIM_HOME", home.path())
        .env("ZIM_LOG", "warn")
        .arg("--plain")
        .args(args)
        .output()
        .expect("spawn cli");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        output.status.success(),
        "command failed: zim {args:?}\nstatus: {:?}\nstdout: {stdout}\nstderr: {stderr}",
        output.status
    );
    (stdout, stderr)
}

fn run_with_stdin(home: &TempDir, args: &[&str], stdin: &[u8]) -> (String, String) {
    let mut child = Command::new(bin())
        .env("ZIM_HOME", home.path())
        .env("ZIM_LOG", "warn")
        .arg("--plain")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn cli");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(stdin)
        .expect("write stdin");
    let output = child.wait_with_output().expect("wait cli");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        output.status.success(),
        "command failed: zim {args:?}\nstatus: {:?}\nstdout: {stdout}\nstderr: {stderr}",
        output.status
    );
    (stdout, stderr)
}

#[test]
fn single_peer_round_trip() {
    let home = tempfile::tempdir().expect("tempdir");
    let port = 17191;
    write_config(&home, port);
    let _daemon = spawn_daemon(&home, port);
    wait_for_port(port, Duration::from_secs(10));

    // Under `--plain`, every op emits JSON; assert against shape
    // rather than pretty text.

    // `zim id` returns a JSON string: "abc...def".
    let (id_out, _) = run(&home, &["id"]);
    let id_str: String = serde_json::from_str(id_out.trim()).expect("id json");
    assert_eq!(id_str.len(), 64, "pubkey hex");

    // Health dashboard with daemon up.
    let (health, _) = run(&home, &["health"]);
    let health_json: serde_json::Value = serde_json::from_str(health.trim()).expect("health json");
    assert_eq!(health_json["daemon_up"], true, "health: {health}");
    assert_eq!(
        health_json["peer_id"].as_str().map(str::len),
        Some(64),
        "health peer_id: {health}"
    );

    // `zim version` returns BuildInfo — has a `version` field.
    let (ver, _) = run(&home, &["version"]);
    let ver_json: serde_json::Value = serde_json::from_str(ver.trim()).expect("version json");
    assert!(
        ver_json.get("version").is_some(),
        "version missing field: {ver}"
    );

    // Empty registry → empty array.
    let (vaults_empty, _) = run(&home, &["vaults", "list"]);
    let vaults0: Vec<serde_json::Value> =
        serde_json::from_str(vaults_empty.trim()).expect("vaults json");
    assert!(vaults0.is_empty(), "vaults: {vaults_empty}");

    // Create a vault — name comes back in the response.
    let (create, _) = run(&home, &["vaults", "create", "demo"]);
    let created: serde_json::Value = serde_json::from_str(create.trim()).expect("create json");
    assert_eq!(created["name"], "demo", "create: {create}");

    // Registry now lists it.
    let (vaults_one, _) = run(&home, &["vaults", "list"]);
    let vaults1: Vec<serde_json::Value> =
        serde_json::from_str(vaults_one.trim()).expect("vaults json");
    assert_eq!(vaults1.len(), 1, "vaults: {vaults_one}");
    assert_eq!(vaults1[0]["name"], "demo", "vaults: {vaults_one}");

    // Freshly-saved genesis — height 0.
    let (head, _) = run(&home, &["vault", "demo", "head"]);
    let head_json: serde_json::Value = serde_json::from_str(head.trim()).expect("head json");
    assert_eq!(head_json["height"], 0, "head: {head}");

    // Empty ls — items array empty.
    let (ls, _) = run(&home, &["vault", "demo", "ls", "/"]);
    let ls_json: serde_json::Value = serde_json::from_str(ls.trim()).expect("ls json");
    assert!(
        ls_json["items"].as_array().unwrap().is_empty(),
        "ls /: {ls}"
    );

    // mkdir.
    let (mkdir, _) = run(&home, &["vault", "demo", "mkdir", "/docs"]);
    let mkdir_json: serde_json::Value = serde_json::from_str(mkdir.trim()).expect("mkdir json");
    assert_eq!(mkdir_json["path"], "/docs", "mkdir: {mkdir}");

    // add from stdin.
    let (add, _) = run_with_stdin(
        &home,
        &["vault", "demo", "add", "/docs/readme.md"],
        b"hello zim\n",
    );
    let add_json: serde_json::Value = serde_json::from_str(add.trim()).expect("add json");
    assert_eq!(add_json["path"], "/docs/readme.md", "add: {add}");

    // ls shows the file.
    let (ls_docs, _) = run(&home, &["vault", "demo", "ls", "/docs"]);
    let ls_docs_json: serde_json::Value = serde_json::from_str(ls_docs.trim()).expect("ls json");
    let items = ls_docs_json["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "ls /docs: {ls_docs}");
    assert_eq!(items[0]["name"], "readme.md", "ls /docs: {ls_docs}");
    assert_eq!(items[0]["kind"], "file", "ls /docs: {ls_docs}");

    // cat — `Bytes` serializes as a JSON array of byte values.
    let (cat, _) = run(&home, &["vault", "demo", "cat", "/docs/readme.md"]);
    let cat_bytes: Vec<u8> = serde_json::from_str(cat.trim()).expect("cat json");
    assert_eq!(cat_bytes, b"hello zim\n", "cat: {cat}");

    // Head height advanced.
    let (head_after, _) = run(&home, &["vault", "demo", "head"]);
    let head_after_json: serde_json::Value =
        serde_json::from_str(head_after.trim()).expect("head json");
    assert!(
        head_after_json["height"].as_u64().unwrap() > 0,
        "head after mutations should advance: {head_after}"
    );

    // rm.
    let (rm, _) = run(&home, &["vault", "demo", "rm", "/docs/readme.md"]);
    let rm_json: serde_json::Value = serde_json::from_str(rm.trim()).expect("rm json");
    assert_eq!(rm_json["path"], "/docs/readme.md", "rm: {rm}");
    let (ls_after_rm, _) = run(&home, &["vault", "demo", "ls", "/docs"]);
    let ls_after_rm_json: serde_json::Value =
        serde_json::from_str(ls_after_rm.trim()).expect("ls json");
    assert!(
        ls_after_rm_json["items"].as_array().unwrap().is_empty(),
        "ls after rm: {ls_after_rm}"
    );
}
