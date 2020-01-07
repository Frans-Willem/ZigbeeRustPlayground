pub enum ExtEnumError {
    InvalidTag,
}

pub trait ExtEnum: Sized {
    type TagType;
    fn into_tag(self) -> Self::TagType;
    fn try_from_tag(value: Self::TagType) -> Result<Self, ExtEnumError>;
}
