pub trait PackTarget: Sized {
    type Error;
    fn append(self, data: &[u8]) -> Result<Self, Self::Error>;
}
