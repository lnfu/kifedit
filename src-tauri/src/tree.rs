use std::collections::BTreeMap;

use kifedit_project::{FileRole, Project};
use kifedit_reason::Hierarchy;
use kifedit_sema::{FileId, Symbol, Term};
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeNode {
    id: String,
    label: String,
    kind: &'static str, // class、member、file、group、formula
    has_children: bool,
    other_parents: usize,
    term: Option<String>,       // 點選後開啟的詞彙頁
    location: Option<Location>, // 點選後在編輯器中開啟的位置
}

#[derive(Serialize)]
pub struct Location {
    file: u32,
    line: usize,
}

/// 樹狀介面的子節點，展開時才查詢（§8.1）。node 為 None 時回傳根節點
pub fn children(project: &Project, view: &str, node: Option<&str>) -> Result<Vec<TreeNode>, String> {
    let taxonomy = project.taxonomy();
    match view {
        "classes" => Ok(taxonomy_children(project, "Entity", None, node)),
        "relations" => Ok(taxonomy_children(project, "Relation", Some(&taxonomy.subrelation), node)),
        "attributes" => Ok(taxonomy_children(project, "Attribute", Some(&taxonomy.sub_attribute), node)),
        "files" => Ok(file_children(project, node)),
        _ => Err(format!("The tree view {view:?} is not available.")),
    }
}

// 類別以 subclass 展開，類別下列出實例。members 不為 None 時，實例（關係或屬性）再以該階層展開
fn taxonomy_children(project: &Project, root: &str, members: Option<&Hierarchy>, node: Option<&str>) -> Vec<TreeNode> {
    let interner = project.interner();
    let taxonomy = project.taxonomy();
    let classes = &taxonomy.subclass;
    let is_class = |s: Symbol| classes.contains(s) || !taxonomy.instances_of(s).is_empty();

    let (mut class_list, mut member_list) = match node {
        None => match interner.get(root).filter(|&s| classes.contains(s)) {
            Some(root) => (vec![root], Vec::new()),
            None => (
                classes.nodes().filter(|&s| classes.parents(s).is_empty()).collect(),
                Vec::new(),
            ),
        },
        Some(name) => match interner.get(name) {
            Some(s) if is_class(s) => (classes.children(s).to_vec(), taxonomy.instances_of(s).to_vec()),
            Some(s) => (Vec::new(), members.map(|h| h.children(s).to_vec()).unwrap_or_default()),
            None => (Vec::new(), Vec::new()),
        },
    };
    class_list.sort_by_key(|&s| interner.resolve(s));
    member_list.sort_by_key(|&s| interner.resolve(s));

    let term_node = |s: Symbol, kind, has_children, other_parents| {
        let name = interner.resolve(s).to_string();
        TreeNode {
            id: name.clone(),
            label: name.clone(),
            kind,
            has_children,
            other_parents,
            term: Some(name),
            location: None,
        }
    };
    let class_nodes = class_list.into_iter().map(|s| {
        let has_children = !classes.children(s).is_empty() || !taxonomy.instances_of(s).is_empty();
        term_node(s, "class", has_children, classes.parents(s).len().saturating_sub(1))
    });
    let member_nodes = member_list.into_iter().map(|s| {
        let has_children = members.is_some_and(|h| !h.children(s).is_empty());
        let parents = match members.filter(|h| h.contains(s)) {
            Some(h) => h.parents(s).len(),
            None => taxonomy.instance_of(s).len(),
        };
        term_node(s, "member", has_children, parents.saturating_sub(1))
    });
    class_nodes.chain(member_nodes).collect()
}

// 檔案 → 依關係分組 → 頂層公式；規則（=>、and 等）歸為一組
fn file_children(project: &Project, node: Option<&str>) -> Vec<TreeNode> {
    let Some(node) = node else {
        return project
            .files()
            .iter()
            .map(|file| {
                let formulas = project.index().file(file.id).map_or(0, |f| f.formulas().len());
                let role = match file.role {
                    FileRole::Library => "library",
                    FileRole::Editable => "editable",
                };
                let name = file.path.file_name().map_or_else(
                    || file.path.display().to_string(),
                    |n| n.to_string_lossy().into_owned(),
                );
                TreeNode {
                    id: format!("file:{}", file.id.0),
                    label: format!("{name} ({role}, {formulas} formulas)"),
                    kind: "file",
                    has_children: formulas > 0,
                    other_parents: 0,
                    term: None,
                    location: Some(Location {
                        file: file.id.0,
                        line: 1,
                    }),
                }
            })
            .collect();
    };

    let mut parts = node.splitn(3, ':');
    let (Some(kind), Some(id)) = (parts.next(), parts.next().and_then(|s| s.parse::<u32>().ok())) else {
        return Vec::new();
    };
    let file = FileId(id);
    let Some(file_index) = project.index().file(file) else {
        return Vec::new();
    };
    let formulas = file_index.formulas();
    let interner = project.interner();
    let group_of = |term: &Term| -> String {
        match term {
            Term::App(head, _) => match head.as_ref() {
                Term::Symbol(s) => interner.resolve(*s).to_string(),
                _ => "(other)".to_string(),
            },
            Term::Logic(..) | Term::Quant(..) => "(rules)".to_string(),
            _ => "(other)".to_string(),
        }
    };

    match (kind, parts.next()) {
        ("file", None) => {
            let mut groups: BTreeMap<String, usize> = BTreeMap::new();
            for formula in formulas {
                *groups.entry(group_of(&formula.term)).or_default() += 1;
            }
            groups
                .into_iter()
                .map(|(group, count)| TreeNode {
                    id: format!("group:{id}:{group}"),
                    label: format!("{group} ({count})"),
                    kind: "group",
                    has_children: true,
                    other_parents: 0,
                    term: None,
                    location: None,
                })
                .collect()
        }
        ("group", Some(group)) => {
            let text = project.file(file).map_or("", |f| f.document.text());
            formulas
                .iter()
                .filter(|f| group_of(&f.term) == group)
                .map(|f| {
                    let start = usize::from(f.range.start());
                    let end = usize::from(f.range.end());
                    let line = text[..start].matches('\n').count() + 1;
                    let first_line = text[start..end].lines().next().unwrap_or("");
                    TreeNode {
                        id: format!("formula:{id}:{}", f.id.index),
                        label: format!("{line}: {first_line}"),
                        kind: "formula",
                        has_children: false,
                        other_parents: 0,
                        term: None,
                        location: Some(Location { file: id, line }),
                    }
                })
                .collect()
        }
        _ => Vec::new(),
    }
}
