use crate::ieee802154::{ExtendedAddress, ShortAddress};
use crate::parse_serialize::{
    Deserialize, DeserializeError, DeserializeResult, Serialize, SerializeError, SerializeResult,
    SerializeTagged,
};
use bitfield::bitfield;
use std::convert::{TryFrom, TryInto};

pub enum Command {}

pub enum FrameType {
    Data(Vec<u8>),
    Command(Command),
    InterPAN(Vec<u8>),
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
default_serialization_newtype!(FrameControl, u16);

impl Serialize for Frame {
    fn serialize_to(&self, target: &mut Vec<u8>) -> SerializeResult<()> {
        let mut fsf = FrameControl(0);
        fsf.set_frame_type(self.frame_type.serialize_tag()?);
        if self.protocol_version > 15 {
            return Err(SerializeError::UnexpectedData);
        }
        fsf.set_protocol_version(self.protocol_version.into());
        fsf.set_discover_route(self.discover_route as u16);
        fsf.set_multicast_flag(0); // TODO
        fsf.set_security(0); // TODO
        fsf.set_source_route(self.source_route.is_some().into());
        fsf.set_destination_ieee_address(self.destination_ext.is_some().into());
        fsf.set_source_ieee_address(self.source_ext.is_some().into());
        fsf.set_reserved(0);
        (
            fsf,
            self.destination,
            self.source,
            self.radius,
            self.sequence_number,
        )
            .serialize_to(target)?;
        if let Some(destination_ext) = self.destination_ext.as_ref() {
            destination_ext.serialize_to(target)?;
        }
        if let Some(source_ext) = self.source_ext.as_ref() {
            source_ext.serialize_to(target)?;
        }
        if let Some(source_route) = self.source_route.as_ref() {
            source_route.serialize_to(target)?;
        }
        // TODO: Rest may be encrypted.
        self.frame_type.serialize_to(target)
    }
}
impl Deserialize for Frame {
    fn deserialize(input: &[u8]) -> DeserializeResult<Self> {
        let (input, fsf) = FrameControl::deserialize(input)?;
        let protocol_version = fsf.protocol_version();
        let discover_route: DiscoverRoute =
            fsf.discover_route()
                .try_into()
                .map_err(|e: enum_tryfrom::InvalidEnumValue| {
                    nom::Err::Error(DeserializeError(input, e.into()))
                })?;
        if fsf.multicast_flag() != 0 {
            return Err(nom::Err::Error(DeserializeError::unimplemented(
                input,
                "Multicast not yet supported",
            )));
        }
        if fsf.security() != 0 {
            return DeserializeError::unimplemented(input, "Security not yet supported").into();
        }
        if fsf.reserved() != 0 {
            return DeserializeError::unimplemented(input, "Reserved was not 0").into();
        }
        let (input, (destination, source, radius, sequence_number)) =
            <(ShortAddress, ShortAddress, u8, u8)>::deserialize(input)?;
        unimplemented!()
    }
}

impl Serialize for FrameType {
    fn serialize_to(&self, target: &mut Vec<u8>) -> SerializeResult<()> {
        match self {
            FrameType::Data(payload) => {
                target.extend_from_slice(&payload);
                Ok(())
            }
            _ => Err(SerializeError::Unimplemented("Not yet implemented")),
        }
    }
}
impl SerializeTagged<u16> for FrameType {
    fn serialize_tag(&self) -> SerializeResult<u16> {
        match self {
            FrameType::Data(_) => Ok(0),
            FrameType::Command(_) => Ok(1),
            FrameType::InterPAN(_) => Ok(3),
        }
    }
}

impl Serialize for SourceRoute {
    fn serialize_to(&self, target: &mut Vec<u8>) -> SerializeResult<()> {
        let len = self.relay_list.len();
        if len > 255 {
            return Err(SerializeError::UnexpectedData);
        }
        (len as u8, self.relay_index).serialize_to(target)?;
        for relay in self.relay_list.iter() {
            relay.serialize_to(target)?;
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
    assert_eq!(frame.serialize().unwrap(), serialized);
}
