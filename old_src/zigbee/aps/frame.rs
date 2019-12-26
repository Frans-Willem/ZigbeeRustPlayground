use crate::ieee802154::{ExtendedAddress, ShortAddress};
use crate::parse_serialize::{
    Deserialize, DeserializeError, DeserializeResult, DeserializeTagged, Serialize, SerializeError,
    SerializeResult, SerializeTagged,
};
use crate::zigbee::aps::commands::*;
use crate::zigbee::security::{KeyIdentifier, Securable, SecuredData};
use crate::zigbee::{ClusterId, EndpointId, GroupId, ProfileId};
use bitfield::bitfield;
use std::convert::{TryFrom, TryInto};

pub struct Payload(Vec<u8>);

impl Serialize for Payload {
    fn serialize_to(&self, target: &mut Vec<u8>) -> SerializeResult<()> {
        target.extend(&self.0);
        Ok(())
    }
}
impl Deserialize for Payload {
    fn deserialize(input: &[u8]) -> DeserializeResult<Self> {
        nom::combinator::map(nom::combinator::rest, |rest: &[u8]| Payload(rest.to_vec()))(input)
    }
}

#[derive(Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct ExtendedHeader {}

pub enum EndpointOrGroup {
    Endpoint(EndpointId),
    Group(GroupId),
}

impl Serialize for EndpointOrGroup {
    fn serialize_to(&self, target: &mut Vec<u8>) -> SerializeResult<()> {
        match self {
            EndpointOrGroup::Endpoint(e) => e.serialize_to(target),
            EndpointOrGroup::Group(g) => g.serialize_to(target),
        }
    }
}

impl SerializeTagged for EndpointOrGroup {
    type TagType = bool;
    fn serialize_tag(&self) -> SerializeResult<Self::TagType> {
        Ok(match self {
            EndpointOrGroup::Endpoint(_) => false,
            EndpointOrGroup::Group(_) => true,
        })
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum DeliveryMode {
    Unicast = 0,
    Reserved = 1,
    Broadcast = 2,
    Group = 3,
}

bitfield! {
    #[derive(Serialize)]
    pub struct FrameControl(u8);
    impl Debug;
    pub frame_type, set_frame_type: 1, 0;
    pub delivery_mode, set_delivery_mode: 3, 2;
    pub ack_format, set_ack_format: 4, 4;
    pub security, set_security: 5, 5;
    pub ack_request, set_ack_request: 6, 6;
    pub extended_header_present, set_extended_header_present: 7, 7;
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum DataDestination {
    Unicast(EndpointId),
    Broadcast(EndpointId), // Is this correct ?
    Group(GroupId),
}
pub enum Frame {
    Data {
        ack_request: bool,
        destination: DataDestination,
        cluster: (ClusterId, ProfileId),
        source: EndpointId,
        aps_counter: u8,
        extended_header: Option<ExtendedHeader>,
        payload: Securable<Payload>,
    },
    Command {
        delivery_mode: DeliveryMode,
        ack_request: bool,
        aps_counter: u8,
        payload: Securable<Command>,
    },
    DataAck {
        destination: EndpointId,
        cluster: (ClusterId, ProfileId),
        source: EndpointId,
        aps_counter: u8,
        extended_header: Option<ExtendedHeader>,
    },
    CommandAck {
        aps_counter: u8,
    },
}

impl Serialize for Frame {
    fn serialize_to(&self, target: &mut Vec<u8>) -> SerializeResult<()> {
        let mut frame_control = FrameControl(0);
        match self {
            Frame::Data {
                ack_request,
                destination,
                cluster,
                source,
                aps_counter,
                extended_header,
                payload,
            } => {
                frame_control.set_frame_type(0);
                match destination {
                    DataDestination::Unicast(_) => frame_control.set_delivery_mode(0),
                    DataDestination::Broadcast(_) => frame_control.set_delivery_mode(2),
                    DataDestination::Group(_) => frame_control.set_delivery_mode(3),
                }
                frame_control.set_ack_format(0);
                frame_control.set_security(payload.serialize_tag()? as u8);
                frame_control.set_ack_request(*ack_request as u8);
                frame_control.set_extended_header_present(extended_header.is_some() as u8);
                frame_control.serialize_to(target)?;
                match destination {
                    DataDestination::Unicast(e) => e.serialize_to(target)?,
                    DataDestination::Broadcast(e) => e.serialize_to(target)?,
                    DataDestination::Group(g) => g.serialize_to(target)?,
                }
                cluster.serialize_to(target)?;
                source.serialize_to(target)?;
                aps_counter.serialize_to(target)?;
                if let Some(extended_header) = extended_header {
                    extended_header.serialize_to(target)?;
                }
                payload.serialize_to(target)
            }
            Frame::Command {
                delivery_mode,
                ack_request,
                aps_counter,
                payload,
            } => {
                frame_control.set_frame_type(1);
                frame_control.set_delivery_mode(*delivery_mode as u8);
                frame_control.set_ack_format(0);
                frame_control.set_security(payload.serialize_tag()? as u8);
                frame_control.set_ack_request(*ack_request as u8);
                frame_control.set_extended_header_present(false as u8);
                frame_control.serialize_to(target)?;
                aps_counter.serialize_to(target)?;
                payload.serialize_to(target)
            }
            _ => Ok(()),
        }
    }
}

#[test]
fn test_aps_transport_key() {
    let serialized = vec![
        0x21, 0x06, 0x10, 0x01, 0x00, 0x00, 0x00, 0xe3, 0xbd, 0x18, 0x74, 0x09, 0x2c, 0x2c, 0xa3,
        0x58, 0x1d, 0x8a, 0x23, 0xb9, 0x6c, 0x3b, 0x80, 0xf0, 0xad, 0x27, 0x1c, 0x59, 0x8a, 0xdf,
        0x27, 0xbc, 0x21, 0xc7, 0x47, 0xf0, 0x31, 0x74, 0x80, 0xbc, 0x8c, 0x53, 0x88, 0x11, 0x8f,
        0x02,
    ];
    let frame = Frame::Command {
        delivery_mode: DeliveryMode::Unicast,
        ack_request: false,
        aps_counter: 6,
        payload: Securable::Secured(SecuredData {
            key_identifier: KeyIdentifier::KeyTransport,
            frame_counter: 1,
            extended_source: None,
            payload: vec![
                0xe3, 0xbd, 0x18, 0x74, 0x09, 0x2c, 0x2c, 0xa3, 0x58, 0x1d, 0x8a, 0x23, 0xb9, 0x6c,
                0x3b, 0x80, 0xf0, 0xad, 0x27, 0x1c, 0x59, 0x8a, 0xdf, 0x27, 0xbc, 0x21, 0xc7, 0x47,
                0xf0, 0x31, 0x74, 0x80, 0xbc, 0x8c, 0x53, 0x88, 0x11, 0x8f, 0x02,
            ],
        }),
    };
    assert_eq!(frame.serialize().unwrap(), serialized);
    //Frame::deserialize_complete(&serialized).unwrap();
}

#[test]
fn test_aps_device_announce() {
    let serialized = vec![
        0x08, 0x00, 0x13, 0x00, 0x00, 0x00, 0x00, 0x96, 0x81, 0x8b, 0x55, 0x06, 0x63, 0x1c, 0xfe,
        0xff, 0x5e, 0xcf, 0xd0, 0x80,
    ];
    let frame = Frame::Data {
        ack_request: false,
        destination: DataDestination::Broadcast(EndpointId(0)),
        cluster: (ClusterId(0x0013), ProfileId(0x0000)),
        source: EndpointId(0),
        aps_counter: 150,
        extended_header: None,
        payload: Securable::Unsecured(Payload(vec![
            0x81, 0x8b, 0x55, 0x06, 0x63, 0x1c, 0xfe, 0xff, 0x5e, 0xcf, 0xd0, 0x80,
        ])),
    };
    //Frame::deserialize_complete(&serialized).unwrap();
    assert_eq!(frame.serialize().unwrap(), serialized);
}
