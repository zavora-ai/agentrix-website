//! The shipped binary's self-check passes end to end against a scratch data dir, and a
//! second run counts the probe again instead of failing: the stores are durable.

use std::path::PathBuf;

fn report(data: &std::path::Path) -> String {
    let exe = std::env::var("CARGO_BIN_EXE_agentrix-site-host").expect("the binary under test");
    let out = std::process::Command::new(exe)
        .arg(data)
        .output()
        .expect("the binary runs");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn self_check_is_green_twice() {
    let dir = tempfile::tempdir().expect("a scratch dir");
    let data: PathBuf = dir.path().join("data");
    let first = report(&data);
    assert!(first.contains("site.probe"), "{first}");
    assert!(first.contains("probe filed"), "{first}");
    let second = report(&data);
    assert!(second.contains("site.digest"), "{second}");
    assert!(second.contains("kernel:kpi"), "{second}");
    assert!(second.contains("probe counted"), "{second}");
    assert!(second.contains("durable:"), "{second}");
    assert!(data.join("kernel.db").is_file(), "the database persists");
}
