use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

use kifedit_project::{EditFileError, FileRole, FormEdit, FormValue, Project};
use kifedit_sema::{FileId, FormulaId};
use kifedit_syntax::{TextEdit, TextRange, apply_edits as apply_text_edits};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::history::{History, Source};

// 文件版本號（§14.4）：每套用一組修改加 1。只在持有 project 鎖時讀寫
#[derive(Default)]
pub struct Session {
    pub project: Option<Project>,
    pub versions: HashMap<u32, u64>,
    pub saved: HashMap<u32, u64>, // 最後一次存檔時的版本號
    pub histories: HashMap<u32, History>,
}

#[derive(Default)]
pub struct AppState {
    pub session: Mutex<Session>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSummary {
    id: u32,
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
    let mut session = state.session.lock().unwrap();
    session.project = Some(project);
    session.versions.clear();
    session.saved.clear();
    session.histories.clear();
    Ok(summary)
}

const NO_PROJECT: &str = "Open a project first.";

#[derive(Deserialize)]
pub struct EditInput {
    from: u32,
    to: u32,
    insert: String,
}

#[derive(Serialize)]
pub struct Diagnostic {
    from: u32,
    to: u32,
    code: &'static str,
    message: &'static str,
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum EditOutcome {
    Applied { version: u64, errors: Vec<Diagnostic> },
    Conflict { version: u64, text: String },
}

/// 套用前端的修改（位置為修改前原文的 UTF-8 位元組）。版本號不一致或修改不合法時，
/// 回傳後端的全文，由前端重新同步：文字檔以後端為準（§14.4）
#[tauri::command]
pub fn apply_edits(
    file: u32,
    version: u64,
    edits: Vec<EditInput>,
    state: State<'_, AppState>,
) -> Result<EditOutcome, String> {
    let mut session = state.session.lock().unwrap();
    let Session {
        project,
        versions,
        histories,
        ..
    } = &mut *session;
    let project = project.as_mut().ok_or(NO_PROJECT)?;
    let current = versions.entry(file).or_insert(0);

    let conflict = |project: &Project, version: u64| -> Result<EditOutcome, String> {
        let text = project
            .text(FileId(file))
            .ok_or("There is no file with this ID.")?
            .to_string();
        Ok(EditOutcome::Conflict { version, text })
    };
    if version != *current {
        return conflict(project, *current);
    }

    let edits: Vec<TextEdit> = edits
        .into_iter()
        .map(|e| TextEdit::replace(TextRange::new(e.from.into(), e.to.into()), e.insert))
        .collect();
    match project.edit(FileId(file), &edits) {
        Ok(inverse) => histories
            .entry(file)
            .or_default()
            .record(Source::Editor, edits, inverse),
        Err(EditFileError::Edit(_)) => return conflict(project, *current),
        Err(e) => return Err(e.to_string()),
    }
    *current += 1;
    Ok(EditOutcome::Applied {
        version: *current,
        errors: diagnostics(project, file),
    })
}

#[derive(Serialize)]
pub struct FileState {
    version: u64,
    text: String,
    errors: Vec<Diagnostic>,
}

fn file_state(session: &Session, file: u32) -> Result<FileState, String> {
    let project = session.project.as_ref().ok_or(NO_PROJECT)?;
    let text = project
        .text(FileId(file))
        .ok_or("There is no file with this ID.")?;
    Ok(FileState {
        version: session.versions.get(&file).copied().unwrap_or(0),
        text: text.to_string(),
        errors: diagnostics(project, file),
    })
}

/// 復原或重做一個單位；沒有可復原的修改時回傳 None
fn step_history(state: &AppState, file: u32, redo: bool) -> Result<Option<FileState>, String> {
    let mut session = state.session.lock().unwrap();
    let Session {
        project,
        versions,
        histories,
        ..
    } = &mut *session;
    let project = project.as_mut().ok_or(NO_PROJECT)?;
    let history = histories.entry(file).or_default();
    let changed = if redo {
        history.redo(project, FileId(file))
    } else {
        history.undo(project, FileId(file))
    }
    .map_err(|e| e.to_string())?;
    if !changed {
        return Ok(None);
    }
    *versions.entry(file).or_insert(0) += 1;
    file_state(&session, file).map(Some)
}

#[derive(Deserialize)]
#[serde(tag = "kind", content = "text", rename_all = "camelCase")]
pub enum ValueInput {
    Symbol(String),
    String(String),
    Number(String),
}

impl From<ValueInput> for FormValue {
    fn from(value: ValueInput) -> Self {
        match value {
            ValueInput::Symbol(s) => FormValue::Symbol(s),
            ValueInput::String(s) => FormValue::Str(s),
            ValueInput::Number(s) => FormValue::Number(s),
        }
    }
}

#[derive(Deserialize, Clone, Copy)]
pub struct FormulaInput {
    file: u32,
    index: u32,
}

impl From<FormulaInput> for FormulaId {
    fn from(f: FormulaInput) -> Self {
        FormulaId {
            file: FileId(f.file),
            index: f.index,
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum FormInput {
    AddFact {
        file: u32,
        relation: String,
        args: Vec<ValueInput>,
    },
    ChangeArgument {
        formula: FormulaInput,
        argument: usize,
        value: ValueInput,
    },
    DeleteFact {
        formula: FormulaInput,
    },
    ChangeString {
        formula: FormulaInput,
        argument: usize,
        text: String,
    },
}

impl FormInput {
    fn into_edit(self) -> (u32, FormEdit) {
        match self {
            FormInput::AddFact {
                file,
                relation,
                args,
            } => (
                file,
                FormEdit::AddFact {
                    file: FileId(file),
                    relation,
                    args: args.into_iter().map(Into::into).collect(),
                },
            ),
            FormInput::ChangeArgument {
                formula,
                argument,
                value,
            } => (
                formula.file,
                FormEdit::ChangeArgument {
                    formula: formula.into(),
                    argument,
                    value: value.into(),
                },
            ),
            FormInput::DeleteFact { formula } => (
                formula.file,
                FormEdit::DeleteFact {
                    formula: formula.into(),
                },
            ),
            FormInput::ChangeString {
                formula,
                argument,
                text,
            } => (
                formula.file,
                FormEdit::ChangeString {
                    formula: formula.into(),
                    argument,
                    text,
                },
            ),
        }
    }
}

// 預覽：會變動的行，以共同前綴與後綴比較
#[derive(Serialize)]
pub struct Preview {
    line: usize,
    removed: Vec<String>,
    added: Vec<String>,
}

#[tauri::command]
pub fn preview_form(input: FormInput, state: State<'_, AppState>) -> Result<Preview, String> {
    let session = state.session.lock().unwrap();
    let project = session.project.as_ref().ok_or(NO_PROJECT)?;
    let (file, edit) = input.into_edit();
    let edits = project.preview_form(&edit).map_err(|e| e.to_string())?;
    let before = project
        .text(FileId(file))
        .ok_or("There is no file with this ID.")?;
    let mut after = before.to_string();
    apply_text_edits(&mut after, &edits).map_err(|e| e.to_string())?;
    Ok(line_diff(before, &after))
}

fn line_diff(before: &str, after: &str) -> Preview {
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
    let lines = |v: &[&str]| v.iter().map(|s| s.to_string()).collect();
    Preview {
        line: prefix + 1,
        removed: lines(&b[prefix..b.len() - suffix]),
        added: lines(&a[prefix..a.len() - suffix]),
    }
}

#[derive(Serialize)]
pub struct FormApplied {
    file: u32,
    version: u64,
}

#[tauri::command]
pub fn apply_form(input: FormInput, state: State<'_, AppState>) -> Result<FormApplied, String> {
    let mut session = state.session.lock().unwrap();
    let project = session.project.as_ref().ok_or(NO_PROJECT)?;
    let (file, edit) = input.into_edit();
    let edits = project.preview_form(&edit).map_err(|e| e.to_string())?;
    let version = apply_recorded(&mut session, file, edits, Source::Form)?;
    Ok(FormApplied { file, version })
}

// 套用一組修改並記入復原歷史，回傳新的版本號
fn apply_recorded(
    session: &mut Session,
    file: u32,
    edits: Vec<TextEdit>,
    source: Source,
) -> Result<u64, String> {
    let project = session.project.as_mut().ok_or(NO_PROJECT)?;
    let inverse = project
        .edit(FileId(file), &edits)
        .map_err(|e| e.to_string())?;
    session
        .histories
        .entry(file)
        .or_default()
        .record(source, edits, inverse);
    let version = session.versions.entry(file).or_insert(0);
    *version += 1;
    Ok(*version)
}

#[tauri::command]
pub fn search_names(query: String, state: State<'_, AppState>) -> Result<Vec<crate::search::NameHit>, String> {
    let session = state.session.lock().unwrap();
    let project = session.project.as_ref().ok_or(NO_PROJECT)?;
    Ok(crate::search::names(project, &query))
}

#[tauri::command]
pub fn search_text(query: String, state: State<'_, AppState>) -> Result<Vec<crate::search::TextHit>, String> {
    let session = state.session.lock().unwrap();
    let project = session.project.as_ref().ok_or(NO_PROJECT)?;
    Ok(crate::search::text(project, &query))
}

#[tauri::command]
pub fn run_query(query: String, state: State<'_, AppState>) -> Result<crate::search::QueryOutcome, String> {
    let session = state.session.lock().unwrap();
    let project = session.project.as_ref().ok_or(NO_PROJECT)?;
    crate::search::query(project, &query)
}

/// 格式化整個檔案；有 offset 時只格式化該位置的頂層公式。沒有變動時回傳 None
#[tauri::command]
pub fn format_file(file: u32, offset: Option<u32>, state: State<'_, AppState>) -> Result<Option<FileState>, String> {
    let mut session = state.session.lock().unwrap();
    let project = session.project.as_ref().ok_or(NO_PROJECT)?;
    let edit = match offset {
        Some(offset) => crate::format::formula_edit(project, FileId(file), offset)?,
        None => crate::format::file_edit(project, FileId(file))?,
    };
    let Some(edit) = edit else { return Ok(None) };
    apply_recorded(&mut session, file, vec![edit], Source::Form)?;
    file_state(&session, file).map(Some)
}

#[derive(Serialize)]
pub struct FormatChange {
    file: u32,
    path: String,
    preview: Preview,
}

#[derive(Serialize)]
pub struct FormatPlan {
    changes: Vec<FormatChange>,
    skipped: Vec<String>, // 有語法錯誤而不格式化的檔案
}

/// Format Project 的預覽：只包含 editable 檔案（§8.6）
#[tauri::command]
pub fn format_project_preview(state: State<'_, AppState>) -> Result<FormatPlan, String> {
    let session = state.session.lock().unwrap();
    let project = session.project.as_ref().ok_or(NO_PROJECT)?;
    let mut plan = FormatPlan {
        changes: Vec::new(),
        skipped: Vec::new(),
    };
    for source in project.files().iter().filter(|f| f.role == FileRole::Editable) {
        let path = source.path.display().to_string();
        match crate::format::file_edit(project, source.id) {
            Ok(Some(edit)) => {
                let before = project.text(source.id).unwrap_or_default();
                let mut after = before.to_string();
                apply_text_edits(&mut after, &[edit]).map_err(|e| e.to_string())?;
                plan.changes.push(FormatChange {
                    file: source.id.0,
                    path,
                    preview: line_diff(before, &after),
                });
            }
            Ok(None) => {}
            Err(_) => plan.skipped.push(path),
        }
    }
    Ok(plan)
}

#[derive(Serialize)]
pub struct FileVersion {
    file: u32,
    version: u64,
}

#[tauri::command]
pub fn format_project(state: State<'_, AppState>) -> Result<Vec<FileVersion>, String> {
    let mut session = state.session.lock().unwrap();
    let project = session.project.as_ref().ok_or(NO_PROJECT)?;
    let edits: Vec<(u32, TextEdit)> = project
        .files()
        .iter()
        .filter(|f| f.role == FileRole::Editable)
        .filter_map(|f| match crate::format::file_edit(project, f.id) {
            Ok(Some(edit)) => Some((f.id.0, edit)),
            _ => None,
        })
        .collect();
    edits
        .into_iter()
        .map(|(file, edit)| {
            let version = apply_recorded(&mut session, file, vec![edit], Source::Form)?;
            Ok(FileVersion { file, version })
        })
        .collect()
}

#[tauri::command]
pub fn undo(file: u32, state: State<'_, AppState>) -> Result<Option<FileState>, String> {
    step_history(&state, file, false)
}

#[tauri::command]
pub fn redo(file: u32, state: State<'_, AppState>) -> Result<Option<FileState>, String> {
    step_history(&state, file, true)
}

#[tauri::command]
pub fn save_file(file: u32, state: State<'_, AppState>) -> Result<u64, String> {
    let mut session = state.session.lock().unwrap();
    let project = session.project.as_ref().ok_or(NO_PROJECT)?;
    project
        .save(FileId(file))
        .map_err(|e| format!("Cannot save the file: {e}"))?;
    let version = session.versions.get(&file).copied().unwrap_or(0);
    session.saved.insert(file, version);
    Ok(version)
}

fn diagnostics(project: &Project, file: u32) -> Vec<Diagnostic> {
    project
        .parse(FileId(file))
        .map(|p| {
            p.errors
                .iter()
                .map(|e| Diagnostic {
                    from: e.range.start().into(),
                    to: e.range.end().into(),
                    code: e.kind.code(),
                    message: e.kind.message(),
                })
                .collect()
        })
        .unwrap_or_default()
}

#[tauri::command]
pub fn tree_children(
    view: String,
    node: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<crate::tree::TreeNode>, String> {
    let session = state.session.lock().unwrap();
    let project = session.project.as_ref().ok_or(NO_PROJECT)?;
    crate::tree::children(project, &view, node.as_deref())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileContent {
    id: u32,
    path: String,
    read_only: bool,
    text: String,
    version: u64,
    saved_version: u64,
    errors: Vec<Diagnostic>,
}

#[tauri::command]
pub fn file_text(file: u32, state: State<'_, AppState>) -> Result<FileContent, String> {
    let session = state.session.lock().unwrap();
    let project = session.project.as_ref().ok_or(NO_PROJECT)?;
    let source = project
        .file(FileId(file))
        .ok_or("There is no file with this ID.")?;
    Ok(FileContent {
        id: file,
        path: source.path.display().to_string(),
        read_only: source.role == FileRole::Library,
        text: project.text(FileId(file)).unwrap_or_default().to_string(),
        version: session.versions.get(&file).copied().unwrap_or(0),
        saved_version: session.saved.get(&file).copied().unwrap_or(0),
        errors: diagnostics(project, file),
    })
}

#[tauri::command]
pub fn term_page(name: String, state: State<'_, AppState>) -> Result<crate::term_page::TermPage, String> {
    let session = state.session.lock().unwrap();
    let project = session.project.as_ref().ok_or(NO_PROJECT)?;
    Ok(crate::term_page::build(project, &name))
}

fn summarize(project: &Project, load_ms: f64) -> ProjectSummary {
    let files = project
        .files()
        .iter()
        .map(|file| FileSummary {
            id: file.id.0,
            path: file.path.display().to_string(),
            role: match file.role {
                FileRole::Library => "library",
                FileRole::Editable => "editable",
            },
            formulas: project
                .index()
                .file(file.id)
                .map_or(0, |f| f.formulas().len()),
            syntax_errors: project.parse(file.id).map_or(0, |p| p.errors.len()),
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
