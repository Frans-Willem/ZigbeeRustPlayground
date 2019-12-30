use snowflake::ProcessUniqueId;

/**
 * I wrapped this functionality such that I can easily swap it out with something else,
 * e.g. a smaller integer, on smaller systems
 */

#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy)]
pub struct UniqueKey(ProcessUniqueId);

impl UniqueKey {
    pub fn new() -> UniqueKey {
        UniqueKey(ProcessUniqueId::new())
    }
}
