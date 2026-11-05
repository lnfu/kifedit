//! M2 關卡 1：打字到診斷更新 ≤ 300 毫秒（含 debounce），完整 SUMO＋mm.kif。
//! 需要 release 模式：cargo test --release -p kifedit-project --test latency -- --ignored --nocapture

use std::fs;
use std::path::Path;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use kifedit_project::{Checker, Project, Update};
use kifedit_syntax::{TextEdit, TextRange};

#[test]
#[ignore = "needs full SUMO and the release build"]
fn typing_to_diagnostics_is_within_300_ms() {
    let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
    let lab = data.join("fixtures/mm.kif");
    let sumo = data.join("sumo");
    if !lab.is_file() || !sumo.is_dir() {
        eprintln!("Skip the test. Cannot find {} or {}.", lab.display(), sumo.display());
        return;
    }
    let mut library: Vec<String> = fs::read_dir(&sumo)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "kif"))
        .map(|p| format!("{p:?}"))
        .collect();
    library.sort();
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("project.toml");
    fs::write(
        &config,
        format!(
            "[project]\nname = \"latency\"\n\n[files]\nreadonly = [{}]\n",
            library.join(", ")
        ),
    )
    .unwrap();
    fs::copy(&lab, dir.path().join("mm.kif")).unwrap();

    let project = Arc::new(Mutex::new(Project::open(&config).unwrap()));
    let lab_id = project
        .lock()
        .unwrap()
        .files()
        .iter()
        .find(|f| f.path.ends_with("mm.kif"))
        .unwrap()
        .id;
    let (sender, receiver) = mpsc::channel::<Update>();
    let shared = project.clone();
    let checker = Checker::spawn(
        move || Some(shared.lock().unwrap().snapshot()),
        move |update| {
            let _ = sender.send(update);
        },
    );

    // 先完成第一次完整檢查
    checker.check_all();
    let count = project.lock().unwrap().files().len();
    for _ in 0..count {
        receiver.recv_timeout(Duration::from_secs(60)).unwrap();
    }

    // 四種修改輪流：註記中打字、刪字、改關係名、改 domain
    let text = project.lock().unwrap().text(lab_id).unwrap().to_string();
    let note = text.find("型號尚未確認").unwrap() as u32;
    let relation = text.find("(rosTopic").unwrap() as u32 + 1;
    let domain = text.find("(domain rosTopic 2 SymbolicString)").unwrap() as u32 + "(domain rosTopic 2 ".len() as u32;
    let edits = [
        vec![TextEdit::insert(note.into(), "x")],
        vec![TextEdit::delete(TextRange::new(note.into(), (note + 1).into()))],
        vec![TextEdit::replace(TextRange::new(relation.into(), (relation + 8).into()), "rosTopiX")],
        vec![TextEdit::replace(TextRange::new(relation.into(), (relation + 8).into()), "rosTopic")],
        vec![TextEdit::replace(TextRange::new(domain.into(), (domain + 14).into()), "SymbolicStrinX")],
        vec![TextEdit::replace(TextRange::new(domain.into(), (domain + 14).into()), "SymbolicString")],
    ];

    let mut times = Vec::new();
    for i in 0..100 {
        let version = {
            let mut project = project.lock().unwrap();
            project.edit(lab_id, &edits[i % edits.len()]).unwrap();
            project.version(lab_id)
        };
        let start = Instant::now();
        checker.edited(lab_id);
        loop {
            let update = receiver.recv_timeout(Duration::from_secs(10)).unwrap();
            if update.file == lab_id && update.version == version {
                break;
            }
        }
        times.push(start.elapsed().as_secs_f64() * 1000.0);
        // 讓背景的函式庫檔驗證跑完，再做下一次修改
        while receiver.recv_timeout(Duration::from_millis(300)).is_ok() {}
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p95 = times[94];
    eprintln!(
        "typing to diagnostics: median {:.0} ms, p95 {p95:.0} ms, max {:.0} ms",
        times[49], times[99]
    );
    assert!(p95 <= 300.0, "p95 is {p95:.0} ms. The limit is 300 ms.");
}
