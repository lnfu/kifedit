use std::time::{Duration, Instant};

use kifedit_project::{EditFileError, Project};
use kifedit_sema::FileId;
use kifedit_syntax::TextEdit;

// 編輯器中連續輸入，間隔小於此值時合併為一個復原單位（同 VS Code）
const GROUP_WINDOW: Duration = Duration::from_millis(500);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Editor,
    Form,
}

// forward 與 inverse 都以套用前的文字為準；復原時依相反順序套用各步的 inverse
struct Step {
    forward: Vec<TextEdit>,
    inverse: Vec<TextEdit>,
}

struct Entry {
    steps: Vec<Step>,
    source: Source,
    time: Instant,
    sealed: bool, // 表單修改與復原／重做產生的單位不再合併
}

/// 每個檔案一份復原歷史，表單與編輯器共用（§8.5）
#[derive(Default)]
pub struct History {
    undo: Vec<Entry>,
    redo: Vec<Entry>,
}

impl History {
    pub fn record(&mut self, source: Source, forward: Vec<TextEdit>, inverse: Vec<TextEdit>) {
        self.redo.clear();
        let now = Instant::now();
        let step = Step { forward, inverse };
        if let Some(last) = self.undo.last_mut() {
            let mergeable = source == Source::Editor
                && last.source == Source::Editor
                && !last.sealed
                && now.duration_since(last.time) < GROUP_WINDOW;
            if mergeable {
                last.steps.push(step);
                last.time = now;
                return;
            }
        }
        self.undo.push(Entry {
            steps: vec![step],
            source,
            time: now,
            sealed: source == Source::Form,
        });
    }

    /// 回傳是否有可復原的修改
    pub fn undo(&mut self, project: &mut Project, file: FileId) -> Result<bool, EditFileError> {
        let Some(entry) = self.undo.pop() else {
            return Ok(false);
        };
        let mut steps = Vec::with_capacity(entry.steps.len());
        for step in entry.steps.into_iter().rev() {
            let forward = project.edit(file, &step.inverse)?;
            steps.push(Step {
                forward,
                inverse: step.inverse,
            });
        }
        steps.reverse();
        self.redo.push(Entry {
            steps,
            source: entry.source,
            time: entry.time,
            sealed: true,
        });
        Ok(true)
    }

    pub fn redo(&mut self, project: &mut Project, file: FileId) -> Result<bool, EditFileError> {
        let Some(entry) = self.redo.pop() else {
            return Ok(false);
        };
        let mut steps = Vec::with_capacity(entry.steps.len());
        for step in entry.steps {
            let inverse = project.edit(file, &step.forward)?;
            steps.push(Step {
                forward: step.forward,
                inverse,
            });
        }
        self.undo.push(Entry {
            steps,
            source: entry.source,
            time: entry.time,
            sealed: true,
        });
        Ok(true)
    }
}
