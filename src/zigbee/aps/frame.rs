use crate::ieee802154::{ExtendedAddress, ShortAddress};
use crate::parse_serialize::{
    Deserialize, DeserializeError, DeserializeResult, DeserializeTagged, Serialize, SerializeError,
    SerializeResult, SerializeTagged,
};
use crate::zigbee::aps::commands::*;
use crate::zigbee::security::{KeyIdentifier, SecurableTagged, SecuredData};
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

pub enum FrameType {
    Data {
        destination: EndpointOrGroup,
        cluster: (ClusterId, ProfileId),
        source_endpoint: EndpointId,
        extended_header: Option<ExtendedHeader>,
        payload: Payload,
    },
    Command {
        command: Command,
    },
    DataAck {
        destination_endpoint: EndpointId,
        cluster: (ClusterId, ProfileId),
        source_endpoint: EndpointId,
        extended_header: Option<ExtendedHeader>,
    },
    CommandAck,
}

pub enum DeliveryMode {
    NormalUnicast = 0,
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

pub struct Frame {
    frame_type: FrameType,
    delivery_mode: DeliveryMode,
    aps_counter: u8,
}

impl Serialize for Frame {
    fn serialize_to(&self, target: &mut Vec<u8>) -> SerializeResult<()> {
        unimplemented!();
    }
}

impl Deserialize for Frame {
    fn deserialize(input: &[u8]) -> DeserializeResult<Self> {
        unimplemented!();
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
    /*
        let frame = Frame {
            frame_type: FrameType::Command(/*secured*/),
            delivery_mode: DeliveryMode::Unicast,

        };
    */
    Frame::deserialize_complete(&serialized).unwrap();
}

#[test]
fn test_aps_device_announce() {
    let serialized = vec![
        0x08, 0x00, 0x13, 0x00, 0x00, 0x00, 0x00, 0x96, 0x81, 0x8b, 0x55, 0x06, 0x63, 0x1c, 0xfe,
        0xff, 0x5e, 0xcf, 0xd0, 0x80,
    ];
    Frame::deserialize_complete(&serialized).unwrap();
}
