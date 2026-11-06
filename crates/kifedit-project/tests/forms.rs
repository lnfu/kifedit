use std::fs;
use std::path::Path;

use kifedit_project::{FormEdit, FormError, FormValue, Project};
use kifedit_sema::{FileId, FormulaId};

fn project_with(dir: &Path, library: &str, editable: &str) -> Project {
    fs::write(dir.join("base.kif"), library).unwrap();
    fs::write(dir.join("lab.kif"), editable).unwrap();
    let config = dir.join("kifedit.toml");
    fs::write(
        &config,
        "[project]\nname = \"test\"\n\n[files]\nreadonly = [\"base.kif\"]\n",
    )
    .unwrap();
    Project::open(&config).unwrap()
}

fn apply(project: &mut Project, edit: FormEdit) -> String {
    project.apply_form(&edit).unwrap();
    project.text(FileId(1)).unwrap().to_string()
}

fn add(relation: &str, args: &[&str]) -> FormEdit {
    FormEdit::AddFact {
        file: FileId(1),
        relation: relation.into(),
        args: args.iter().map(|a| FormValue::Symbol((*a).into())).collect(),
    }
}

#[test]
fn library_files_cannot_be_changed_by_forms() {
    let dir = tempfile::tempdir().unwrap();
    let project = project_with(dir.path(), "(instance A B)\n", "");
    let edit = FormEdit::DeleteFact {
        formula: FormulaId {
            file: FileId(0),
            index: 0,
        },
    };
    assert!(matches!(
        project.preview_form(&edit),
        Err(FormError::LibraryFile { .. })
    ));
}

#[test]
fn new_term_is_added_at_the_end_after_a_blank_line() {
    let dir = tempfile::tempdir().unwrap();
    let mut project = project_with(dir.path(), "", "(instance A B)\n");
    assert_eq!(
        apply(&mut project, add("instance", &["C", "D"])),
        "(instance A B)\n\n(instance C D)\n"
    );
}

#[test]
fn new_fact_goes_after_a_trailing_comment() {
    let dir = tempfile::tempdir().unwrap();
    let mut project = project_with(dir.path(), "", "(instance A B) ; note\n\n(instance C D)\n");
    assert_eq!(
        apply(&mut project, add("part", &["A", "C"])),
        "(instance A B) ; note\n(part A C)\n\n(instance C D)\n"
    );
}

#[test]
fn term_format_uses_the_second_argument_as_the_term() {
    let dir = tempfile::tempdir().unwrap();
    let mut project = project_with(dir.path(), "", "(instance A B)\n\n(instance C D)\n");
    let edit = FormEdit::AddFact {
        file: FileId(1),
        relation: "termFormat".into(),
        args: vec![
            FormValue::Symbol("EnglishLanguage".into()),
            FormValue::Symbol("A".into()),
            FormValue::Str("a".into()),
        ],
    };
    assert_eq!(
        apply(&mut project, edit),
        "(instance A B)\n(termFormat EnglishLanguage A \"a\")\n\n(instance C D)\n"
    );
}

#[test]
fn crlf_files_get_crlf_lines() {
    let dir = tempfile::tempdir().unwrap();
    let mut project = project_with(dir.path(), "", "(instance A B)\r\n");
    assert_eq!(
        apply(&mut project, add("part", &["A", "C"])),
        "(instance A B)\r\n(part A C)\r\n"
    );
}

#[test]
fn delete_removes_the_trailing_comment_and_the_line() {
    let dir = tempfile::tempdir().unwrap();
    let mut project = project_with(dir.path(), "", "(a b)\n(c d) ; note\n(e f)\n");
    let edit = FormEdit::DeleteFact {
        formula: FormulaId {
            file: FileId(1),
            index: 1,
        },
    };
    assert_eq!(apply(&mut project, edit), "(a b)\n(e f)\n");
}

#[test]
fn delete_keeps_other_forms_on_the_same_line() {
    let dir = tempfile::tempdir().unwrap();
    let mut project = project_with(dir.path(), "", "(a b) (c d)\n");
    let edit = FormEdit::DeleteFact {
        formula: FormulaId {
            file: FileId(1),
            index: 1,
        },
    };
    assert_eq!(apply(&mut project, edit), "(a b) \n");
}
