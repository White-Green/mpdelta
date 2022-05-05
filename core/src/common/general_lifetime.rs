pub trait AsGeneralLifetime<'a> {
    type GeneralLifetimeType: 'a;
    fn into_general_lifetime(self) -> Self::GeneralLifetimeType;
}

impl<'a, 'b: 'a, T: 'a> AsGeneralLifetime<'a> for &'b T {
    type GeneralLifetimeType = &'a T;

    fn into_general_lifetime(self) -> Self::GeneralLifetimeType {
        self
    }
}

impl<'a, 'b: 'a, T: 'a> AsGeneralLifetime<'a> for &'b mut T {
    type GeneralLifetimeType = &'a mut T;

    fn into_general_lifetime(self) -> Self::GeneralLifetimeType {
        self
    }
}
