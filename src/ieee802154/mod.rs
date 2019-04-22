pub mod mac;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShortAddress(pub u16);
default_parse_serialize_newtype!(ShortAddress, u16);

impl ShortAddress {
    pub fn as_u16(&self) -> u16 {
        let ShortAddress(retval) = self;
        *retval
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExtendedAddress(pub u64);
default_parse_serialize_newtype!(ExtendedAddress, u64);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PANID(pub u16);
default_parse_serialize_newtype!(PANID, u16);

impl PANID {
    pub fn as_u16(&self) -> u16 {
        let PANID(retval) = self;
        *retval
    }
}
