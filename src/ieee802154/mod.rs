pub mod mac;

/*
use crate::parse_serialize::{
    ParseFromBuf, ParseFromBufTagged, ParseResult, SerializeResult,
    SerializeToBuf,
};
*/

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShortAddress(pub u16);
default_parse_serialize_newtype!(ShortAddress, u16);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExtendedAddress(pub u64);
default_parse_serialize_newtype!(ExtendedAddress, u64);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PANID(pub u16);
default_parse_serialize_newtype!(PANID, u16);
