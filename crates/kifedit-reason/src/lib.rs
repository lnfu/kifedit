mod hierarchy;
mod query;
mod taxonomy;

pub use hierarchy::Hierarchy;
pub use query::{Answer, NeedsProver, QueryResult, evaluate};
pub use taxonomy::{Kinds, Taxonomy};
