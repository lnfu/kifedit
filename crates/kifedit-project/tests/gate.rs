// M1 關卡：能用表單完成 stretch4_lab.kif 的日常修改，且不動到其他排版（M1.md 的 G1–G4）

use std::fs;
use std::path::Path;

use kifedit_project::{FormEdit, FormError, FormValue, Project};
use kifedit_sema::{FileId, FormulaId, Term};

const LAB: FileId = FileId(0);

fn open() -> Option<(tempfile::TempDir, Project)> {
    let lab = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/fixtures/stretch4_lab.kif");
    if !lab.is_file() {
        eprintln!("Skip the test. Cannot find {}.", lab.display());
        return None;
    }
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("project.toml");
    fs::write(
        &config,
        format!("[project]\nname = \"gate\"\n\n[files]\neditable = [{lab:?}]\n"),
    )
    .unwrap();
    let project = Project::open(&config).unwrap();
    Some((dir, project))
}

fn find(project: &Project, relation: &str, first: &str, accept: impl Fn(&[Term]) -> bool) -> FormulaId {
    let symbol = |s| project.interner().get(s).unwrap();
    project
        .index()
        .facts(symbol(relation), symbol(first))
        .find(|f| matches!(&f.term, Term::App(_, args) if accept(args)))
        .unwrap_or_else(|| panic!("Cannot find the fact ({relation} {first} ...)."))
        .id
}

fn is(project: &Project, term: &Term, name: &str) -> bool {
    matches!(term, Term::Symbol(s) if project.interner().resolve(*s) == name)
}

fn text(project: &Project) -> String {
    project.file(LAB).unwrap().document.text().to_string()
}

// 以共同前綴與後綴比較，回傳（第一個不同的行號，從 1 起算；被刪除的行；被加入的行）
fn line_diff<'a>(before: &'a str, after: &'a str) -> (usize, Vec<&'a str>, Vec<&'a str>) {
    let b: Vec<&str> = before.split('\n').collect();
    let a: Vec<&str> = after.split('\n').collect();
    let prefix = b.iter().zip(&a).take_while(|(x, y)| x == y).count();
    let room = b.len().min(a.len()) - prefix;
    let suffix = b
        .iter()
        .rev()
        .zip(a.iter().rev())
        .take(room)
        .take_while(|(x, y)| x == y)
        .count();
    (
        prefix + 1,
        b[prefix..b.len() - suffix].to_vec(),
        a[prefix..a.len() - suffix].to_vec(),
    )
}

fn check(project: &mut Project, edit: FormEdit, line: usize, removed: &[&str], added: &[&str]) {
    let before = text(project);
    let errors_before = project.file(LAB).unwrap().document.parse().errors.len();

    let inverse = project.apply_form(&edit).unwrap();
    let after = text(project);
    assert_eq!(line_diff(&before, &after), (line, removed.to_vec(), added.to_vec()));
    assert_eq!(
        project.file(LAB).unwrap().document.parse().errors.len(),
        errors_before,
        "The form edit adds a syntax error."
    );

    project.edit(LAB, &inverse).unwrap();
    assert!(text(project) == before, "The inverse edit does not restore the file.");
}

#[test]
fn g1_add_fact_after_the_last_formula_of_the_term() {
    let Some((_dir, mut project)) = open() else { return };
    let edit = FormEdit::AddFact {
        file: LAB,
        relation: "rosFrameId".into(),
        args: vec![
            FormValue::Symbol("HeadLidarLeft".into()),
            FormValue::Str("head_lidar_left".into()),
        ],
    };
    check(&mut project, edit, 545, &[], &["(rosFrameId HeadLidarLeft \"head_lidar_left\")"]);
}

#[test]
fn g2_change_one_argument() {
    let Some((_dir, mut project)) = open() else { return };
    let formula = find(&project, "attribute", "SceneCameraArray", |args| {
        is(&project, &args[1], "NotYetUsedStatus")
    });
    let edit = FormEdit::ChangeArgument {
        formula,
        argument: 1,
        value: FormValue::Symbol("InUseStatus".into()),
    };
    check(
        &mut project,
        edit,
        508,
        &["(attribute SceneCameraArray NotYetUsedStatus)"],
        &["(attribute SceneCameraArray InUseStatus)"],
    );
}

#[test]
fn g3_delete_fact() {
    let Some((_dir, mut project)) = open() else { return };
    let formula = find(&project, "todoNote", "HeadCameraCenter", |_| true);
    check(
        &mut project,
        FormEdit::DeleteFact { formula },
        598,
        &["(todoNote HeadCameraCenter \"確認鏡頭類型與視角。\")"],
        &[],
    );
}

#[test]
fn g4a_change_one_line_string() {
    let Some((_dir, mut project)) = open() else { return };
    let formula = find(&project, "documentation", "HeadCameraCenter", |_| true);
    let edit = FormEdit::ChangeString {
        formula,
        argument: 2,
        text: "頭部中央的高解析彩色相機，用於辨識物體。".into(),
    };
    check(
        &mut project,
        edit,
        597,
        &["  \"頭部中央的高解析彩色相機。\")"],
        &["  \"頭部中央的高解析彩色相機，用於辨識物體。\")"],
    );
}

#[test]
fn g4b_change_a_multi_line_string() {
    let Some((_dir, mut project)) = open() else { return };
    let formula = find(&project, "documentation", "GripperCamera", |_| true);
    let Term::App(_, args) = &project.index().formula(formula).unwrap().term else {
        unreachable!()
    };
    let Term::Str(current) = &args[2] else {
        panic!("The documentation has no string.")
    };
    let new_text = project
        .interner()
        .resolve(*current)
        .replace("只當作相機與深度感測器", "當作相機與深度感測器");
    let edit = FormEdit::ChangeString {
        formula,
        argument: 2,
        text: new_text,
    };
    check(
        &mut project,
        edit,
        620,
        &["目前的使用方式：只當作相機與深度感測器，RVC2 的神經網路運算能力沒有使用。"],
        &["目前的使用方式：當作相機與深度感測器，RVC2 的神經網路運算能力沒有使用。"],
    );
}

#[test]
fn invalid_form_values_are_rejected() {
    let Some((_dir, project)) = open() else { return };
    let formula = find(&project, "documentation", "HeadCameraCenter", |_| true);

    let string = |text: &str| FormEdit::ChangeString {
        formula,
        argument: 2,
        text: text.into(),
    };
    assert_eq!(project.preview_form(&string("a \"b\"")), Err(FormError::QuotationMark));

    let change = |argument, value| FormEdit::ChangeArgument {
        formula,
        argument,
        value,
    };
    assert!(matches!(
        project.preview_form(&change(0, FormValue::Symbol("Head Camera".into()))),
        Err(FormError::BadSymbol(_))
    ));
    assert!(matches!(
        project.preview_form(&change(0, FormValue::Symbol("?X".into()))),
        Err(FormError::BadSymbol(_))
    ));
    assert!(matches!(
        project.preview_form(&change(0, FormValue::Number("3x".into()))),
        Err(FormError::BadNumber(_))
    ));
    assert_eq!(
        project.preview_form(&change(9, FormValue::Symbol("A".into()))),
        Err(FormError::NoArgument(9))
    );
    assert_eq!(
        project.preview_form(&FormEdit::ChangeString {
            formula,
            argument: 0,
            text: "x".into()
        }),
        Err(FormError::NotAString)
    );
}
