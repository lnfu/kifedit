use std::path::{Path, PathBuf};

/// SUMO 的根目錄 data/sumo
pub fn sumo_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/sumo");
    assert!(
        root.is_dir(),
        "Cannot find the directory {}. Clone the SUMO repository into this directory.",
        root.display()
    );
    root
}

/// 遞迴列出 dir 底下所有 .kif 檔，依路徑排序
pub fn kif_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect(dir, &mut out);
    out.sort();
    out
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|e| e == "kif") {
            out.push(path);
        }
    }
}
