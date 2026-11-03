use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

use kifedit_project::{FileRole, Project};
use serde::Serialize;
use tauri::State;

#[derive(Default)]
pub struct AppState {
    pub project: Mutex<Option<Project>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSummary {
    path: String,
    role: &'static str,
    formulas: usize,
    syntax_errors: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    name: String,
    root: String,
    files: Vec<FileSummary>,
    formula_count: usize,
    load_ms: f64,
}

// CPU 工作放在背景執行緒，不阻塞 IPC（§3.2）
#[tauri::command]
pub async fn open_project(path: String, state: State<'_, AppState>) -> Result<ProjectSummary, String> {
    let start = Instant::now();
    let path = PathBuf::from(path);
    let project = tauri::async_runtime::spawn_blocking(move || Project::open(&path))
        .await
        .map_err(|e| format!("The loading task stopped: {e}"))?
        .map_err(|e| e.to_string())?;
    let load_ms = start.elapsed().as_secs_f64() * 1000.0;

    let summary = summarize(&project, load_ms);
    *state.project.lock().unwrap() = Some(project);
    Ok(summary)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeNode {
    name: String,
    kind: &'static str, // "class" 或 "instance"
    has_children: bool,
    other_parents: usize,
}

const NO_PROJECT: &str = "Open a project first.";

/// 樹狀介面的子節點，展開時才查詢（§8.1）。node 為 None 時回傳根節點
#[tauri::command]
pub fn tree_children(view: String, node: Option<String>, state: State<'_, AppState>) -> Result<Vec<TreeNode>, String> {
    let guard = state.project.lock().unwrap();
    let project = guard.as_ref().ok_or(NO_PROJECT)?;
    if view != "classes" {
        return Err(format!("The tree view {view:?} is not available."));
    }
    let interner = project.interner();
    let taxonomy = project.taxonomy();
    let hierarchy = &taxonomy.subclass;

    let (mut classes, mut instances) = match node {
        None => {
            let roots = match interner.get("Entity").filter(|&s| hierarchy.contains(s)) {
                Some(entity) => vec![entity],
                None => hierarchy.nodes().filter(|&s| hierarchy.parents(s).is_empty()).collect(),
            };
            (roots, Vec::new())
        }
        Some(name) => match interner.get(&name) {
            Some(symbol) => (
                hierarchy.children(symbol).to_vec(),
                taxonomy.instances_of(symbol).to_vec(),
            ),
            None => (Vec::new(), Vec::new()),
        },
    };
    classes.sort_by_key(|&s| interner.resolve(s));
    instances.sort_by_key(|&s| interner.resolve(s));

    let class_nodes = classes.into_iter().map(|s| TreeNode {
        name: interner.resolve(s).to_string(),
        kind: "class",
        has_children: !hierarchy.children(s).is_empty() || !taxonomy.instances_of(s).is_empty(),
        other_parents: hierarchy.parents(s).len().saturating_sub(1),
    });
    let instance_nodes = instances.into_iter().map(|s| TreeNode {
        name: interner.resolve(s).to_string(),
        kind: "instance",
        has_children: false,
        other_parents: taxonomy.instance_of(s).len().saturating_sub(1),
    });
    Ok(class_nodes.chain(instance_nodes).collect())
}

fn summarize(project: &Project, load_ms: f64) -> ProjectSummary {
    let files = project
        .files()
        .iter()
        .map(|file| FileSummary {
            path: file.path.display().to_string(),
            role: match file.role {
                FileRole::Library => "library",
                FileRole::Editable => "editable",
            },
            formulas: project
                .index()
                .file(file.id)
                .map_or(0, |f| f.formulas().len()),
            syntax_errors: file.document.parse().errors.len(),
        })
        .collect();
    ProjectSummary {
        name: project.config().project.name.clone(),
        root: project.root().display().to_string(),
        files,
        formula_count: project.index().formula_count(),
        load_ms,
    }
}
