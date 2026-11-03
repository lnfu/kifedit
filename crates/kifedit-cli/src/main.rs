use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use kifedit_project::{Config, find_config};
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

        /// The files to format. If you do not give files, kifedit formats the editable files in project.toml.
        files: Vec<PathBuf>,
    },
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
    }
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
        config.editable_paths(root)
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
