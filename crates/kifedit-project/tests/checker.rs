use std::fs;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use kifedit_project::{Checker, Project, Update};
use kifedit_sema::FileId;
use kifedit_syntax::{TextEdit, TextRange};

const LIBRARY: &str = "(instance instance BinaryPredicate)\n(instance BinaryPredicate Entity)\n";
const LAB: &str = "(instance Arm Entity)\n";
const LAB_ID: FileId = FileId(1);

fn setup() -> (tempfile::TempDir, Arc<Mutex<Project>>, Checker, mpsc::Receiver<Update>) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("base.kif"), LIBRARY).unwrap();
    fs::write(dir.path().join("lab.kif"), LAB).unwrap();
    let config = dir.path().join("project.toml");
    fs::write(
        &config,
        "[project]\nname = \"test\"\n\n[files]\nreadonly = [\"base.kif\"]\n\n[checks]\nW002 = \"off\"\nI001 = \"off\"\n",
    )
    .unwrap();
    let project = Arc::new(Mutex::new(Project::open(&config).unwrap()));
    let (sender, receiver) = mpsc::channel();
    let shared = project.clone();
    let checker = Checker::spawn(
        move || Some(shared.lock().unwrap().snapshot()),
        move |update| {
            let _ = sender.send(update);
        },
    );
    (dir, project, checker, receiver)
}

fn next(receiver: &mpsc::Receiver<Update>) -> Update {
    receiver.recv_timeout(Duration::from_secs(10)).expect("The checker sent no update.")
}

#[test]
fn check_all_sends_every_file_editable_first() {
    let (_dir, _project, checker, receiver) = setup();
    checker.check_all();
    let first = next(&receiver);
    let second = next(&receiver);
    assert_eq!((first.file, second.file), (LAB_ID, FileId(0)));
    assert!(first.diagnostics.is_empty());
}

#[test]
fn edit_sends_the_edited_file_with_the_new_version() {
    let (_dir, project, checker, receiver) = setup();
    checker.check_all();
    next(&receiver);
    next(&receiver);

    // 加一個未定義的詞彙
    let version = {
        let mut project = project.lock().unwrap();
        let end = LAB.len() as u32;
        project
            .edit(LAB_ID, &[TextEdit::insert(end.into(), "(instance Hand Limb)\n")])
            .unwrap();
        project.version(LAB_ID)
    };
    checker.edited(LAB_ID);
    let update = next(&receiver);
    assert_eq!((update.file, update.version), (LAB_ID, version));
    let codes: Vec<&str> = update.diagnostics.iter().map(|d| d.diagnostic.code).collect();
    assert_eq!(codes, ["E002"]);

    // 函式庫檔的結果沒有變，不再送出
    assert!(receiver.recv_timeout(Duration::from_millis(500)).is_err());

    // 刪掉後錯誤消失
    {
        let mut project = project.lock().unwrap();
        let start = LAB.len() as u32;
        let end = start + "(instance Hand Limb)\n".len() as u32;
        project
            .edit(LAB_ID, &[TextEdit::delete(TextRange::new(start.into(), end.into()))])
            .unwrap();
    }
    checker.edited(LAB_ID);
    assert!(next(&receiver).diagnostics.is_empty());
}
