mod format;
mod line_index;
mod term;
mod tree;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use kifedit_project::{Checker, Project, Severity, Update, find_config};
use kifedit_sema::FileId;
use kifedit_syntax::{TextEdit, TextRange};
use line_index::LineIndex;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

#[derive(Default)]
struct Session {
    project: Option<Project>,
    checker: Option<Checker>,
}

struct Backend {
    client: Client,
    session: Arc<Mutex<Session>>,
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
            match tokio::task::spawn_blocking(move || Project::open(&config)).await {
                Ok(Ok(project)) => self.session.lock().unwrap().project = Some(project),
                Ok(Err(e)) => self.client.show_message(MessageType::ERROR, e.to_string()).await,
                Err(e) => self.client.show_message(MessageType::ERROR, e.to_string()).await,
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
            move || shared.lock().unwrap().project.as_ref().map(Project::snapshot),
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
        }
    }

    // 關閉未儲存的檔案時，內容回到硬碟上的版本
    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        if let Some(text) = uri.to_file_path().ok().and_then(|p| std::fs::read_to_string(p).ok()) {
            self.replace(&uri, text);
        }
    }

    async fn formatting(
        &self,
        params: DocumentFormattingParams,
    ) -> Result<Option<Vec<tower_lsp::lsp_types::TextEdit>>> {
        // 先放開鎖再 await
        let outcome = {
            let session = self.session.lock().unwrap();
            let Some(project) = session.project.as_ref() else { return Ok(None) };
            let Some(id) = file_id(project, &params.text_document.uri) else { return Ok(None) };
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
                self.client.show_message(MessageType::WARNING, message).await;
                Ok(None)
            }
        }
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let session = self.session.lock().unwrap();
        let Some(project) = session.project.as_ref() else { return Ok(None) };
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
        let Some(project) = session.project.as_ref() else { return Ok(None) };
        let Some((_, name, _)) = word_at(project, &params.text_document_position_params) else {
            return Ok(None);
        };
        let locations = locations(project, term::definitions(project, &name));
        Ok((!locations.is_empty()).then_some(GotoDefinitionResponse::Array(locations)))
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let session = self.session.lock().unwrap();
        let Some(project) = session.project.as_ref() else { return Ok(None) };
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
        let Some(project) = session.project.as_ref() else { return Ok(None) };
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
    each_location(project, ranges).into_iter().flatten().collect()
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
struct TermParams {
    name: String,
}

impl Backend {
    async fn tree_children(&self, params: tree::TreeParams) -> Result<Vec<tree::TreeNode>> {
        let session = self.session.lock().unwrap();
        Ok(session.project.as_ref().map(|p| tree::children(p, &params)).unwrap_or_default())
    }

    /// 詞彙的第一個定義，給樹狀檢視點選時開啟
    async fn term_location(&self, params: TermParams) -> Result<Option<Location>> {
        let session = self.session.lock().unwrap();
        let Some(project) = session.project.as_ref() else { return Ok(None) };
        let first = term::definitions(project, &params.name).into_iter().take(1).collect();
        Ok(locations(project, first).into_iter().next())
    }

    fn replace(&self, uri: &Url, text: String) {
        let mut session = self.session.lock().unwrap();
        let Session { project, checker } = &mut *session;
        let Some(project) = project.as_mut() else { return };
        let Some(id) = file_id(project, uri) else { return };
        let old = project.text(id).unwrap_or_default();
        if old == text {
            return;
        }
        let range = TextRange::new(0.into(), (old.len() as u32).into());
        if project.edit(id, &[TextEdit::replace(range, text)]).is_ok() {
            if let Some(checker) = checker {
                checker.edited(id);
            }
        }
    }
}

fn file_id(project: &Project, uri: &Url) -> Option<FileId> {
    let path = uri.to_file_path().ok()?;
    project.files().iter().find(|f| f.path == path).map(|f| f.id)
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
    })
    .custom_method("kifedit/treeChildren", Backend::tree_children)
    .custom_method("kifedit/termLocation", Backend::term_location)
    .finish();
    Server::new(tokio::io::stdin(), tokio::io::stdout(), socket)
        .serve(service)
        .await;
}
