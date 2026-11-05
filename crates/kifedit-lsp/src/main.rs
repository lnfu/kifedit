mod line_index;

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
                Ok(Err(e)) => self.client.log_message(MessageType::ERROR, e.to_string()).await,
                Err(e) => self.client.log_message(MessageType::ERROR, e.to_string()).await,
            }
        }
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
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
}

impl Backend {
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
    let range = |r: TextRange| {
        Range::new(index.position(r.start().into()), index.position(r.end().into()))
    };

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
    let (service, socket) = LspService::new(|client| Backend {
        client,
        session: Default::default(),
    });
    Server::new(tokio::io::stdin(), tokio::io::stdout(), socket)
        .serve(service)
        .await;
}
