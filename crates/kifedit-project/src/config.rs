use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use kifedit_check::Severity;
use kifedit_syntax::FormatOptions;
use serde::Deserialize;

use crate::ProjectError;

pub const CONFIG_FILE_NAME: &str = "project.toml";

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
        if let Some(code) = config.checks.keys().find(|c| kifedit_check::find_rule(c).is_none()) {
            return Err(ProjectError::UnknownCheck {
                path: path.to_owned(),
                code: code.clone(),
            });
        }
        Ok(config)
    }

    pub fn editable_paths(&self, root: &Path) -> Vec<PathBuf> {
        self.files.editable.iter().map(|p| root.join(p)).collect()
    }

    pub fn library_paths(&self, root: &Path) -> Vec<PathBuf> {
        self.files.library.iter().map(|p| root.join(p)).collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectSection {
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FilesSection {
    pub library: Vec<PathBuf>,
    pub editable: Vec<PathBuf>,
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
library  = ["sumo/Merge.kif", "sumo/Mid-level-ontology.kif", "ieee1872_terms.kif"]
editable = ["stretch4_lab.kif"]

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
        assert_eq!(config.files.library.len(), 3);
        assert_eq!(config.files.editable, [PathBuf::from("stretch4_lab.kif")]);
        assert_eq!(config.checks["W002"], Severity::Off);
        assert_eq!(config.vampire.translation, Translation::Tff);
        assert_eq!(config.tests.paths, ["tests/*.tq"]);
    }

    #[test]
    fn missing_sections_use_defaults() {
        let config = Config::parse("[project]\nname = \"x\"\n").unwrap();
        assert!(config.files.editable.is_empty());
        assert_eq!(config.format, FormatSection::default());
        assert_eq!(config.format.options(), FormatOptions::default());
        assert_eq!(config.vampire.time_limit, 30);
    }

    #[test]
    fn unknown_key_is_an_error() {
        let error = Config::parse("[project]\nname = \"x\"\n[format]\nline_widht = 80\n").unwrap_err();
        assert!(error.to_string().contains("line_widht"), "{error}");
    }

    #[test]
    fn project_name_is_required() {
        assert!(Config::parse("[files]\neditable = [\"a.kif\"]\n").is_err());
    }

    #[test]
    fn paths_are_relative_to_the_project_directory() {
        let config = Config::parse("[project]\nname = \"x\"\n[files]\neditable = [\"kb/a.kif\"]\n").unwrap();
        assert_eq!(
            config.editable_paths(Path::new("/lab")),
            [PathBuf::from("/lab/kb/a.kif")]
        );
    }
}
