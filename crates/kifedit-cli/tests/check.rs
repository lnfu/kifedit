use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn kifedit(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_kifedit"))
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}

const LIBRARY: &str = "(instance instance BinaryPredicate)\n(instance BinaryPredicate Entity)\n(instance part BinaryPredicate)\n(domain part 1 Object)\n(domain part 2 Object)\n(subclass Object Entity)\n";

fn project(dir: &Path, lab: &str, checks: &str) {
    fs::write(dir.join("base.kif"), LIBRARY).unwrap();
    fs::write(dir.join("lab.kif"), lab).unwrap();
    fs::write(
        dir.join("kifedit.toml"),
        format!(
            "[project]\nname = \"test\"\n\n[files]\nreadonly = [\"base.kif\"]\n\n[checks]\n{checks}"
        ),
    )
    .unwrap();
}

fn stdout(out: &Output) -> Vec<String> {
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

// 只留錯誤，讓輸出好比對
const ONLY_ERRORS: &str = "W002 = \"off\"\nI001 = \"off\"\n";

#[test]
fn check_prints_errors_and_exits_with_1() {
    let dir = tempfile::tempdir().unwrap();
    project(
        dir.path(),
        "(instance Arm Object)\n(part Arm Arm Arm)\n",
        ONLY_ERRORS,
    );
    let out = kifedit(dir.path(), &["check"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        stdout(&out),
        ["lab.kif:2:1: error[E003]: part must have 2 arguments. It has 3."]
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("Errors: 1. Warnings: 0. Information: 0.")
    );
}

#[test]
fn check_without_errors_exits_with_0() {
    let dir = tempfile::tempdir().unwrap();
    project(
        dir.path(),
        "(instance Arm Object)\n(part Arm Arm)\n",
        ONLY_ERRORS,
    );
    let out = kifedit(dir.path(), &["check"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stdout.is_empty());
}

#[test]
fn library_problems_show_only_with_all() {
    let dir = tempfile::tempdir().unwrap();
    project(dir.path(), "(instance Arm Object)\n", "");
    let without: Vec<String> = stdout(&kifedit(dir.path(), &["check"]));
    let with: Vec<String> = stdout(&kifedit(dir.path(), &["check", "--all"]));
    assert!(without.iter().all(|l| l.starts_with("lab.kif:")));
    assert!(with.iter().any(|l| l.starts_with("base.kif:")));
}

#[test]
fn json_has_one_object_on_each_line() {
    let dir = tempfile::tempdir().unwrap();
    project(
        dir.path(),
        "(instance Arm Object)\n(part Arm Arm Arm)\n",
        ONLY_ERRORS,
    );
    let out = kifedit(dir.path(), &["check", "--format", "json"]);
    let lines = stdout(&out);
    assert_eq!(lines.len(), 1);
    let value: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
    assert_eq!(value["path"], "lab.kif");
    assert_eq!(value["line"], 2);
    assert_eq!(value["column"], 1);
    assert_eq!(value["endLine"], 2);
    assert_eq!(value["endColumn"], 19);
    assert_eq!(value["severity"], "error");
    assert_eq!(value["code"], "E003");
}

#[test]
fn check_without_project_exits_with_2() {
    let dir = tempfile::tempdir().unwrap();
    let out = kifedit(dir.path(), &["check"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("Cannot find kifedit.toml"));
}
