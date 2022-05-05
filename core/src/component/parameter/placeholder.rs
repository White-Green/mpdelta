use uuid::Uuid;

#[derive(Debug, Clone, Copy, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct ImagePlaceholder {
    id: Uuid,
}

#[derive(Debug, Clone, Copy, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct AudioPlaceholder {
    id: Uuid,
}
