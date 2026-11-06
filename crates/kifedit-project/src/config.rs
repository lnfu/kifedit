use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use kifedit_check::Severity;
use kifedit_syntax::FormatOptions;
use serde::Deserialize;

use crate::{FileRole, ProjectError};

pub const CONFIG_FILE_NAME: &str = "kifedit.toml";

pub fn find_config(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .map(|dir| dir.join(CONFIG_FILE_NAME))
        .find(|path| path.is_file())
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub project: ProjectSection,
    #[serde(default)]
    pub files: FilesSection,
    #[serde(default)]
    pub format: FormatSection,
    #[serde(default)]
    pub checks: BTreeMap<String, Severity>,
    #[serde(default)]
    pub vampire: VampireSection,
    #[serde(default)]
    pub tests: TestsSection,
    #[serde(default)]
    pub lean: LeanSection,
}

impl Config {
    pub fn parse(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    pub fn load(path: &Path) -> Result<Self, ProjectError> {
        let text = std::fs::read_to_string(path).map_err(|source| ProjectError::Read {
            path: path.to_owned(),
            source,
        })?;
        let config = Self::parse(&text).map_err(|source| ProjectError::Config {
            path: path.to_owned(),
            source,
        })?;
        // 與 deny_unknown_fields 相同：不認得的規則代碼是設定錯誤
        if let Some(code) = config
            .checks
            .keys()
            .find(|c| kifedit_check::find_rule(c).is_none())
        {
            return Err(ProjectError::UnknownCheck {
                path: path.to_owned(),
                code: code.clone(),
            });
        }
        Ok(config)
    }

    /// 專案的所有檔案，依路徑排序：專案目錄下的 .kif 檔（套用 .kifignore），
    /// 加上 readonly 列出的檔案。readonly 可以在專案目錄外，明確列出時不受 .kifignore 影響
    pub fn files(&self, root: &Path) -> Result<Vec<(FileRole, PathBuf)>, ProjectError> {
        let mut files: BTreeMap<PathBuf, FileRole> = kif_files(root)
            .into_iter()
            .map(|p| (p, FileRole::Editable))
            .collect();
        for path in expand(root, &self.files.readonly)? {
            files.insert(path, FileRole::Library);
        }
        Ok(files.into_iter().map(|(path, role)| (role, path)).collect())
    }
}

fn is_pattern(entry: &str) -> bool {
    entry.contains(['*', '?', '['])
}

// 目錄下所有的 .kif 檔。只套用 .kifignore，不套用 .gitignore：常有專案把 SUMO 放在 .gitignore 中
fn kif_files(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = ignore::WalkBuilder::new(dir)
        .standard_filters(false)
        .hidden(true)
        .add_custom_ignore_filename(".kifignore")
        .build()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
        .map(|e| e.into_path())
        .filter(|p| p.extension().is_some_and(|e| e == "kif"))
        .collect();
    out.sort();
    out
}

// 檔案照原樣；目錄取其下所有 .kif 檔；pattern 只取檔案，比對不到時是錯誤
fn expand(root: &Path, entries: &[PathBuf]) -> Result<Vec<PathBuf>, ProjectError> {
    let mut out: Vec<PathBuf> = Vec::new();
    for entry in entries {
        let text = entry.to_string_lossy();
        if !is_pattern(&text) {
            let path = root.join(entry);
            if path.is_dir() {
                out.extend(kif_files(&path));
            } else {
                out.push(path);
            }
            continue;
        }
        let bad = |message: String| ProjectError::BadPattern {
            pattern: text.to_string(),
            message,
        };
        let full = if entry.is_absolute() {
            text.to_string()
        } else {
            format!("{}/{text}", glob::Pattern::escape(&root.to_string_lossy()))
        };
        let matched: Vec<PathBuf> = glob::glob(&full)
            .map_err(|e| bad(e.to_string()))?
            .filter_map(Result::ok)
            .filter(|p| p.is_file())
            .collect();
        if matched.is_empty() {
            return Err(ProjectError::NoMatch {
                pattern: text.to_string(),
            });
        }
        out.extend(matched);
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectSection {
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FilesSection {
    pub readonly: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FormatSection {
    pub indent: usize,
    pub line_width: usize,
}

impl Default for FormatSection {
    fn default() -> Self {
        let options = FormatOptions::default();
        Self {
            indent: options.indent,
            line_width: options.line_width,
        }
    }
}

impl FormatSection {
    pub fn options(&self) -> FormatOptions {
        FormatOptions {
            line_width: self.line_width,
            indent: self.indent,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Translation {
    Fof,
    #[default]
    Tff,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VampireSection {
    pub path: String,
    pub time_limit: u64,
    pub memory_limit: u64,
    pub max_parallel: usize,
    pub auto_consistency: bool,
    pub translation: Translation,
}

impl Default for VampireSection {
    fn default() -> Self {
        Self {
            path: String::new(),
            time_limit: 30,
            memory_limit: 4096,
            max_parallel: 2,
            auto_consistency: false,
            translation: Translation::Tff,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TestsSection {
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LeanSection {
    pub enabled: bool,
    pub toolchain: String,
    pub mathlib: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = r#"
[project]
name = "stretch4-lab"

[files]
readonly = ["sumo", "ieee1872_terms.kif"]

[format]
indent = 2
line_width = 100

[checks]
W002 = "off"        # 調整單一規則嚴重度: error / warning / info / off

[vampire]
path = ""           # 空白：依序找 PATH, 再用內建版本
time_limit = 30     # 秒
memory_limit = 4096 # MB
max_parallel = 2
auto_consistency = false
translation = "tff" # fof / tff

[tests]
paths = ["tests/*.tq"]

[lean]
enabled = false
toolchain = ""
mathlib = false
"#;

    #[test]
    fn parses_the_example_in_the_design_document() {
        let config = Config::parse(EXAMPLE).unwrap();
        assert_eq!(config.project.name, "stretch4-lab");
        assert_eq!(
            config.files.readonly,
            [PathBuf::from("sumo"), PathBuf::from("ieee1872_terms.kif")]
        );
        assert_eq!(config.checks["W002"], Severity::Off);
        assert_eq!(config.vampire.translation, Translation::Tff);
        assert_eq!(config.tests.paths, ["tests/*.tq"]);
    }

    #[test]
    fn missing_sections_use_defaults() {
        let config = Config::parse("[project]\nname = \"x\"\n").unwrap();
        assert!(config.files.readonly.is_empty());
        assert_eq!(config.format, FormatSection::default());
        assert_eq!(config.format.options(), FormatOptions::default());
        assert_eq!(config.vampire.time_limit, 30);
    }

    #[test]
    fn unknown_key_is_an_error() {
        let error =
            Config::parse("[project]\nname = \"x\"\n[format]\nline_widht = 80\n").unwrap_err();
        assert!(error.to_string().contains("line_widht"), "{error}");
    }

    #[test]
    fn project_name_is_required() {
        assert!(Config::parse("[files]\nreadonly = [\"a.kif\"]\n").is_err());
    }
}
