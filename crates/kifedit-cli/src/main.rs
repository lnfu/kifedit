use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use kifedit_project::{Config, FileRole, Project, Severity, find_config};
use serde::Serialize;
use kifedit_syntax::parser::parse;
use kifedit_syntax::{FormatOptions, format_file};

#[derive(Parser)]
#[command(name = "kifedit", version, about = "SUO-KIF ontology editor")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Format SUO-KIF files.
    Fmt {
        /// Do not write the files. Show the files that are not formatted.
        #[arg(long)]
        check: bool,

        /// The files to format. If you do not give files, kifedit formats the project files that are not read-only.
        files: Vec<PathBuf>,
    },
    /// Check the project in project.toml for problems.
    Check {
        /// Also show the problems in read-only files.
        #[arg(long)]
        all: bool,

        /// The output format.
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum OutputFormat {
    /// One line for each problem: path:line:column: severity[code]: message
    Text,
    /// One JSON object on each line.
    Json,
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Fmt { check, files } => match fmt_targets(files) {
            Ok((files, options)) => fmt(&files, &options, check),
            Err(message) => {
                eprintln!("{message}");
                ExitCode::from(2)
            }
        },
        Command::Check { all, format } => match check(all, format) {
            Ok(code) => code,
            Err(message) => {
                eprintln!("{message}");
                ExitCode::from(2)
            }
        },
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonDiagnostic<'a> {
    path: &'a str,
    line: usize,
    column: usize,
    end_line: usize,
    end_column: usize,
    severity: &'static str,
    code: &'static str,
    message: &'a str,
}

// 結束碼同 fmt：0 沒有錯誤，1 有錯誤，2 讀檔或設定失敗
fn check(all: bool, format: OutputFormat) -> Result<ExitCode, String> {
    let cwd = std::env::current_dir()
        .map_err(|e| format!("Cannot read the current directory: {e}"))?;
    let config = find_config(&cwd).ok_or(
        "Cannot find project.toml in this directory or in a parent directory.",
    )?;
    let project = Project::open(&config).map_err(|e| e.to_string())?;
    project.check_all();

    let (mut errors, mut warnings, mut infos) = (0, 0, 0);
    for file in project.files() {
        if !all && file.role == FileRole::Library {
            continue;
        }
        let text = project.text(file.id).unwrap_or_default();
        let path = file.path.strip_prefix(project.root()).unwrap_or(&file.path);
        let path = path.display().to_string();
        for d in project.diagnostics(file.id) {
            let severity = severity_name(d.severity);
            match d.severity {
                Severity::Error => errors += 1,
                Severity::Warning => warnings += 1,
                Severity::Info | Severity::Off => infos += 1,
            }
            let (line, column) = position(text, d.range.start().into());
            match format {
                OutputFormat::Text => {
                    println!("{path}:{line}:{column}: {severity}[{}]: {}", d.code, d.message);
                }
                OutputFormat::Json => {
                    let (end_line, end_column) = position(text, d.range.end().into());
                    let json = JsonDiagnostic {
                        path: &path,
                        line,
                        column,
                        end_line,
                        end_column,
                        severity,
                        code: d.code,
                        message: &d.message,
                    };
                    println!("{}", serde_json::to_string(&json).expect("a diagnostic can be written as JSON"));
                }
            }
        }
    }
    eprintln!("Errors: {errors}. Warnings: {warnings}. Information: {infos}.");
    Ok(if errors > 0 { ExitCode::from(1) } else { ExitCode::SUCCESS })
}

fn severity_name(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "info",
        Severity::Off => "off",
    }
}

/// 行與欄都從 1 開始；欄以字元計算，與 rustc 相同
fn position(text: &str, offset: usize) -> (usize, usize) {
    let before = &text[..offset.min(text.len())];
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().unwrap_or("").chars().count() + 1;
    (line, column)
}

// 與 cargo 相同：從目前目錄往上找 project.toml；找到就用它的 [format]
fn fmt_targets(files: Vec<PathBuf>) -> Result<(Vec<PathBuf>, FormatOptions), String> {
    let cwd = std::env::current_dir()
        .map_err(|e| format!("Cannot read the current directory: {e}"))?;
    let Some(config_path) = find_config(&cwd) else {
        if files.is_empty() {
            return Err(
                "Cannot find project.toml in this directory or in a parent directory. Give the files to format."
                    .to_string(),
            );
        }
        return Ok((files, FormatOptions::default()));
    };
    let config = Config::load(&config_path).map_err(|e| e.to_string())?;
    let root = config_path.parent().unwrap_or(Path::new("."));
    let files = if files.is_empty() {
        config
            .files(root)
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|(role, _)| *role == FileRole::Editable)
            .map(|(_, path)| path)
            .collect()
    } else {
        files
    };
    Ok((files, config.format.options()))
}

// 結束碼與 rustfmt --check 相同：0 全部已格式化，1 有檔案需要格式化，2 讀寫失敗
fn fmt(files: &[PathBuf], options: &FormatOptions, check: bool) -> ExitCode {
    let mut unformatted = 0;
    let mut failed = false;

    for path in files {
        let src = match std::fs::read_to_string(path) {
            Ok(src) => src,
            Err(e) => {
                eprintln!("{}: Cannot read the file: {e}", path.display());
                failed = true;
                continue;
            }
        };
        let out = format_file(&parse(&src).syntax(), options);
        if out == src {
            continue;
        }
        if check {
            println!("{}: The file is not formatted.", path.display());
            unformatted += 1;
        } else if let Err(e) = std::fs::write(path, &out) {
            eprintln!("{}: Cannot write the file: {e}", path.display());
            failed = true;
        }
    }

    if unformatted > 0 {
        eprintln!(
            "Files that are not formatted: {unformatted} of {}. Use `kifedit fmt` to format them.",
            files.len()
        );
    }
    if failed {
        ExitCode::from(2)
    } else if unformatted > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
