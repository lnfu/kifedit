//! 背景檢查（§7.2、§14.3）。同 LSP：被編輯的檔案先檢查、先送出；
//! 其他檔案在背景重新驗證，只送出結果有變的檔案。修改專案時進行中的檢查會被 salsa 取消

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

use kifedit_sema::FileId;

use crate::{Diagnostic, FileRole, LocatedDiagnostic, Snapshot};

/// 停止輸入後多久開始檢查
pub const DEBOUNCE: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Update {
    pub file: FileId,
    pub version: u64,
    pub diagnostics: Vec<LocatedDiagnostic>,
}

enum Message {
    Edited(FileId),
    All,
    Stop,
}

pub struct Checker {
    sender: Sender<Message>,
    thread: Option<JoinHandle<()>>,
}

impl Checker {
    /// `snapshot` 在背景執行緒中呼叫，應該只短暫持有專案的鎖；`sink` 收到每一筆更新
    pub fn spawn(
        snapshot: impl Fn() -> Option<Snapshot> + Send + 'static,
        sink: impl FnMut(Update) + Send + 'static,
    ) -> Self {
        let (sender, receiver) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("kifedit-checker".into())
            .spawn(move || run(receiver, snapshot, sink))
            .expect("Cannot start the checker thread.");
        Self {
            sender,
            thread: Some(thread),
        }
    }

    /// 檔案被修改後呼叫
    pub fn edited(&self, file: FileId) {
        let _ = self.sender.send(Message::Edited(file));
    }

    /// 檢查所有檔案，例如開啟專案後
    pub fn check_all(&self) {
        let _ = self.sender.send(Message::All);
    }
}

impl Drop for Checker {
    fn drop(&mut self) {
        let _ = self.sender.send(Message::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(
    receiver: Receiver<Message>,
    snapshot: impl Fn() -> Option<Snapshot>,
    mut sink: impl FnMut(Update),
) {
    let mut first: Vec<FileId> = Vec::new(); // 被編輯的檔案，下一輪先檢查並一定送出
    let mut sent: HashMap<FileId, Vec<Diagnostic>> = HashMap::new();
    let mut pending = false;

    loop {
        let message = if pending {
            receiver.recv_timeout(DEBOUNCE)
        } else {
            receiver.recv().map_err(|_| RecvTimeoutError::Disconnected)
        };
        match message {
            Ok(Message::Stop) | Err(RecvTimeoutError::Disconnected) => return,
            Ok(Message::Edited(file)) => {
                if !first.contains(&file) {
                    first.push(file);
                }
                pending = true;
                continue;
            }
            Ok(Message::All) => {
                sent.clear();
                pending = true;
                continue;
            }
            Err(RecvTimeoutError::Timeout) => {}
        }

        let Some(snapshot) = snapshot() else {
            pending = false;
            continue;
        };
        let result = catch_unwind(AssertUnwindSafe(|| {
            check_round(&snapshot, &first, &mut sent, &mut sink);
        }));
        drop(snapshot);
        match result {
            Ok(()) => {
                first.clear();
                pending = false;
            }
            // 被修改取消：保留 first，等下一個通知後重來
            Err(payload) if payload.is::<salsa::Cancelled>() => pending = false,
            Err(payload) => resume_unwind(payload),
        }
    }
}

fn check_round(
    snapshot: &Snapshot,
    first: &[FileId],
    sent: &mut HashMap<FileId, Vec<Diagnostic>>,
    sink: &mut impl FnMut(Update),
) {
    let files = snapshot.files();
    let rest = |role| {
        files
            .iter()
            .filter(move |f| f.role == role && !first.contains(&f.id))
            .map(|f| f.id)
    };
    let order = first
        .iter()
        .copied()
        .chain(rest(FileRole::Editable))
        .chain(rest(FileRole::Library));
    for id in order {
        let Some(file) = files.get(id.0 as usize) else { continue };
        let diagnostics = snapshot.diagnostics(id);
        if !first.contains(&id) && sent.get(&id) == Some(&diagnostics) {
            continue;
        }
        sent.insert(id, diagnostics.clone());
        sink(Update {
            file: id,
            version: file.version,
            diagnostics: snapshot.locate(id, diagnostics),
        });
    }
}
