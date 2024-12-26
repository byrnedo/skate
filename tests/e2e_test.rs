use std::{env, panic, process};
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::io::{stderr, stdout};
use std::process::Stdio;
use serde::{Deserialize, Serialize};
use serde_json::Value;

fn setup() {}

fn teardown() {}

fn run_test<T>(test: T) -> ()
where
    T: FnOnce() -> () + panic::UnwindSafe,
{
    setup();
    let result = panic::catch_unwind(|| {
        test()
    });
    teardown();
    assert!(result.is_ok())
}

#[derive(Debug, Clone)]
struct SkateError {
    exit_code: i32,
    message: String,
}

impl Error for SkateError {}

impl Display for SkateError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "exit code: {}, message: {}", self.exit_code, self.message)
    }
}

fn skate(command: &str, args: &[&str]) -> Result<(String, String), SkateError> {
    let output = process::Command::new("./target/debug/skate")
        .args([&[command], args].concat())
        .output().map_err(|e| SkateError { exit_code: -1, message: e.to_string() })?;

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        return Err(SkateError { exit_code: output.status.code().unwrap_or_default(), message: stderr });
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    Ok((stdout, stderr))
}

fn skate_stdout(command: &str, args: &[&str]) -> Result<(), SkateError> {
    let mut child = process::Command::new("./target/debug/skate")
        .args([&[command], args].concat())
        .stdout(stdout())
        .stderr(stderr())
        .spawn().map_err(|e| SkateError { exit_code: -1, message: e.to_string() })?;


    let status = child.wait().map_err(|e| SkateError { exit_code: -1, message: e.to_string() })?;
    if !status.success() {
        return Err(SkateError { exit_code: status.code().unwrap_or_default(), message: "".to_string() });
    }

    Ok(())
}

fn ips() -> Result<(String, String), anyhow::Error> {
    let output = process::Command::new("multipass")
        .args(&["info", "--format", "json"])
        .output()?;

    let info: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let node1 = info["info"]["node-1"]["ipv4"][0].as_str().ok_or(anyhow::anyhow!("failed to get node-1 ip"))?;
    let node2 = info["info"]["node-2"]["ipv4"][0].as_str().ok_or(anyhow::anyhow!("failed to get node-2 ip"))?;

    Ok((node1.to_string(), node2.to_string()))
}

#[test]
fn test_cluster_creation() -> Result<(), anyhow::Error> {
    if env::var("SKATE_E2E").is_err() {
        return Ok(());
    }
    let ips = ips()?;

    let user = env::var("USER")?;

    skate_stdout("delete", &["cluster", "integration-test", "--yes"]);
    skate_stdout("create", &["cluster", "integration-test"])?;
    skate_stdout("config", &["use-context", "integration-test"])?;
    skate_stdout("create", &["node", "--name", "node-1", "--host", &ips.0, "--subnet-cidr", "20.1.0.0/16", "--key", "/tmp/skate-e2e-key", "--user", &user])?;
    skate_stdout("create", &["node", "--name", "node-2", "--host", &ips.1, "--subnet-cidr", "20.2.0.0/16", "--key", "/tmp/skate-e2e-key", "--user", &user])?;
    let (stdout, _stderr) = skate("refresh", &["--json"])?;

    let state: Value = serde_json::from_str(&stdout)?;

    assert_eq!(state["nodes"].as_array().unwrap().len(), 2);
    let node1 = state["nodes"][0].clone();
    let node2 = state["nodes"][1].clone();

    assert_eq!(node1["node_name"], "node-1");
    assert_eq!(node1["status"], "Healthy");
    assert_eq!(node2["node_name"], "node-2");
    assert_eq!(node2["status"], "Healthy");

    // TODO -  validate that things work
    // check internet works
    // check dns works
    //
    Ok(())
}