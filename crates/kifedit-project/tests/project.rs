use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use kifedit_project::{EditFileError, FileRole, Project, ProjectError};
use kifedit_sema::FileId;
use kifedit_syntax::{TextEdit, TextRange};

const LIBRARY: &str = "(subclass Camera Device)\n(subclass Device Entity)\n";
const EDITABLE: &str = "(instance HeadCamera Camera)\n";

fn write_project(dir: &Path, files: &str) -> PathBuf {
    fs::write(dir.join("lib.kif"), LIBRARY).unwrap();
    fs::write(dir.join("lab.kif"), EDITABLE).unwrap();
    let config = dir.join("project.toml");
    fs::write(&config, format!("[project]\nname = \"test\"\n\n[files]\n{files}")).unwrap();
    config
}

fn small_project(dir: &Path) -> Project {
    let config = write_project(dir, "library = [\"lib.kif\"]\neditable = [\"lab.kif\"]\n");
    Project::open(&config).unwrap()
}

#[test]
fn opens_library_and_editable_files() {
    let dir = tempfile::tempdir().unwrap();
    let project = small_project(dir.path());

    let roles: Vec<_> = project.files().iter().map(|f| f.role).collect();
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
fn save_writes_the_document() {
    let dir = tempfile::tempdir().unwrap();
    let mut project = small_project(dir.path());
    project
        .edit(FileId(1), &[TextEdit::insert(0.into(), "; new\n")])
        .unwrap();
    project.save(FileId(1)).unwrap();
    let saved = fs::read_to_string(dir.path().join("lab.kif")).unwrap();
    assert_eq!(saved, format!("; new\n{EDITABLE}"));
}

#[test]
fn missing_file_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let config = write_project(dir.path(), "editable = [\"missing.kif\"]\n");
    let error = Project::open(&config).unwrap_err();
    assert!(matches!(error, ProjectError::Read { .. }));
    assert!(error.to_string().starts_with("Cannot read the file"), "{error}");

}

#[test]
fn patterns_expand_in_path_order_and_editable_wins() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("kb/sub")).unwrap();
    for name in ["kb/b.kif", "kb/a.kif", "kb/sub/c.kif", "kb/notes.txt"] {
        fs::write(dir.path().join(name), "").unwrap();
    }
    let config = write_project(
        dir.path(),
        "library = [\"kb/**/*.kif\", \"kb/a.kif\"]\neditable = [\"kb/b.kif\", \"lab.kif\"]\n",
    );
    let project = Project::open(&config).unwrap();
    let files: Vec<(String, FileRole)> = project
        .files()
        .iter()
        .map(|f| (f.path.strip_prefix(dir.path()).unwrap().display().to_string(), f.role))
        .collect();
    assert_eq!(
        files,
        [
            ("kb/a.kif".to_string(), FileRole::Library),
            ("kb/sub/c.kif".to_string(), FileRole::Library),
            ("kb/b.kif".to_string(), FileRole::Editable),
            ("lab.kif".to_string(), FileRole::Editable),
        ]
    );
}

#[test]
fn pattern_without_matches_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let config = write_project(dir.path(), "library = [\"sumo/*.kif\"]\neditable = [\"lab.kif\"]\n");
    let error = Project::open(&config).unwrap_err();
    assert!(matches!(&error, ProjectError::NoMatch { pattern } if pattern == "sumo/*.kif"), "{error}");
}

#[test]
fn create_file_does_not_change_the_config_when_a_pattern_includes_it() {
    let dir = tempfile::tempdir().unwrap();
    let config = write_project(dir.path(), "library = [\"lib.kif\"]\neditable = [\"*.kif\"]\n");
    let before = fs::read_to_string(&config).unwrap();
    let mut project = Project::open(&config).unwrap();
    project.create_file("new.kif").unwrap();
    assert_eq!(fs::read_to_string(&config).unwrap(), before);

    // 子目錄不在 *.kif 之內，要寫入
    project.create_file("kb/other.kif").unwrap();
    assert!(fs::read_to_string(&config).unwrap().contains("\"*.kif\", \"kb/other.kif\""));
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
    let config = dir.path().join("project.toml");
    let library = ["Merge.kif", "Mid-level-ontology.kif"].map(|f| data.join("sumo").join(f));
    fs::write(
        &config,
        format!(
            "[project]\nname = \"mm\"\n\n[files]\nlibrary = [{:?}, {:?}]\neditable = [{:?}]\n",
            library[0], library[1], lab
        ),
    )
    .unwrap();

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
        project.files().len(),
        project.index().formula_count()
    );
}

#[test]
fn checks_section_changes_or_removes_rules() {
    let dir = tempfile::tempdir().unwrap();
    let files = "library = [\"lib.kif\"]\neditable = [\"lab.kif\"]\n";
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
    assert_eq!(codes("W002 = \"off\"\nE002 = \"warning\""), ["I001 Info", "E002 Warning"]);
}

#[test]
fn unknown_check_code_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let config = write_project(dir.path(), "editable = [\"lab.kif\"]\n\n[checks]\nW999 = \"off\"\n");
    assert!(matches!(
        Project::open(&config),
        Err(ProjectError::UnknownCheck { code, .. }) if code == "W999"
    ));
}

#[test]
fn create_file_adds_an_editable_file_and_keeps_the_config_layout() {
    let dir = tempfile::tempdir().unwrap();
    let config = write_project(
        dir.path(),
        "# library files\nlibrary = [\"lib.kif\"]\neditable = [\"lab.kif\"] # my files\n",
    );
    let mut project = Project::open(&config).unwrap();
    let id = project.create_file("robots/arm.kif").unwrap();

    assert_eq!(project.text(id), Some(""));
    assert_eq!(project.file(id).unwrap().role, FileRole::Editable);
    assert!(dir.path().join("robots/arm.kif").is_file());
    let text = fs::read_to_string(&config).unwrap();
    assert!(text.contains("# library files\n"));
    assert!(text.contains("editable = [\"lab.kif\", \"robots/arm.kif\"] # my files"), "{text}");

    // 新檔案可以編輯，重新開啟後仍在專案中
    project.edit(id, &[TextEdit::insert(0.into(), "(instance Arm Camera)\n")]).unwrap();
    project.save(id).unwrap();
    let reopened = Project::open(&config).unwrap();
    assert_eq!(reopened.files().len(), 3);
    assert_eq!(reopened.text(FileId(2)), Some("(instance Arm Camera)\n"));
}

#[test]
fn create_file_rejects_bad_names() {
    use kifedit_project::CreateFileError;

    let dir = tempfile::tempdir().unwrap();
    let mut project = small_project(dir.path());
    assert!(matches!(project.create_file(" "), Err(CreateFileError::EmptyName)));
    assert!(matches!(project.create_file("notes.txt"), Err(CreateFileError::NotKif)));
    assert!(matches!(project.create_file("../out.kif"), Err(CreateFileError::OutsideProject { .. })));
    assert!(matches!(project.create_file("/tmp/out.kif"), Err(CreateFileError::OutsideProject { .. })));
    assert!(matches!(project.create_file("lab.kif"), Err(CreateFileError::Exists { .. })));
    assert_eq!(project.files().len(), 2);
}
