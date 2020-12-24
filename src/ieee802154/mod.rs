pub mod mac;

use crate::pack::Pack;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Pack)]
pub struct ShortAddress(pub u16);

impl std::fmt::Debug for ShortAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("ShortAddress({:#4X})", self.0))
    }
}

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

#[derive(Clone, Copy, PartialEq, Eq, Hash, Pack)]
pub struct ExtendedAddress(pub u64);

impl std::fmt::Debug for ExtendedAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("ExtendedAddress({:#16X})", self.0))
    }
}

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

#[derive(Clone, Copy, PartialEq, Eq, Hash, Pack)]
pub struct PANID(pub u16);

impl std::fmt::Debug for PANID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("PANID({:#4X})", self.0))
    }
}

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
