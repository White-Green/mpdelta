use uuid::Uuid;

#[derive(Debug, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct ImagePlaceholder {
    id: Uuid,
}

#[derive(Debug, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct AudioPlaceholder {
    id: Uuid,
}
