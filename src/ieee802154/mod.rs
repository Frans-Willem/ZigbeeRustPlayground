pub mod mac;

use crate::pack::Pack;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Pack)]
pub struct ShortAddress(pub u16);

impl From<u16> for ShortAddress {
    fn from(x: u16) -> Self {
        Self(x)
    }
}

impl Into<u16> for ShortAddress {
    fn into(self) -> u16 {
        self.0
    }
}

impl ShortAddress {
    pub fn broadcast() -> Self {
        ShortAddress(0xFFFF)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Pack)]
pub struct ExtendedAddress(pub u64);

impl From<u64> for ExtendedAddress {
    fn from(x: u64) -> Self {
        Self(x)
    }
}

impl Into<u64> for ExtendedAddress {
    fn into(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Pack)]
pub struct PANID(pub u16);

impl From<u16> for PANID {
    fn from(x: u16) -> Self {
        Self(x)
    }
}

impl Into<u16> for PANID {
    fn into(self) -> u16 {
        self.0
    }
}

impl PANID {
    pub fn broadcast() -> Self {
        PANID(0xFFFF)
    }
}
