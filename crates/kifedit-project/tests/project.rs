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
    assert_eq!(project.file(lab).unwrap().document.text(), EDITABLE);
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
fn missing_file_and_duplicate_file_are_errors() {
    let dir = tempfile::tempdir().unwrap();
    let config = write_project(dir.path(), "editable = [\"missing.kif\"]\n");
    let error = Project::open(&config).unwrap_err();
    assert!(matches!(error, ProjectError::Read { .. }));
    assert!(error.to_string().starts_with("Cannot read the file"), "{error}");

    let config = write_project(dir.path(), "library = [\"lab.kif\"]\neditable = [\"lab.kif\"]\n");
    assert!(matches!(
        Project::open(&config),
        Err(ProjectError::DuplicateFile { .. })
    ));
}

#[test]
fn opens_the_example_project_with_sumo() {
    let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
    let lab = data.join("fixtures/stretch4_lab.kif");
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
            "[project]\nname = \"stretch4-lab\"\n\n[files]\nlibrary = [{:?}, {:?}]\neditable = [{:?}]\n",
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
