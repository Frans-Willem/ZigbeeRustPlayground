use crate::pack::{PackError, PackTarget};

pub struct SlicePackTarget<'lifetime>(&'lifetime mut [u8], usize);

#[derive(Debug, PartialEq, Eq)]
pub enum SlicePackError {
    NotEnoughSpace,
}

impl Into<PackError<SlicePackError>> for SlicePackError {
    fn into(self) -> PackError<SlicePackError> {
        PackError::TargetError(self)
    }
}
impl<'lifetime> PackTarget for SlicePackTarget<'lifetime> {
    type Error = SlicePackError;
    fn append(self, data: &[u8]) -> Result<Self, Self::Error> {
        let SlicePackTarget(target, offset) = self;
        if offset + data.len() > target.len() {
            Err(Self::Error::NotEnoughSpace)
        } else {
            target[offset..offset + data.len()].copy_from_slice(data);
            Ok(SlicePackTarget(target, offset + data.len()))
        }
    }
}

impl<'lifetime> SlicePackTarget<'lifetime> {
    pub fn new(target: &'lifetime mut [u8]) -> Self {
        SlicePackTarget(target, 0)
    }
}

impl<'lifetime> From<&'lifetime mut [u8]> for SlicePackTarget<'lifetime> {
    fn from(target: &'lifetime mut [u8]) -> SlicePackTarget<'lifetime> {
        SlicePackTarget::new(target)
    }
}

impl<'lifetime> Into<usize> for SlicePackTarget<'lifetime> {
    fn into(self) -> usize {
        self.1
    }
}
