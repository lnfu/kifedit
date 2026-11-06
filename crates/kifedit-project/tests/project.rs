use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use kifedit_project::{EditFileError, FileRole, Project, ProjectError};
use kifedit_sema::FileId;
use kifedit_syntax::{TextEdit, TextRange};

const LIBRARY: &str = "(subclass Camera Device)\n(subclass Device Entity)\n";
const EDITABLE: &str = "(instance HeadCamera Camera)\n";

fn write_project(dir: &Path, files: &str) -> PathBuf {
    fs::write(dir.join("base.kif"), LIBRARY).unwrap();
    fs::write(dir.join("lab.kif"), EDITABLE).unwrap();
    let config = dir.join("kifedit.toml");
    fs::write(
        &config,
        format!("[project]\nname = \"test\"\n\n[files]\n{files}"),
    )
    .unwrap();
    config
}

fn small_project(dir: &Path) -> Project {
    let config = write_project(dir, "readonly = [\"base.kif\"]\n");
    Project::open(&config).unwrap()
}

#[test]
fn opens_library_and_editable_files() {
    let dir = tempfile::tempdir().unwrap();
    let project = small_project(dir.path());

    let roles: Vec<_> = project.files().map(|f| f.role).collect();
    assert_eq!(roles, [FileRole::Library, FileRole::Editable]);
    assert_eq!(project.index().formula_count(), 3);

    let name = |s| project.interner().get(s).unwrap();
    let types = project.taxonomy().types_of(name("HeadCamera"));
    assert_eq!(types, [name("Camera"), name("Device"), name("Entity")]);
}

#[test]
fn edit_updates_index_and_taxonomy() {
    let dir = tempfile::tempdir().unwrap();
    let mut project = small_project(dir.path());
    let lab = FileId(1);

    // "(instance HeadCamera Camera)" 的 Camera 位於 21..27
    let edit = TextEdit::replace(TextRange::new(21.into(), 27.into()), "Device");
    let inverse = project.edit(lab, &[edit]).unwrap();
    let name = |p: &Project, s| p.interner().get(s).unwrap();
    assert_eq!(
        project.taxonomy().instance_of(name(&project, "HeadCamera")),
        [name(&project, "Device")]
    );

    project.edit(lab, &inverse).unwrap();
    assert_eq!(project.text(lab).unwrap(), EDITABLE);
    assert_eq!(
        project.taxonomy().instance_of(name(&project, "HeadCamera")),
        [name(&project, "Camera")]
    );
}

#[test]
fn library_files_cannot_be_edited() {
    let dir = tempfile::tempdir().unwrap();
    let mut project = small_project(dir.path());
    let edit = TextEdit::insert(0.into(), "x");
    assert!(matches!(
        project.edit(FileId(0), &[edit]),
        Err(EditFileError::LibraryFile { .. })
    ));
}

#[test]
fn missing_file_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let config = write_project(dir.path(), "readonly = [\"missing.kif\"]\n");
    let error = Project::open(&config).unwrap_err();
    assert!(matches!(error, ProjectError::Read { .. }));
    assert!(
        error.to_string().starts_with("Cannot read the file"),
        "{error}"
    );
}

fn paths(project: &Project, root: &Path) -> Vec<(String, FileRole)> {
    let mut out: Vec<_> = project
        .files()
        .map(|f| {
            (
                f.path
                    .strip_prefix(root)
                    .unwrap_or(&f.path)
                    .display()
                    .to_string(),
                f.role,
            )
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[test]
fn all_kif_files_are_in_the_project_and_readonly_patterns_win() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("kb/sub")).unwrap();
    for name in ["kb/b.kif", "kb/a.kif", "kb/sub/c.kif", "kb/notes.txt"] {
        fs::write(dir.path().join(name), "").unwrap();
    }
    let config = write_project(dir.path(), "readonly = [\"kb/**/*.kif\", \"base.kif\"]\n");
    let project = Project::open(&config).unwrap();
    assert_eq!(
        paths(&project, dir.path()),
        [
            ("base.kif".to_string(), FileRole::Library),
            ("kb/a.kif".to_string(), FileRole::Library),
            ("kb/b.kif".to_string(), FileRole::Library),
            ("kb/sub/c.kif".to_string(), FileRole::Library),
            ("lab.kif".to_string(), FileRole::Editable),
        ]
    );
}

#[test]
fn kifignore_removes_files_but_readonly_entries_stay() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("drafts")).unwrap();
    fs::write(dir.path().join("drafts/x.kif"), "").unwrap();
    fs::write(dir.path().join(".kifignore"), "drafts/\nbase.kif\n").unwrap();
    let config = write_project(dir.path(), "readonly = [\"base.kif\"]\n");
    let project = Project::open(&config).unwrap();
    assert_eq!(
        paths(&project, dir.path()),
        [
            ("base.kif".to_string(), FileRole::Library),
            ("lab.kif".to_string(), FileRole::Editable),
        ]
    );
}

#[test]
fn readonly_directory_can_be_outside_the_project() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("Merge.kif"), LIBRARY).unwrap();
    let config = write_project(dir.path(), &format!("readonly = [{:?}]\n", outside.path()));
    let project = Project::open(&config).unwrap();
    let merge = project
        .files()
        .find(|f| f.path.ends_with("Merge.kif"))
        .unwrap();
    assert_eq!(merge.role, FileRole::Library);
    assert_eq!(project.files().count(), 3);
}

#[test]
fn pattern_without_matches_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let config = write_project(dir.path(), "readonly = [\"sumo/*.kif\"]\n");
    let error = Project::open(&config).unwrap_err();
    assert!(
        matches!(&error, ProjectError::NoMatch { pattern } if pattern == "sumo/*.kif"),
        "{error}"
    );
}

#[test]
fn opens_the_example_project_with_sumo() {
    let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
    let lab = data.join("fixtures/mm.kif");
    if !lab.is_file() {
        eprintln!("Skip the test. Cannot find {}.", lab.display());
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("kifedit.toml");
    let library = ["Merge.kif", "Mid-level-ontology.kif"].map(|f| data.join("sumo").join(f));
    fs::write(
        &config,
        format!(
            "[project]\nname = \"mm\"\n\n[files]\nreadonly = [{:?}, {:?}]\n",
            library[0], library[1]
        ),
    )
    .unwrap();
    fs::copy(&lab, dir.path().join("mm.kif")).unwrap();

    let start = Instant::now();
    let project = Project::open(&config).unwrap();
    let elapsed = start.elapsed();

    let name = |s| project.interner().get(s).unwrap();
    let kinds = project.taxonomy().kinds(name("MyStretch4"));
    assert!(kinds.individual, "MyStretch4 is not an individual.");
    assert!(
        project
            .taxonomy()
            .types_of(name("MyStretch4"))
            .contains(&name("Robot")),
        "MyStretch4 is not a Robot."
    );
    eprintln!(
        "Opened {} files with {} formulas in {elapsed:?}.",
        project.files().count(),
        project.index().formula_count()
    );
}

#[test]
fn checks_section_changes_or_removes_rules() {
    let dir = tempfile::tempdir().unwrap();
    let files = "readonly = [\"base.kif\"]\n";
    let codes = |checks: &str| -> Vec<String> {
        let config = write_project(dir.path(), &format!("{files}\n[checks]\n{checks}"));
        let project = Project::open(&config).unwrap();
        project
            .diagnostics(FileId(1))
            .iter()
            .map(|d| format!("{} {:?}", d.code, d.severity))
            .collect()
    };
    // lab.kif：instance 未宣告（E002）、HeadCamera 沒有說明（W002）、沒有被引用（I001）
    assert_eq!(codes(""), ["I001 Info", "W002 Warning", "E002 Error"]);
    assert_eq!(
        codes("W002 = \"off\"\nE002 = \"warning\""),
        ["I001 Info", "E002 Warning"]
    );
}

#[test]
fn unknown_check_code_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let config = write_project(dir.path(), "\n[checks]\nW999 = \"off\"\n");
    assert!(matches!(
        Project::open(&config),
        Err(ProjectError::UnknownCheck { code, .. }) if code == "W999"
    ));
}

#[test]
fn refresh_files_adds_and_removes_files_and_keeps_ids() {
    let dir = tempfile::tempdir().unwrap();
    let mut project = small_project(dir.path());
    let lab = project.file_id(&dir.path().join("lab.kif")).unwrap();

    fs::write(dir.path().join("new.kif"), "(instance Arm Camera)\n").unwrap();
    let changes = project.refresh_files().unwrap();
    assert!(changes.removed.is_empty());
    let [new] = changes.added[..] else {
        panic!("{changes:?}")
    };
    assert_eq!(project.file(new).unwrap().role, FileRole::Editable);
    let name = |p: &Project, s| p.interner().get(s).unwrap();
    assert_eq!(
        project.taxonomy().instance_of(name(&project, "Arm")),
        [name(&project, "Camera")]
    );

    // 刪除後其他檔案的 ID 不變；同一個路徑再出現時拿回原本的 ID
    fs::remove_file(dir.path().join("new.kif")).unwrap();
    assert_eq!(project.refresh_files().unwrap().removed, [new]);
    assert!(project.file(new).is_none());
    assert_eq!(project.file_id(&dir.path().join("lab.kif")), Some(lab));
    assert!(
        project
            .taxonomy()
            .instance_of(name(&project, "Arm"))
            .is_empty()
    );

    fs::write(dir.path().join("new.kif"), "").unwrap();
    assert_eq!(project.refresh_files().unwrap().added, [new]);
}

#[test]
fn refresh_files_keeps_unsaved_text_and_follows_kifignore() {
    let dir = tempfile::tempdir().unwrap();
    let mut project = small_project(dir.path());
    let lab = project.file_id(&dir.path().join("lab.kif")).unwrap();
    project
        .edit(lab, &[TextEdit::insert(0.into(), "; unsaved\n")])
        .unwrap();

    fs::write(dir.path().join(".kifignore"), "lab.kif\n").unwrap();
    assert_eq!(project.refresh_files().unwrap().removed, [lab]);

    fs::remove_file(dir.path().join(".kifignore")).unwrap();
    assert_eq!(project.refresh_files().unwrap().added, [lab]);
    assert_eq!(project.text(lab), Some(EDITABLE));

    project
        .edit(lab, &[TextEdit::insert(0.into(), "; unsaved\n")])
        .unwrap();
    assert_eq!(project.refresh_files().unwrap(), Default::default());
    assert_eq!(project.text(lab).unwrap(), format!("; unsaved\n{EDITABLE}"));
}
