pub trait AsGeneralLifetime<'a> {
    type GeneralLifetimeType: 'a;
}

impl<'a, 'b: 'a, T: 'a> AsGeneralLifetime<'a> for &'b T {
    type GeneralLifetimeType = &'a T;
}

impl<'a, 'b: 'a, T: 'a> AsGeneralLifetime<'a> for &'b mut T {
    type GeneralLifetimeType = &'a mut T;
}
