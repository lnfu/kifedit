use kifedit_project::Project;
use kifedit_reason::Hierarchy;
use kifedit_sema::Symbol;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct TreeParams {
    pub view: String,
    pub node: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeNode {
    name: String,
    kind: &'static str, // class、member
    has_children: bool,
    other_parents: usize,
}

/// 展開時才查詢子節點。node 為 None 時回傳根節點
pub fn children(project: &Project, params: &TreeParams) -> Vec<TreeNode> {
    let taxonomy = project.taxonomy();
    let node = params.node.as_deref();
    match params.view.as_str() {
        "classes" => taxonomy_children(project, "Entity", None, node),
        "relations" => taxonomy_children(project, "Relation", Some(&taxonomy.subrelation), node),
        "attributes" => {
            taxonomy_children(project, "Attribute", Some(&taxonomy.sub_attribute), node)
        }
        _ => Vec::new(),
    }
}

// 類別以 subclass 展開，類別下列出實例。members 不為 None 時，實例（關係或屬性）再以該階層展開
fn taxonomy_children(
    project: &Project,
    root: &str,
    members: Option<&Hierarchy>,
    node: Option<&str>,
) -> Vec<TreeNode> {
    let interner = project.interner();
    let taxonomy = project.taxonomy();
    let classes = &taxonomy.subclass;
    let is_class = |s: Symbol| classes.contains(s) || !taxonomy.instances_of(s).is_empty();

    let (mut class_list, mut member_list) = match node {
        None => match interner.get(root).filter(|&s| classes.contains(s)) {
            Some(root) => (vec![root], Vec::new()),
            None => (
                classes
                    .nodes()
                    .filter(|&s| classes.parents(s).is_empty())
                    .collect(),
                Vec::new(),
            ),
        },
        Some(name) => match interner.get(name) {
            Some(s) if is_class(s) => (
                classes.children(s).to_vec(),
                taxonomy.instances_of(s).to_vec(),
            ),
            Some(s) => (
                Vec::new(),
                members.map(|h| h.children(s).to_vec()).unwrap_or_default(),
            ),
            None => (Vec::new(), Vec::new()),
        },
    };
    class_list.sort_by_key(|&s| interner.resolve(s));
    member_list.sort_by_key(|&s| interner.resolve(s));

    let term_node = |s: Symbol, kind, has_children, other_parents| TreeNode {
        name: interner.resolve(s).to_string(),
        kind,
        has_children,
        other_parents,
    };
    let class_nodes = class_list.into_iter().map(|s| {
        let has_children = !classes.children(s).is_empty() || !taxonomy.instances_of(s).is_empty();
        term_node(
            s,
            "class",
            has_children,
            classes.parents(s).len().saturating_sub(1),
        )
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
