#[derive(PartialEq, Eq, Debug, Copy, Clone)]
pub enum ExtEnumError {
    InvalidTag,
}

pub trait ExtEnum: Sized {
    type Tag;
    fn into_tag(&self) -> Self::Tag;
    fn try_from_tag(value: Self::Tag) -> Result<Self, ExtEnumError>;
}
