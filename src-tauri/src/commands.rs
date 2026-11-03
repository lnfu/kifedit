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

const NO_PROJECT: &str = "Open a project first.";

#[tauri::command]
pub fn tree_children(
    view: String,
    node: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<crate::tree::TreeNode>, String> {
    let guard = state.project.lock().unwrap();
    let project = guard.as_ref().ok_or(NO_PROJECT)?;
    crate::tree::children(project, &view, node.as_deref())
}

#[tauri::command]
pub fn term_page(name: String, state: State<'_, AppState>) -> Result<crate::term_page::TermPage, String> {
    let guard = state.project.lock().unwrap();
    let project = guard.as_ref().ok_or(NO_PROJECT)?;
    Ok(crate::term_page::build(project, &name))
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
