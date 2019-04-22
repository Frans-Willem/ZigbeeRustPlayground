pub mod mac;

#[cfg(test)]
use bytes::IntoBuf;
use bytes::{Buf, BufMut, Bytes};
use std::convert::{TryFrom, TryInto};
use std::result::Result;

use crate::parse_serialize::{
    ParseError, ParseFromBuf, ParseFromBufTagged, ParseResult, SerializeError, SerializeResult,
    SerializeToBuf, SerializeToBufTagged,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShortAddress(pub u16);
default_parse_serialize_newtype!(ShortAddress, u16);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExtendedAddress(pub u64);
default_parse_serialize_newtype!(ExtendedAddress, u64);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PANID(pub u16);
default_parse_serialize_newtype!(PANID, u16);

