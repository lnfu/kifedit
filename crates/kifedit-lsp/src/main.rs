mod format;
mod forms;
mod line_index;
mod query;
mod semantic;
mod term;
mod term_page;
mod tree;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use kifedit_project::{
    CONFIG_FILE_NAME, Checker, FileRole, Project, Severity, Update, find_config,
};
use kifedit_sema::FileId;
use kifedit_syntax::{TextEdit, TextRange};
use line_index::LineIndex;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

#[derive(Default)]
struct Session {
    config: Option<PathBuf>,
    project: Option<Project>,
    checker: Option<Checker>,
    open: HashMap<Url, String>, // 編輯器中開啟的文件與其內容；不在專案中的文件也記錄，加入專案時使用
}

struct Backend {
    client: Client,
    session: Arc<Mutex<Session>>,
    tokens_requests: Arc<AtomicU64>, // 每次要求更新語意標記時加 1，只有最後一次會送出
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let root = params
            .workspace_folders
            .as_ref()
            .and_then(|f| f.first())
            .and_then(|f| f.uri.to_file_path().ok());
        if let Some(config) = root.as_deref().and_then(find_config) {
            self.session.lock().unwrap().config = Some(config.clone());
            match tokio::task::spawn_blocking(move || Project::open(&config)).await {
                Ok(Ok(project)) => self.session.lock().unwrap().project = Some(project),
                Ok(Err(e)) => {
                    self.client
                        .show_message(MessageType::ERROR, e.to_string())
                        .await
                }
                Err(e) => {
                    self.client
                        .show_message(MessageType::ERROR, e.to_string())
                        .await
                }
            }
        }
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                document_formatting_provider: Some(OneOf::Left(true)),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            legend: semantic::legend(),
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                            ..Default::default()
                        },
                    ),
                ),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        let shared = self.session.clone();
        let sink_session = self.session.clone();
        let client = self.client.clone();
        let handle = tokio::runtime::Handle::current();
        // sink 不可在 checker 執行緒上鎖 session：checker 持有快照時，edit 會等快照被丟棄
        let checker = Checker::spawn(
            move || {
                shared
                    .lock()
                    .unwrap()
                    .project
                    .as_ref()
                    .map(Project::snapshot)
            },
            move |update| {
                let session = sink_session.clone();
                let client = client.clone();
                handle.spawn(async move {
                    let published = to_lsp(&session.lock().unwrap(), update);
                    if let Some((uri, diagnostics)) = published {
                        client.publish_diagnostics(uri, diagnostics, None).await;
                    }
                });
            },
        );
        checker.check_all();
        self.session.lock().unwrap().checker = Some(checker);
    }

    async fn shutdown(&self) -> Result<()> {
        self.session.lock().unwrap().checker = None;
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.replace(&params.text_document.uri, params.text_document.text);
    }

    async fn did_change(&self, mut params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.pop() {
            self.replace(&params.text_document.uri, change.text);
            self.refresh_tokens_later();
        }
    }

    // 關閉未儲存的檔案時，內容回到硬碟上的版本
    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        if let Some(text) = uri
            .to_file_path()
            .ok()
            .and_then(|p| std::fs::read_to_string(p).ok())
        {
            self.replace(&uri, text);
        }
        self.session.lock().unwrap().open.remove(&uri);
    }

    // 插件監看 *.kif、.kifignore 與 kifedit.toml
    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        let config_changed = params
            .changes
            .iter()
            .any(|c| c.uri.path().ends_with(&format!("/{CONFIG_FILE_NAME}")));
        let outcome = if config_changed {
            self.reopen().await
        } else {
            self.refresh()
        };
        match outcome {
            Ok(removed) => {
                for uri in removed {
                    self.client.publish_diagnostics(uri, Vec::new(), None).await;
                }
                self.refresh_tokens_later();
            }
            Err(message) => self.client.show_message(MessageType::ERROR, message).await,
        }
    }

    async fn formatting(
        &self,
        params: DocumentFormattingParams,
    ) -> Result<Option<Vec<tower_lsp::lsp_types::TextEdit>>> {
        // 先放開鎖再 await
        let outcome = {
            let session = self.session.lock().unwrap();
            let Some(project) = session.project.as_ref() else {
                return Ok(None);
            };
            let Some(id) = file_id(project, &params.text_document.uri) else {
                return Ok(None);
            };
            format::file_edit(project, id).map(|edit| {
                let index = LineIndex::new(project.text(id).unwrap_or_default());
                edit.map(|e| tower_lsp::lsp_types::TextEdit {
                    range: index.range(e.range),
                    new_text: e.new_text,
                })
            })
        };
        match outcome {
            Ok(edit) => Ok(Some(edit.into_iter().collect())),
            Err(message) => {
                self.client
                    .show_message(MessageType::WARNING, message)
                    .await;
                Ok(None)
            }
        }
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let session = self.session.lock().unwrap();
        let Some(project) = session.project.as_ref() else {
            return Ok(None);
        };
        let Some(id) = file_id(project, &params.text_document.uri) else {
            return Ok(None);
        };
        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: semantic::tokens(project, id),
        })))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let session = self.session.lock().unwrap();
        let Some(project) = session.project.as_ref() else {
            return Ok(None);
        };
        let Some((file, name, range)) = word_at(project, &params.text_document_position_params)
        else {
            return Ok(None);
        };
        let index = LineIndex::new(project.text(file).unwrap_or_default());
        Ok(term::hover(project, &name).map(|value| Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value,
            }),
            range: Some(index.range(range)),
        }))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let session = self.session.lock().unwrap();
        let Some(project) = session.project.as_ref() else {
            return Ok(None);
        };
        let Some((_, name, _)) = word_at(project, &params.text_document_position_params) else {
            return Ok(None);
        };
        let locations = locations(project, term::definitions(project, &name));
        Ok((!locations.is_empty()).then_some(GotoDefinitionResponse::Array(locations)))
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let session = self.session.lock().unwrap();
        let Some(project) = session.project.as_ref() else {
            return Ok(None);
        };
        let Some((_, name, _)) = word_at(project, &params.text_document_position) else {
            return Ok(None);
        };
        Ok(Some(locations(project, term::references(project, &name))))
    }

    #[allow(deprecated)] // SymbolInformation.deprecated 欄位
    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        if params.query.is_empty() {
            return Ok(Some(Vec::new()));
        }
        let session = self.session.lock().unwrap();
        let Some(project) = session.project.as_ref() else {
            return Ok(None);
        };
        let found = term::symbols(project, &params.query);
        let places = each_location(project, found.iter().map(|s| (s.2, s.3)).collect());
        Ok(Some(
            found
                .into_iter()
                .zip(places)
                .filter_map(|((name, kinds, _, _), location)| {
                    Some(SymbolInformation {
                        name,
                        kind: symbol_kind(&kinds),
                        tags: None,
                        deprecated: None,
                        location: location?,
                        container_name: (!kinds.is_empty()).then(|| kinds.join(", ")),
                    })
                })
                .collect(),
        ))
    }
}

fn symbol_kind(kinds: &[&str]) -> SymbolKind {
    match kinds.first().copied() {
        Some("class") => SymbolKind::CLASS,
        Some("relation" | "function") => SymbolKind::FUNCTION,
        Some("attribute") => SymbolKind::ENUM_MEMBER,
        Some("individual") => SymbolKind::CONSTANT,
        _ => SymbolKind::VARIABLE,
    }
}

fn locations(project: &Project, ranges: Vec<(FileId, TextRange)>) -> Vec<Location> {
    each_location(project, ranges)
        .into_iter()
        .flatten()
        .collect()
}

/// 位元組範圍 → LSP Location，與輸入一對一；每個檔案只建一次 LineIndex
fn each_location(project: &Project, ranges: Vec<(FileId, TextRange)>) -> Vec<Option<Location>> {
    let mut indexes: HashMap<FileId, Option<(Url, LineIndex)>> = HashMap::new();
    ranges
        .into_iter()
        .map(|(file, range)| {
            let entry = indexes.entry(file).or_insert_with(|| {
                let uri = Url::from_file_path(&project.file(file)?.path).ok()?;
                Some((uri, LineIndex::new(project.text(file)?)))
            });
            let (uri, index) = entry.as_ref()?;
            Some(Location::new(uri.clone(), index.range(range)))
        })
        .collect()
}

fn word_at(
    project: &Project,
    position: &TextDocumentPositionParams,
) -> Option<(FileId, String, TextRange)> {
    let file = file_id(project, &position.text_document.uri)?;
    let offset = LineIndex::new(project.text(file)?).offset(position.position);
    let (name, range) = term::word_at(project, file, offset)?;
    Some((file, name, range))
}

#[derive(serde::Deserialize)]
struct FormParams {
    input: forms::FormInput,
}

#[derive(serde::Serialize)]
struct EditableFile {
    id: u32,
    path: String,
}

fn failure(message: impl Into<String>) -> tower_lsp::jsonrpc::Error {
    tower_lsp::jsonrpc::Error {
        code: tower_lsp::jsonrpc::ErrorCode::InvalidParams,
        message: message.into().into(),
        data: None,
    }
}

const NO_PROJECT: &str = "There is no open project.";

#[derive(serde::Deserialize)]
struct TermParams {
    name: String,
}

impl Backend {
    async fn term_page(&self, params: TermParams) -> Result<term_page::TermPage> {
        let session = self.session.lock().unwrap();
        let project = session
            .project
            .as_ref()
            .ok_or_else(|| failure(NO_PROJECT))?;
        Ok(term_page::build(project, &params.name))
    }

    async fn editable_files(&self) -> Result<Vec<EditableFile>> {
        let session = self.session.lock().unwrap();
        let project = session
            .project
            .as_ref()
            .ok_or_else(|| failure(NO_PROJECT))?;
        Ok(project
            .files()
            .filter(|f| f.role == FileRole::Editable)
            .map(|f| EditableFile {
                id: f.id.0,
                path: f.path.display().to_string(),
            })
            .collect())
    }

    async fn preview_form(&self, params: FormParams) -> Result<forms::Preview> {
        let session = self.session.lock().unwrap();
        let project = session
            .project
            .as_ref()
            .ok_or_else(|| failure(NO_PROJECT))?;
        let (file, edit) = params.input.into_edit();
        let edits = project
            .preview_form(&edit)
            .map_err(|e| failure(e.to_string()))?;
        let before = project
            .text(FileId(file))
            .ok_or_else(|| failure("There is no file with this ID."))?;
        let mut after = before.to_string();
        kifedit_syntax::apply_edits(&mut after, &edits).map_err(|e| failure(e.to_string()))?;
        Ok(forms::line_diff(before, &after))
    }

    /// 由 VS Code 套用修改：VS Code 擁有文件內容，套用後以 didChange 通知 server
    async fn apply_form(&self, params: FormParams) -> Result<()> {
        let change = {
            let session = self.session.lock().unwrap();
            let project = session
                .project
                .as_ref()
                .ok_or_else(|| failure(NO_PROJECT))?;
            let (file, edit) = params.input.into_edit();
            let file = FileId(file);
            let edits = project
                .preview_form(&edit)
                .map_err(|e| failure(e.to_string()))?;
            let source = project
                .file(file)
                .ok_or_else(|| failure("There is no file with this ID."))?;
            let uri = Url::from_file_path(&source.path)
                .map_err(|_| failure("The file path is not valid."))?;
            let index = LineIndex::new(project.text(file).unwrap_or_default());
            let edits = edits
                .into_iter()
                .map(|e| tower_lsp::lsp_types::TextEdit {
                    range: index.range(e.range),
                    new_text: e.new_text,
                })
                .collect();
            WorkspaceEdit::new(HashMap::from([(uri, edits)]))
        };
        let response = self.client.apply_edit(change).await?;
        if !response.applied {
            return Err(failure(
                response
                    .failure_reason
                    .unwrap_or_else(|| "VS Code did not apply the change.".into()),
            ));
        }
        Ok(())
    }

    async fn tree_children(&self, params: tree::TreeParams) -> Result<Vec<tree::TreeNode>> {
        let session = self.session.lock().unwrap();
        Ok(session
            .project
            .as_ref()
            .map(|p| tree::children(p, &params))
            .unwrap_or_default())
    }

    async fn query(&self, params: query::QueryParams) -> Result<query::QueryOutcome<Location>> {
        let session = self.session.lock().unwrap();
        let Some(project) = session.project.as_ref() else {
            return Ok(query::QueryOutcome::Error {
                message: "There is no open project.".into(),
            });
        };
        let (columns, rows) = match query::run(project, &params.query) {
            query::QueryOutcome::Answers { columns, rows } => (columns, rows),
            query::QueryOutcome::NeedsProver { message } => {
                return Ok(query::QueryOutcome::NeedsProver { message });
            }
            query::QueryOutcome::Error { message } => {
                return Ok(query::QueryOutcome::Error { message });
            }
        };
        let ranges = rows
            .iter()
            .flat_map(|r| r.support.iter().map(|s| s.location))
            .collect();
        let mut places = each_location(project, ranges).into_iter();
        let rows = rows
            .into_iter()
            .map(|row| query::Row {
                values: row.values,
                support: row
                    .support
                    .into_iter()
                    .filter_map(|s| {
                        Some(query::Support {
                            text: s.text,
                            location: places.next()??,
                        })
                    })
                    .collect(),
            })
            .collect();
        Ok(query::QueryOutcome::Answers { columns, rows })
    }

    // 修改可能改變 taxonomy，其他已開啟文件的語意標記也要重算；連續輸入時只在停下後送出一次
    fn refresh_tokens_later(&self) {
        let request = self.tokens_requests.fetch_add(1, Ordering::SeqCst) + 1;
        let (requests, client) = (self.tokens_requests.clone(), self.client.clone());
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            if requests.load(Ordering::SeqCst) == request {
                let _ = client.semantic_tokens_refresh().await;
            }
        });
    }

    fn replace(&self, uri: &Url, text: String) {
        let mut session = self.session.lock().unwrap();
        session.open.insert(uri.clone(), text.clone());
        let Session {
            project, checker, ..
        } = &mut *session;
        let Some(project) = project.as_mut() else {
            return;
        };
        let Some(id) = file_id(project, uri) else {
            return;
        };
        if set_text(project, id, text) {
            if let Some(checker) = checker {
                checker.edited(id);
            }
        }
    }

    /// 依磁碟與 .kifignore 更新專案的檔案；回傳被移除的檔案，以清除它們的診斷
    fn refresh(&self) -> std::result::Result<Vec<Url>, String> {
        let mut session = self.session.lock().unwrap();
        let Session {
            project,
            checker,
            open,
            ..
        } = &mut *session;
        let Some(project) = project.as_mut() else {
            return Ok(Vec::new());
        };
        let before: HashMap<FileId, PathBuf> =
            project.files().map(|f| (f.id, f.path.clone())).collect();
        let changes = project.refresh_files().map_err(|e| e.to_string())?;
        if changes == Default::default() {
            return Ok(Vec::new());
        }
        use_open_texts(project, open, &changes.added);
        if let Some(checker) = checker {
            changes.added.iter().for_each(|&id| checker.edited(id));
            checker.check_all();
        }
        Ok(changes
            .removed
            .iter()
            .filter_map(|id| Url::from_file_path(before.get(id)?).ok())
            .collect())
    }

    /// kifedit.toml 改變時重新開啟專案。開啟失敗時保留原本的專案
    async fn reopen(&self) -> std::result::Result<Vec<Url>, String> {
        let Some(config) = self.session.lock().unwrap().config.clone() else {
            return Ok(Vec::new());
        };
        let mut project = tokio::task::spawn_blocking(move || Project::open(&config))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;

        let mut session = self.session.lock().unwrap();
        let ids: Vec<FileId> = project.files().map(|f| f.id).collect();
        use_open_texts(&mut project, &session.open, &ids);
        let removed = session
            .project
            .as_ref()
            .map(|old| {
                old.files()
                    .filter(|f| project.file_id(&f.path).is_none())
                    .filter_map(|f| Url::from_file_path(&f.path).ok())
                    .collect()
            })
            .unwrap_or_default();
        session.project = Some(project);
        if let Some(checker) = &session.checker {
            checker.check_all();
        }
        Ok(removed)
    }
}

/// 取代整份文字；文字沒有改變或檔案是唯讀時回傳 false
fn set_text(project: &mut Project, id: FileId, text: String) -> bool {
    let old = project.text(id).unwrap_or_default();
    if old == text {
        return false;
    }
    let range = TextRange::new(0.into(), (old.len() as u32).into());
    project.edit(id, &[TextEdit::replace(range, text)]).is_ok()
}

// 新加入專案的檔案若已在編輯器中開啟，以編輯器的內容（可能未存檔）為準
fn use_open_texts(project: &mut Project, open: &HashMap<Url, String>, ids: &[FileId]) {
    for &id in ids {
        let Some(path) = project.file(id).map(|f| f.path.clone()) else {
            continue;
        };
        let Some(text) = Url::from_file_path(&path)
            .ok()
            .and_then(|uri| open.get(&uri))
        else {
            continue;
        };
        set_text(project, id, text.clone());
    }
}

fn file_id(project: &Project, uri: &Url) -> Option<FileId> {
    let path = uri.to_file_path().ok()?;
    project.file_id(&path)
}

fn to_lsp(session: &Session, update: Update) -> Option<(Url, Vec<Diagnostic>)> {
    let project = session.project.as_ref()?;
    if project.version(update.file) != update.version {
        return None;
    }
    let uri = Url::from_file_path(&project.file(update.file)?.path).ok()?;
    let index = LineIndex::new(project.text(update.file)?);
    let range = |r: TextRange| index.range(r);

    let mut out: Vec<Diagnostic> = project
        .parse(update.file)
        .into_iter()
        .flat_map(|p| &p.errors)
        .map(|e| Diagnostic {
            range: range(e.range),
            severity: Some(DiagnosticSeverity::ERROR),
            code: Some(NumberOrString::String(e.kind.code().into())),
            message: e.kind.message().to_string(),
            source: Some("kifedit".into()),
            ..Default::default()
        })
        .collect();
    out.extend(update.diagnostics.into_iter().filter_map(|located| {
        let d = located.diagnostic;
        let severity = match d.severity {
            Severity::Error => DiagnosticSeverity::ERROR,
            Severity::Warning => DiagnosticSeverity::WARNING,
            Severity::Info => DiagnosticSeverity::INFORMATION,
            Severity::Off => return None,
        };
        Some(Diagnostic {
            range: range(d.range),
            severity: Some(severity),
            code: Some(NumberOrString::String(d.code.into())),
            message: d.message,
            source: Some("kifedit".into()),
            ..Default::default()
        })
    }));
    Some((uri, out))
}

#[tokio::main]
async fn main() {
    let (service, socket) = LspService::build(|client| Backend {
        client,
        session: Default::default(),
        tokens_requests: Default::default(),
    })
    .custom_method("kifedit/treeChildren", Backend::tree_children)
    .custom_method("kifedit/query", Backend::query)
    .custom_method("kifedit/termPage", Backend::term_page)
    .custom_method("kifedit/editableFiles", Backend::editable_files)
    .custom_method("kifedit/previewForm", Backend::preview_form)
    .custom_method("kifedit/applyForm", Backend::apply_form)
    .finish();
    Server::new(tokio::io::stdin(), tokio::io::stdout(), socket)
        .serve(service)
        .await;
}
