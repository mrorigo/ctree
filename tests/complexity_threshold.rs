// Rust guideline compliant 2026-10-06
use std::fs;
use std::process::Command;

#[test]
fn complexity_threshold_lists_violations_and_returns_failure_status() {
    let path = std::env::temp_dir().join(format!(
        "ctree-complexity-threshold-{}.js",
        std::process::id()
    ));
    fs::write(
        &path,
        "function nested(x, y) { if (x) { if (y) return; } }\n",
    )
    .expect("write test fixture");

    let output = Command::new(env!("CARGO_BIN_EXE_ctree"))
        .arg("--complexity-threshold")
        .arg("2")
        .arg(&path)
        .output()
        .expect("run ctree");
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("FUNCTIONS (complexity > 2)"));
    assert!(stdout.contains("nested\tcomplexity 4"));

    let output = Command::new(env!("CARGO_BIN_EXE_ctree"))
        .arg("--format")
        .arg("json")
        .arg("--complexity-threshold")
        .arg("2")
        .arg(&path)
        .output()
        .expect("run ctree in JSON mode");
    assert_eq!(output.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
    assert_eq!(json["complexity_threshold"], 2);
    assert_eq!(json["complexity_violations"][0]["name"], "nested");
    assert_eq!(json["complexity_violations"][0]["complexity"], 4);

    let output = Command::new(env!("CARGO_BIN_EXE_ctree"))
        .arg("--format")
        .arg("csv")
        .arg("--complexity-threshold")
        .arg("2")
        .arg(&path)
        .output()
        .expect("run ctree in CSV mode");
    assert_eq!(output.status.code(), Some(1));
    let csv = String::from_utf8_lossy(&output.stdout);
    assert!(csv.contains("function,JavaScript"));
    assert!(csv.contains("\"nested\",1"));

    let output = Command::new(env!("CARGO_BIN_EXE_ctree"))
        .arg("--format")
        .arg("md")
        .arg("--complexity-threshold")
        .arg("2")
        .arg(&path)
        .output()
        .expect("run ctree in Markdown mode");
    assert_eq!(output.status.code(), Some(1));
    let markdown = String::from_utf8_lossy(&output.stdout);
    assert!(markdown.contains("# ctree report"));
    assert!(markdown.contains("## Functions over complexity 2"));
    assert!(markdown.contains("| nested | 4 |"));
    let _ = fs::remove_file(&path);
}
