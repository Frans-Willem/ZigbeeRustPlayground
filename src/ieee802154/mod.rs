pub mod mac;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ShortAddress(pub u16);
default_serialization_newtype!(ShortAddress, u16);

impl ShortAddress {
    pub fn as_u16(&self) -> u16 {
        let ShortAddress(retval) = self;
        *retval
    }

    pub fn broadcast() -> Self {
        ShortAddress(0xFFFF)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExtendedAddress(pub u64);
default_serialization_newtype!(ExtendedAddress, u64);

impl ExtendedAddress {
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PANID(pub u16);
default_serialization_newtype!(PANID, u16);

impl PANID {
    pub fn as_u16(&self) -> u16 {
        let PANID(retval) = self;
        *retval
    }

    pub fn broadcast() -> Self {
        PANID(0xFFFF)
    }
}
