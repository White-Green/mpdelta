use uuid::Uuid;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Project {
    id: Uuid,
    /* TODO */
}

impl Project {
    pub(crate) fn new_empty(id: Uuid) -> Project {
        Project { id }
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RootComponentClass {
    id: Uuid,
    /* TODO */
}

impl RootComponentClass {
    pub(crate) fn new_empty(id: Uuid) -> RootComponentClass {
        RootComponentClass { id }
    }
}
