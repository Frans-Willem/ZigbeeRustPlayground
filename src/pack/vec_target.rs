use crate::pack::PackTarget;

pub struct VecPackTarget(Vec<u8>);

#[derive(Debug, PartialEq, Eq)]
pub enum VecPackError {}

impl PackTarget for VecPackTarget {
    type Error = VecPackError;
    fn append(self, data: &[u8]) -> Result<Self, Self::Error> {
        let VecPackTarget(mut vec) = self;
        vec.extend_from_slice(data);
        Ok(VecPackTarget(vec))
    }
}

impl VecPackTarget {
    pub fn new() -> VecPackTarget {
        VecPackTarget(Vec::new())
    }
}

impl Into<Vec<u8>> for VecPackTarget {
    fn into(self) -> Vec<u8> {
        self.0
    }
}
