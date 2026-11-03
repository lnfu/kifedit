use std::path::Path;
use std::time::Instant;

use kifedit_reason::Taxonomy;
use kifedit_sema::{FileId, Index, Interner};
use kifedit_syntax::parser::parse;

#[test]
fn taxonomy_of_sumo() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/sumo");
    let mut paths: Vec<_> = std::fs::read_dir(&root)
        .unwrap_or_else(|e| panic!("Cannot read the directory {}: {e}", root.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "kif"))
        .collect();
    paths.sort();

    let interner = Interner::default();
    let mut index = Index::default();
    for (i, path) in paths.iter().enumerate() {
        let src = std::fs::read_to_string(path).unwrap();
        index.set_file(FileId(i as u32), &parse(&src).syntax(), &interner);
    }

    let start = Instant::now();
    let taxonomy = Taxonomy::build(&index, &interner);
    let elapsed = start.elapsed();

    let name = |s| interner.get(s).unwrap();
    let camera = name("Camera");
    for ancestor in ["Device", "Artifact", "Entity"] {
        assert!(
            taxonomy.subclass.is_ancestor(name(ancestor), camera),
            "{ancestor} is not an ancestor of Camera."
        );
    }

    let type_pairs: usize = taxonomy
        .individuals()
        .map(|x| taxonomy.types_of(x).len())
        .sum();
    eprintln!("Build time: {elapsed:?}.");
    eprintln!(
        "Classes: {}. Ancestor pairs: {}. Subclass cycles: {}.",
        taxonomy.subclass.len(),
        taxonomy.subclass.ancestor_pairs(),
        taxonomy.subclass.cycles().len()
    );
    eprintln!(
        "Individuals: {}. Instance-of pairs after closure: {type_pairs}.",
        taxonomy.individuals().count()
    );
    eprintln!(
        "Relations in subrelation: {}. Attributes in subAttribute: {}.",
        taxonomy.subrelation.len(),
        taxonomy.sub_attribute.len()
    );
    for cycle in taxonomy.subclass.cycles() {
        let names: Vec<_> = cycle.iter().map(|&s| interner.resolve(s)).collect();
        eprintln!("  Cycle: {}", names.join(" -> "));
    }
}
