use std::fs;
use std::path::Path;
use std::time::Instant;

use kifedit_project::{Project, QueryError};
use kifedit_reason::QueryResult;
use kifedit_sema::Term;

fn answers_for(project: &Project, query: &str) -> Vec<String> {
    let QueryResult::Answers(answers) = project.query(query, usize::MAX).unwrap() else {
        panic!("The query needs a prover: {query}");
    };
    answers
        .iter()
        .map(|a| match &a.bindings[0].1 {
            Term::Symbol(s) => project.interner().resolve(*s).to_string(),
            other => format!("{other:?}"),
        })
        .collect()
}

#[test]
fn query_text_is_parsed_and_evaluated() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("lab.kif"), "(subclass Camera Device)\n(instance HeadCamera Camera)\n").unwrap();
    let config = dir.path().join("project.toml");
    fs::write(&config, "[project]\nname = \"test\"\n").unwrap();
    let project = Project::open(&config).unwrap();

    assert_eq!(answers_for(&project, "(instance ?X Device)"), ["HeadCamera"]);
    assert_eq!(project.query("(instance ?X", 10), Err(QueryError::Syntax));
    assert_eq!(project.query("(a) (b)", 10), Err(QueryError::NotOneQuery));
    assert_eq!(project.query("", 10), Err(QueryError::NotOneQuery));
}

#[test]
fn query_speed_on_the_example_project() {
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
            "[project]\nname = \"mm\"\n\n[files]\nreadonly = [{:?}, {:?}]\n",
            library[0], library[1]
        ),
    )
    .unwrap();
    fs::copy(&lab, dir.path().join("mm.kif")).unwrap();
    let project = Project::open(&config).unwrap();

    assert!(answers_for(&project, "(instance ?X Robot)").contains(&"MyStretch4".to_string()));
    for query in [
        "(instance ?X Robot)",
        "(instance ?X Device)",
        "(instance MyStretch4 ?C)",
        "(subclass ?X Object)",
        "(robotSensingPart ?X MyStretch4)",
        "(documentation MyStretch4 ?L ?S)",
    ] {
        let start = Instant::now();
        let result = project.query(query, usize::MAX).unwrap();
        let elapsed = start.elapsed();
        let count = match result {
            QueryResult::Answers(answers) => answers.len(),
            QueryResult::NeedsProver(_) => 0,
        };
        eprintln!("Query {query}: {count} answers in {elapsed:?}.");
    }
}
