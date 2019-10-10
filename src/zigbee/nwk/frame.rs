use crate::ieee802154::{ExtendedAddress, ShortAddress};
use crate::parse_serialize::{
    Deserialize, DeserializeError, DeserializeResult, DeserializeTagged, Serialize, SerializeError,
    SerializeResult, SerializeTagged,
};
use crate::zigbee::security::{KeyIdentifier, MaybeSecured, SecuredData};
use bitfield::bitfield;
use std::convert::{TryFrom, TryInto};

#[derive(PartialEq, Eq, Debug)]
pub enum Command {}

#[derive(PartialEq, Eq, Debug)]
pub struct UntypedPayload(pub Vec<u8>);

impl Serialize for UntypedPayload {
    fn serialize_to(&self, target: &mut Vec<u8>) -> SerializeResult<()> {
        target.extend_from_slice(&self.0);
        Ok(())
    }
}

impl Deserialize for UntypedPayload {
    fn deserialize(input: &[u8]) -> DeserializeResult<UntypedPayload> {
        let (input, data) = nom::combinator::rest(input)?;
        Ok((input, UntypedPayload(data.to_vec())))
    }
}

#[derive(PartialEq, Eq, Debug)]
pub enum FrameType {
    Data(MaybeSecured<UntypedPayload>),
    Command(MaybeSecured<Command>),
    InterPAN(MaybeSecured<UntypedPayload>),
}

#[derive(PartialEq, Eq, Debug, Clone, Copy, TryFromPrimitive)]
#[TryFromPrimitiveType = "u16"]
pub enum DiscoverRoute {
    SupressRouteDiscovery = 0,
    EnableRouteDiscovery = 1,
}

#[derive(PartialEq, Eq, Debug)]
pub struct SourceRoute {
    relay_index: u8,
    relay_list: Vec<ShortAddress>,
}

#[derive(PartialEq, Eq, Debug)]
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
        let frame_type_tag = self.frame_type.serialize_tag()?;
        fsf.set_frame_type(frame_type_tag.1);
        if self.protocol_version > 15 {
            return Err(SerializeError::UnexpectedData);
        }
        fsf.set_protocol_version(self.protocol_version.into());
        fsf.set_discover_route(self.discover_route as u16);
        fsf.set_multicast_flag(0); // TODO
        fsf.set_security(frame_type_tag.0 as u16);
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
        self.frame_type.serialize_to(target)
    }
}
impl Deserialize for Frame {
    fn deserialize(input: &[u8]) -> DeserializeResult<Self> {
        let (input, fsf) = FrameControl::deserialize(input)?;
        let protocol_version = fsf.protocol_version() as u8;
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
        if fsf.reserved() != 0 {
            return DeserializeError::unimplemented(input, "Reserved was not 0").into();
        }
        let (input, (destination, source, radius, sequence_number)) =
            <(ShortAddress, ShortAddress, u8, u8)>::deserialize(input)?;
        let (input, destination_ext) = nom::combinator::cond(
            fsf.destination_ieee_address() != 0,
            ExtendedAddress::deserialize,
        )(input)?;
        let (input, source_ext) = nom::combinator::cond(
            fsf.source_ieee_address() != 0,
            ExtendedAddress::deserialize,
        )(input)?;
        let (input, source_route) =
            nom::combinator::cond(fsf.source_route() != 0, SourceRoute::deserialize)(input)?;
        let (input, frame_type) =
            FrameType::deserialize((fsf.security() != 0, fsf.frame_type()), input)?;
        Ok((
            input,
            Frame {
                frame_type,
                protocol_version,
                destination,
                source,
                radius,
                sequence_number,
                discover_route,
                destination_ext,
                source_ext,
                source_route,
            },
        ))
    }
}

impl Serialize for FrameType {
    fn serialize_to(&self, target: &mut Vec<u8>) -> SerializeResult<()> {
        match self {
            FrameType::Data(payload) => payload.serialize_to(target),
            _ => Err(SerializeError::Unimplemented("Not yet implemented")),
        }
    }
}
impl SerializeTagged for FrameType {
    type TagType = (bool, u16);
    fn serialize_tag(&self) -> SerializeResult<(bool, u16)> {
        match self {
            FrameType::Data(d) => Ok((d.serialize_tag()?, 0)),
            _ => Err(SerializeError::Unimplemented(
                "Not yet implemented frame type",
            )),
            /*
            FrameType::Command(d) => (d.serialize_tag()?, 1),
            FrameType::InterPAN(d) => (d.serialize_tag()?, 3),
            */
        }
    }
}
impl DeserializeTagged for FrameType {
    type TagType = (bool, u16);
    fn deserialize(tag: (bool, u16), input: &[u8]) -> DeserializeResult<FrameType> {
        match tag.1 {
            0 => {
                let (input, payload) = MaybeSecured::deserialize(tag.0, input)?;
                Ok((input, FrameType::Data(payload)))
            }
            _ => DeserializeError::unexpected_data(input).into(),
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
impl Deserialize for SourceRoute {
    fn deserialize(input: &[u8]) -> DeserializeResult<SourceRoute> {
        let (input, (len, relay_index)) = <(u8, u8)>::deserialize(input)?;
        let (input, relay_list) =
            nom::multi::count(ShortAddress::deserialize, len as usize)(input)?;
        Ok((
            input,
            SourceRoute {
                relay_index,
                relay_list,
            },
        ))
    }
}

#[test]
fn test_zigbee_nwk_frame_transport_key() {
    // Transport key transmission
    let frame = Frame {
        frame_type: FrameType::Data(MaybeSecured::Unsecured(UntypedPayload(vec![
            0x21, 0x05, 0x10, 0x00, 0x00, 0x00, 0x00, 0xd8, 0x5b, 0x3a, 0x13, 0x09, 0xff, 0x1b,
            0x1b, 0x97, 0x71, 0xa2, 0xaa, 0xda, 0x9f, 0x3b, 0x2b, 0x25, 0x14, 0x35, 0x32, 0x29,
            0x94, 0xd3, 0xf3, 0xd1, 0xa2, 0x98, 0xda, 0x93, 0x66, 0x9c, 0x8d, 0xff, 0x67, 0x73,
            0xef, 0x5f, 0x94, 0xc5,
        ]))),
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
    assert_eq!(frame, Frame::deserialize_complete(&serialized).unwrap());
}

#[test]
fn test_zigbee_nwk_frame_device_announcement() {
    let frame = Frame {
        frame_type: FrameType::Data(MaybeSecured::Secured(SecuredData {
            key_identifier: KeyIdentifier::Network(0),
            frame_counter: 0,
            extended_source: Some(ExtendedAddress(0xd0cf5efffe1c6306)),
            payload: vec![
                0x6c, 0x41, 0xb1, 0x8d, 0x1c, 0xf1, 0x21, 0xc4, 0x53, 0xc8, 0xd9, 0xcf, 0xa5, 0xf2,
                0xbc, 0x17, 0x9c, 0xfb, 0xee, 0x40, 0x03, 0x78, 0x23, 0x2d,
            ],
        })),
        protocol_version: 2,
        destination: ShortAddress(0xFFFD),
        source: ShortAddress(0x558B),
        radius: 30,
        sequence_number: 251,
        discover_route: DiscoverRoute::SupressRouteDiscovery,
        destination_ext: None,
        source_ext: Some(ExtendedAddress(0xd0cf5efffe1c6306)),
        source_route: None,
    };
    let serialized = vec![
        0x08, 0x12, 0xfd, 0xff, 0x8b, 0x55, 0x1e, 0xfb, 0x06, 0x63, 0x1c, 0xfe, 0xff, 0x5e, 0xcf,
        0xd0, 0x28, 0x00, 0x00, 0x00, 0x00, 0x06, 0x63, 0x1c, 0xfe, 0xff, 0x5e, 0xcf, 0xd0, 0x00,
        0x6c, 0x41, 0xb1, 0x8d, 0x1c, 0xf1, 0x21, 0xc4, 0x53, 0xc8, 0xd9, 0xcf, 0xa5, 0xf2, 0xbc,
        0x17, 0x9c, 0xfb, 0xee, 0x40, 0x03, 0x78, 0x23, 0x2d,
    ];
    assert_eq!(frame, Frame::deserialize_complete(&serialized).unwrap());
    assert_eq!(serialized, frame.serialize().unwrap());
}
