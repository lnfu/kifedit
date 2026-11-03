use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

const UNFORMATTED: &str = "(instance   A    B )\n";
const FORMATTED: &str = "(instance A B)\n";

// CARGO_BIN_EXE_<name> 由 Cargo 在建置整合測試時提供
fn kifedit(args: &[&OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_kifedit"))
        .args(args)
        .output()
        .unwrap()
}

fn write_kif(dir: &Path, text: &str) -> std::path::PathBuf {
    let path = dir.join("a.kif");
    fs::write(&path, text).unwrap();
    path
}

#[test]
fn fmt_formats_the_file_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_kif(dir.path(), UNFORMATTED);

    let out = kifedit(&["fmt".as_ref(), path.as_os_str()]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(fs::read_to_string(&path).unwrap(), FORMATTED);
}

#[test]
fn check_reports_an_unformatted_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_kif(dir.path(), UNFORMATTED);

    let out = kifedit(&["fmt".as_ref(), "--check".as_ref(), path.as_os_str()]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stdout).contains("The file is not formatted."));
    assert_eq!(fs::read_to_string(&path).unwrap(), UNFORMATTED);
}

#[test]
fn check_passes_a_formatted_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_kif(dir.path(), FORMATTED);

    let out = kifedit(&["fmt".as_ref(), "--check".as_ref(), path.as_os_str()]);
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stdout.is_empty());
}

#[test]
fn missing_file_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("missing.kif");

    let out = kifedit(&["fmt".as_ref(), path.as_os_str()]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("Cannot read the file"));
}

#[test]
fn fmt_without_files_is_a_usage_error() {
    let out = kifedit(&["fmt".as_ref()]);
    assert_eq!(out.status.code(), Some(2));
}
