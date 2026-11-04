mod hierarchy;
mod queries;
mod query;
mod taxonomy;

pub use hierarchy::Hierarchy;
pub use query::{Answer, NeedsProver, QueryResult, evaluate};
pub use queries::{project_taxonomy, taxonomy_facts};
pub use taxonomy::{Kinds, Taxonomy, TaxonomyFacts};
