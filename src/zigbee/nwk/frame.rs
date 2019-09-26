use crate::ieee802154::{ExtendedAddress, ShortAddress};
use crate::parse_serialize::Error as ParseError;
use crate::parse_serialize::Result as ParseResult;
#[cfg(test)]
use crate::parse_serialize::SerializeToBufEx;
use crate::parse_serialize::{
    ParseFromBuf, ParseFromBufTagged, SerializeToBuf, SerializeToBufTagged,
};
use bitfield::bitfield;
use bytes::{Buf, BufMut, Bytes};
use std::convert::{TryFrom, TryInto};

pub enum Command {}

pub enum FrameType {
    Data(Bytes),
    Command(Command),
    InterPAN(Bytes),
}

#[derive(Clone, Copy, TryFromPrimitive)]
#[TryFromPrimitiveType = "u16"]
pub enum DiscoverRoute {
    SupressRouteDiscovery = 0,
    EnableRouteDiscovery = 1,
}

pub struct SourceRoute {
    relay_index: u8,
    relay_list: Vec<ShortAddress>,
}

pub struct Frame {
    frame_type: FrameType,
    protocol_version: u8,
    destination: ShortAddress,
    source: ShortAddress,
    radius: u8,
    sequence_number: u8,
    discover_route: DiscoverRoute,
    destination_ext: Option<ExtendedAddress>,
    source_ext: Option<ExtendedAddress>,
    // TODO: Multicast control
    source_route: Option<SourceRoute>,
    // TODO: Security
}

/*=== Bitfields for serializing & parsing ===*/
bitfield! {
    pub struct FrameControl(u16);
    impl Debug;
    pub frame_type, set_frame_type: 1, 0;
    pub protocol_version, set_protocol_version: 5, 2;
    pub discover_route, set_discover_route: 7, 6;
    pub multicast_flag, set_multicast_flag: 8, 8;
    pub security, set_security: 9, 9;
    pub source_route, set_source_route: 10, 10;
    pub destination_ieee_address, set_destination_ieee_address: 11, 11;
    pub source_ieee_address, set_source_ieee_address: 12, 12;
    pub reserved, set_reserved: 15, 13;
}
default_parse_serialize_newtype!(FrameControl, u16);

impl SerializeToBuf for Frame {
    fn serialize_to_buf(&self, buf: &mut BufMut) -> ParseResult<()> {
        let mut fsf = FrameControl(0);
        fsf.set_frame_type(self.frame_type.get_serialize_tag()?);
        if self.protocol_version > 15 {
            return Err(ParseError::UnexpectedData);
        }
        fsf.set_protocol_version(self.protocol_version.into());
        fsf.set_discover_route(self.discover_route as u16);
        fsf.set_multicast_flag(0); // TODO
        fsf.set_security(0); // TODO
        fsf.set_source_route(self.source_route.is_some().into());
        fsf.set_destination_ieee_address(self.destination_ext.is_some().into());
        fsf.set_source_ieee_address(self.source_ext.is_some().into());
        fsf.set_reserved(0);
        fsf.serialize_to_buf(buf)?;
        self.destination.serialize_to_buf(buf)?;
        self.source.serialize_to_buf(buf)?;
        self.radius.serialize_to_buf(buf)?;
        self.sequence_number.serialize_to_buf(buf)?;
        if let Some(destination_ext) = self.destination_ext.as_ref() {
            destination_ext.serialize_to_buf(buf)?;
        }
        if let Some(source_ext) = self.source_ext.as_ref() {
            source_ext.serialize_to_buf(buf)?;
        }
        if let Some(source_route) = self.source_route.as_ref() {
            source_route.serialize_to_buf(buf)?;
        }
        // TODO: Rest may be encrypted.
        self.frame_type.serialize_to_buf(buf)
    }
}
impl ParseFromBuf for Frame {
    fn parse_from_buf(buf: &mut Buf) -> Result<Self, ParseError> {
        let fsf = FrameControl::parse_from_buf(buf)?;
        let protocol_version = fsf.protocol_version();
        let discover_route: DiscoverRoute = fsf.discover_route().try_into()?;
        if fsf.multicast_flag() != 0 {
            return Err(ParseError::Unimplemented("Multicast not yet supported"));
        }
        if fsf.security() != 0 {
            return Err(ParseError::Unimplemented("Security not yet supported"));
        }
        if fsf.reserved() != 0 {
            return Err(ParseError::Unimplemented("Reserved was not 0"));
        }
        let destination = ShortAddress::parse_from_buf(buf)?;
        let source = ShortAddress::parse_from_buf(buf)?;
        let radius = u8::parse_from_buf(buf)?;
        let sequence_number = u8::parse_from_buf(buf)?;
        unimplemented!()
    }
}

impl SerializeToBuf for FrameType {
    fn serialize_to_buf(&self, buf: &mut BufMut) -> Result<(), ParseError> {
        match self {
            FrameType::Data(payload) => payload.serialize_to_buf(buf),
            _ => unimplemented!(),
        }
    }
}
impl SerializeToBufTagged<u16> for FrameType {
    fn get_serialize_tag(&self) -> Result<u16, ParseError> {
        match self {
            FrameType::Data(_) => Ok(0),
            FrameType::Command(_) => Ok(1),
            FrameType::InterPAN(_) => Ok(3),
        }
    }
}

impl SerializeToBuf for SourceRoute {
    fn serialize_to_buf(&self, buf: &mut BufMut) -> ParseResult<()> {
        let len = self.relay_list.len();
        if len > 255 {
            return Err(ParseError::UnexpectedData);
        }
        (len as u8).serialize_to_buf(buf)?;
        self.relay_index.serialize_to_buf(buf)?;
        for relay in self.relay_list.iter() {
            relay.serialize_to_buf(buf)?;
        }
        Ok(())
    }
}

#[test]
fn test_zigbee_nwk_frame() {
    // Transport key transmission
    let frame = Frame {
        frame_type: FrameType::Data(
            vec![
                0x21, 0x05, 0x10, 0x00, 0x00, 0x00, 0x00, 0xd8, 0x5b, 0x3a, 0x13, 0x09, 0xff, 0x1b,
                0x1b, 0x97, 0x71, 0xa2, 0xaa, 0xda, 0x9f, 0x3b, 0x2b, 0x25, 0x14, 0x35, 0x32, 0x29,
                0x94, 0xd3, 0xf3, 0xd1, 0xa2, 0x98, 0xda, 0x93, 0x66, 0x9c, 0x8d, 0xff, 0x67, 0x73,
                0xef, 0x5f, 0x94, 0xc5,
            ]
            .into(),
        ),
        protocol_version: 2,
        destination: ShortAddress(0x558b),
        source: ShortAddress(0),
        radius: 30,
        sequence_number: 26,
        discover_route: DiscoverRoute::EnableRouteDiscovery,
        destination_ext: None,
        source_ext: None,
        source_route: None,
    };
    let serialized = vec![
        0x48, 0x00, 0x8b, 0x55, 0x00, 0x00, 0x1e, 0x1a, 0x21, 0x05, 0x10, 0x00, 0x00, 0x00, 0x00,
        0xd8, 0x5b, 0x3a, 0x13, 0x09, 0xff, 0x1b, 0x1b, 0x97, 0x71, 0xa2, 0xaa, 0xda, 0x9f, 0x3b,
        0x2b, 0x25, 0x14, 0x35, 0x32, 0x29, 0x94, 0xd3, 0xf3, 0xd1, 0xa2, 0x98, 0xda, 0x93, 0x66,
        0x9c, 0x8d, 0xff, 0x67, 0x73, 0xef, 0x5f, 0x94, 0xc5,
    ];
    assert_eq!(frame.serialize_as_vec().unwrap(), serialized);
}
