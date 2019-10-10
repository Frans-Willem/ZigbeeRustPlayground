use crate::ieee802154::{ExtendedAddress, ShortAddress, PANID};
use crate::parse_serialize::{
    Deserialize, DeserializeError, DeserializeResult, DeserializeTagged, Serialize, SerializeError,
    SerializeResult, SerializeTagged,
};
use bitfield::bitfield;
#[cfg(test)]
use std::convert::{TryFrom, TryInto};

/*=== Publicly accessible structures & enums ===*/

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum AddressSpecification {
    None,
    Short(PANID, ShortAddress),
    Extended(PANID, ExtendedAddress),
}

#[derive(Debug, PartialEq, TryFromPrimitive, Copy, Clone, Eq, Hash)]
#[TryFromPrimitiveType = "u8"]
pub enum DeviceType {
    RFD = 0, // Reduced function device
    FFD = 1, // Full functioning device
}

#[derive(Debug, PartialEq, TryFromPrimitive, Copy, Clone, Eq, Hash)]
#[TryFromPrimitiveType = "u8"]
pub enum PowerSource {
    Battery = 0, // Not AC powered
    Powered = 1, // AC powered
}

#[derive(Debug, PartialEq, TryFromPrimitive, Copy, Clone, Eq, Hash)]
#[TryFromPrimitiveType = "u8"]
pub enum AssociationResponseStatus {
    AssociationSuccessful = 0,
    PANAtCapacity = 1,
    PANAccessDenied = 2,
    HoppingSequenceOffsetDuplication = 3,
    FastAssociationSuccessful = 0x80,
}
default_serialization_enum!(AssociationResponseStatus, u8);

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub enum Command {
    AssociationRequest {
        /* 0x01 */
        alternate_pan_coordinator: bool,
        device_type: DeviceType,
        power_source: PowerSource,
        receive_on_when_idle: bool,
        security_capability: bool,
        allocate_address: bool,
    },
    AssociationResponse {
        /* 0x02 */
        short_address: ShortAddress,
        status: AssociationResponseStatus,
    },
    DataRequest,   /* 0x04 */
    BeaconRequest, /* 0x07 */
}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub enum FrameType {
    Beacon {
        beacon_order: usize,
        superframe_order: usize,
        final_cap_slot: usize,
        battery_life_extension: bool,
        pan_coordinator: bool,
        association_permit: bool,
    },
    Data,
    Ack,
    Command(Command),
    Reserved,
    Multipurpose,
    Fragment,
    Extended,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct Frame {
    pub frame_pending: bool,
    pub acknowledge_request: bool,
    pub sequence_number: Option<u8>,
    pub destination: AddressSpecification,
    pub source: AddressSpecification,
    pub frame_type: FrameType,
    pub payload: Vec<u8>,
}

impl Frame {
    /**
     * TODO: Doing this manually is too slow, it should be left to hardware.
     * Maybe remove ?
     */
    pub fn create_ack(&self, frame_pending: bool) -> Option<Frame> {
        if !self.acknowledge_request {
            None
        } else {
            Some(Frame {
                frame_pending,
                acknowledge_request: false,
                sequence_number: self.sequence_number,
                destination: AddressSpecification::None,
                source: AddressSpecification::None,
                frame_type: FrameType::Ack,
                payload: vec![],
            })
        }
    }

    pub fn expect_ack(&self) -> Option<u8> {
        if !self.acknowledge_request {
            None
        } else {
            self.sequence_number
        }
    }
}

/*=== Into & From implementations */
impl From<(PANID, ShortAddress)> for AddressSpecification {
    fn from(item: (PANID, ShortAddress)) -> Self {
        AddressSpecification::Short(item.0, item.1)
    }
}

impl From<(PANID, ExtendedAddress)> for AddressSpecification {
    fn from(item: (PANID, ExtendedAddress)) -> Self {
        AddressSpecification::Extended(item.0, item.1)
    }
}

impl<A> From<Option<A>> for AddressSpecification
where
    A: Into<AddressSpecification>,
{
    fn from(item: Option<A>) -> Self {
        match item {
            None => AddressSpecification::None,
            Some(x) => x.into(),
        }
    }
}

impl Into<Option<PANID>> for AddressSpecification {
    fn into(self) -> Option<PANID> {
        match self {
            AddressSpecification::None => None,
            AddressSpecification::Short(panid, _) => Some(panid),
            AddressSpecification::Extended(panid, _) => Some(panid),
        }
    }
}

/*=== Bitfields for serialization & parsing ===*/
bitfield! {
    pub struct FrameControl(u16);
    impl Debug;
    pub frame_type, set_frame_type: 2, 0;
    pub security_enabled, set_security_enabled: 3, 3;
    pub frame_pending, set_frame_pending: 4, 4;
    pub acknowledge_request, set_acknowledge_request: 5, 5;
    pub pan_id_compression, set_pan_id_compression: 6, 6;
    pub reserved, set_reserved: 7, 7;
    pub sequence_number_supression, set_sequence_number_supression: 8, 8;
    pub information_elements_present, set_information_elements_present: 9, 9;
    pub destination_addressing_mode, set_destination_addressing_mode: 11, 10;
    pub frame_version, set_frame_version: 13, 12;
    pub source_addressing_mode, set_source_addressing_mode: 15, 14;
}
default_serialization_newtype!(FrameControl, u16);

#[test]
fn test_frame_control_parsing() {
    // Beacon request
    let input: [u8; 2] = [0x03, 0x08];
    let parsed = FrameControl::deserialize_complete(&input).unwrap();
    assert_eq!(parsed.frame_type(), 3);
    assert_eq!(parsed.security_enabled(), 0);
    assert_eq!(parsed.frame_pending(), 0);
    assert_eq!(parsed.acknowledge_request(), 0);
    assert_eq!(parsed.pan_id_compression(), 0);
    assert_eq!(parsed.reserved(), 0);
    assert_eq!(parsed.sequence_number_supression(), 0);
    assert_eq!(parsed.information_elements_present(), 0);
    assert_eq!(parsed.destination_addressing_mode(), 2);
    assert_eq!(parsed.frame_version(), 0);
    assert_eq!(parsed.source_addressing_mode(), 0);

    // Link status
    let input: [u8; 2] = [0x41, 0x88];
    let parsed = FrameControl::deserialize_complete(&input).unwrap();
    assert_eq!(parsed.frame_type(), 1);
    assert_eq!(parsed.security_enabled(), 0);
    assert_eq!(parsed.frame_pending(), 0);
    assert_eq!(parsed.acknowledge_request(), 0);
    assert_eq!(parsed.pan_id_compression(), 1);
    assert_eq!(parsed.reserved(), 0);
    assert_eq!(parsed.sequence_number_supression(), 0);
    assert_eq!(parsed.information_elements_present(), 0);
    assert_eq!(parsed.destination_addressing_mode(), 2);
    assert_eq!(parsed.frame_version(), 0);
    assert_eq!(parsed.source_addressing_mode(), 2);

    // Beacon
    let input: [u8; 2] = [0x00, 0x80];
    let parsed = FrameControl::deserialize_complete(&input).unwrap();
    assert_eq!(parsed.frame_type(), 0);
    assert_eq!(parsed.security_enabled(), 0);
    assert_eq!(parsed.frame_pending(), 0);
    assert_eq!(parsed.acknowledge_request(), 0);
    assert_eq!(parsed.pan_id_compression(), 0);
    assert_eq!(parsed.reserved(), 0);
    assert_eq!(parsed.sequence_number_supression(), 0);
    assert_eq!(parsed.information_elements_present(), 0);
    assert_eq!(parsed.destination_addressing_mode(), 0);
    assert_eq!(parsed.frame_version(), 0);
    assert_eq!(parsed.source_addressing_mode(), 2);
}

#[test]
fn test_frame_control_serialize() {
    let input: [u8; 2] = [0x41, 0x88];
    let parsed = FrameControl::deserialize_complete(&input).unwrap();
    assert_eq!(parsed.serialize().unwrap(), input);

    let input: [u8; 2] = [0x00, 0x80];
    let parsed = FrameControl::deserialize_complete(&input).unwrap();
    assert_eq!(parsed.serialize().unwrap(), input);
}

bitfield! {
    struct SuperframeSpecification(u16);
    impl Debug;
    pub beacon_order, set_beacon_order: 3, 0;
    pub superframe_order, set_superframe_order: 7, 4;
    pub final_cap_slot, set_final_cap_slot: 11, 8;
    pub battery_life_extension, set_battery_life_extension: 12, 12;
    pub reserved, set_reserved: 13, 13;
    pub pan_coordinator, set_pan_coordinator: 14, 14;
    pub association_permit, set_association_permit: 15, 15;
}
default_serialization_newtype!(SuperframeSpecification, u16);

bitfield! {
    struct AssociationRequest(u8);
    impl Debug;
    pub alternate_pan_coordinator, set_alternate_pan_coordinator: 0, 0;
    pub device_type, set_device_type: 1, 1;
    pub power_source, set_power_source: 2, 2;
    pub receive_on_when_idle, set_receive_on_when_idle: 3, 3;
    pub association_type, set_association_type: 4, 4;
    pub reserved2, set_reserved2: 5, 5;
    pub security_capability, set_security_capability: 6, 6;
    pub allocate_address, set_allocate_address: 7, 7;
}
default_serialization_newtype!(AssociationRequest, u8);

/**
 * Not implementing Serialize & Deserialize, as these serializations take an extra parameter (PANID
 * handling).
 */
impl AddressSpecification {
    fn serialize_to(&self, skip_panid: bool, target: &mut Vec<u8>) -> SerializeResult<()> {
        match self {
            AddressSpecification::None => Ok(()),
            AddressSpecification::Short(panid, address) => {
                if !skip_panid {
                    panid.serialize_to(target)?;
                }
                address.serialize_to(target)
            }
            AddressSpecification::Extended(panid, address) => {
                if !skip_panid {
                    panid.serialize_to(target)?;
                }
                address.serialize_to(target)
            }
        }
    }

    fn serialize_tag(&self) -> SerializeResult<u16> {
        Ok(match self {
            AddressSpecification::None => 0,
            AddressSpecification::Short(_, _) => 2,
            AddressSpecification::Extended(_, _) => 3,
        })
    }

    fn deserialize(tag: u16, use_panid: Option<PANID>, input: &[u8]) -> DeserializeResult<Self> {
        match tag {
            0 => Ok((input, AddressSpecification::None)),
            1 => Err(nom::Err::Error(DeserializeError::unimplemented(
                input,
                "Unable to parse Frame with 'Reserved' address specification",
            ))),
            2 => {
                let (input, panid) = if let Some(panid) = use_panid {
                    (input, panid)
                } else {
                    PANID::deserialize(input)?
                };
                let (input, address) = ShortAddress::deserialize(input)?;
                Ok((input, AddressSpecification::Short(panid, address)))
            }
            3 => {
                let (input, panid) = if let Some(panid) = use_panid {
                    (input, panid)
                } else {
                    PANID::deserialize(input)?
                };
                let (input, address) = ExtendedAddress::deserialize(input)?;
                Ok((input, AddressSpecification::Extended(panid, address)))
            }
            _ => Err(nom::Err::Error(DeserializeError::unexpected_data(input))),
        }
    }
}
impl Deserialize for Command {
    fn deserialize(input: &[u8]) -> DeserializeResult<Self> {
        let (input, command_id) = u8::deserialize(input)?;
        match command_id {
            1 => {
                let (input, bf) = AssociationRequest::deserialize(input)?;
                Ok((
                    input,
                    Command::AssociationRequest {
                        alternate_pan_coordinator: bf.alternate_pan_coordinator() > 0,
                        device_type: bf.device_type().try_into().unwrap(),
                        power_source: bf.power_source().try_into().unwrap(),
                        receive_on_when_idle: bf.receive_on_when_idle() > 0,
                        security_capability: bf.security_capability() > 0,
                        allocate_address: bf.allocate_address() > 0,
                    },
                ))
            }
            2 => {
                let (input, short_address) = ShortAddress::deserialize(input)?;
                let (input, status) = AssociationResponseStatus::deserialize(input)?;
                Ok((
                    input,
                    Command::AssociationResponse {
                        short_address,
                        status,
                    },
                ))
            }
            4 => Ok((input, Command::DataRequest)),
            7 => Ok((input, Command::BeaconRequest)),
            _ => Err(nom::Err::Error(DeserializeError::unimplemented(
                input,
                "Command not implemented",
            ))),
        }
    }
}

impl Serialize for Command {
    fn serialize_to(&self, target: &mut Vec<u8>) -> SerializeResult<()> {
        match self {
            Command::AssociationResponse {
                short_address,
                status,
            } => (2 as u8, short_address, status).serialize_to(target),
            _ => Err(SerializeError::Unimplemented(
                "Serialization of command not implemented",
            )),
        }
    }
}

impl DeserializeTagged<u16> for FrameType {
    fn deserialize(frame_type: u16, input: &[u8]) -> DeserializeResult<FrameType> {
        match frame_type {
            0 => {
                let (input, (superframe_spec, gts, pending_addresses)) =
                    <(SuperframeSpecification, u8, u8)>::deserialize(input)?;
                if gts != 0 || pending_addresses != 0 {
                    Err(nom::Err::Error(DeserializeError::unimplemented(
                        input,
                        "Beacon frame, GTS or pending addresses not empty",
                    )))
                } else {
                    Ok((
                        input,
                        FrameType::Beacon {
                            beacon_order: superframe_spec.beacon_order() as usize,
                            superframe_order: superframe_spec.superframe_order() as usize,
                            final_cap_slot: superframe_spec.final_cap_slot() as usize,
                            battery_life_extension: superframe_spec.battery_life_extension() != 0,
                            pan_coordinator: superframe_spec.pan_coordinator() != 0,
                            association_permit: superframe_spec.association_permit() != 0,
                        },
                    ))
                }
            }
            1 => Ok((input, FrameType::Data)),
            2 => Ok((input, FrameType::Ack)),
            3 => nom::combinator::map(Command::deserialize, FrameType::Command)(input),
            4 => Ok((input, FrameType::Reserved)),
            _ => Err(nom::Err::Error(if frame_type > 7 {
                DeserializeError::unexpected_data(input)
            } else {
                DeserializeError::unimplemented(input, "MAC Type not implemented")
            })),
        }
    }
}

impl SerializeTagged<u16> for FrameType {
    fn serialize_tag(&self) -> SerializeResult<u16> {
        match self {
            FrameType::Beacon { .. } => Ok(0),
            FrameType::Data => Ok(1),
            FrameType::Ack => Ok(2),
            FrameType::Command(_) => Ok(3),
            FrameType::Reserved => Ok(4),
            _ => Err(SerializeError::Unimplemented("FrameType not implemented")),
        }
    }
}

impl Serialize for FrameType {
    fn serialize_to(&self, target: &mut Vec<u8>) -> SerializeResult<()> {
        match self {
            FrameType::Beacon {
                beacon_order,
                superframe_order,
                final_cap_slot,
                battery_life_extension,
                pan_coordinator,
                association_permit,
            } => {
                let mut ss = SuperframeSpecification(0);
                ss.set_beacon_order(
                    (*beacon_order)
                        .try_into()
                        .map_err(|_| SerializeError::Unimplemented("Beacon order is too big"))?,
                );
                ss.set_superframe_order(
                    (*superframe_order).try_into().map_err(|_| {
                        SerializeError::Unimplemented("Superframe order is too big")
                    })?,
                );
                ss.set_final_cap_slot(
                    (*final_cap_slot)
                        .try_into()
                        .map_err(|_| SerializeError::Unimplemented("Final cap slot is too big"))?,
                );
                ss.set_battery_life_extension((*battery_life_extension).into());
                ss.set_reserved(0);
                ss.set_pan_coordinator((*pan_coordinator).into());
                ss.set_association_permit((*association_permit).into());
                (ss, 0 as u8, 0 as u8).serialize_to(target)
            }
            FrameType::Data => Ok(()),
            FrameType::Ack => Ok(()),
            FrameType::Command(cmd) => cmd.serialize_to(target),
            _ => Err(SerializeError::Unimplemented("Frametype not implemented")),
        }
    }
}

impl Deserialize for Frame {
    fn deserialize(input: &[u8]) -> DeserializeResult<Frame> {
        let (input, fsf) = FrameControl::deserialize(input)?;
        let frame_pending = fsf.frame_pending() > 0;
        let acknowledge_request = fsf.acknowledge_request() > 0;
        let (input, sequence_number) =
            nom::combinator::cond(fsf.sequence_number_supression() == 0, u8::deserialize)(input)?;
        let (input, destination) =
            AddressSpecification::deserialize(fsf.destination_addressing_mode(), None, input)?;
        let source_pan_compression: Option<PANID> = if fsf.pan_id_compression() != 0 {
            destination.into()
        } else {
            None
        };
        let (input, source) = AddressSpecification::deserialize(
            fsf.source_addressing_mode(),
            source_pan_compression,
            input,
        )?;
        let (input, frame_type) = FrameType::deserialize(fsf.frame_type(), input)?;
        let (input, payload) = nom::combinator::rest(input)?;
        Ok((
            input,
            Frame {
                frame_pending,
                acknowledge_request,
                sequence_number,
                destination,
                source,
                frame_type,
                payload: payload.to_vec(),
            },
        ))
    }
}

#[test]
fn test_parse_mac_frame() {
    // Beacon request
    let input: [u8; 8] = [0x03, 0x08, 0xa5, 0xFF, 0xFF, 0xFF, 0xFF, 0x07];
    let parsed = Frame::deserialize_complete(&input).unwrap();
    assert_eq!(
        parsed,
        Frame {
            frame_pending: false,
            acknowledge_request: false,
            sequence_number: Some(165),
            destination: (PANID::broadcast(), ShortAddress::broadcast()).into(),
            source: AddressSpecification::None,
            frame_type: FrameType::Command(Command::BeaconRequest),
            payload: vec![]
        }
    );

    // Link Status
    let input: [u8; 44] = [
        0x41, 0x88, 0x01, 0x98, 0x76, 0xFF, 0xFF, 0x00, 0x00, 0x09, 0x12, 0xFC, 0xFF, 0x00, 0x00,
        0x01, 0x13, 0x15, 0x68, 0x89, 0x0e, 0x00, 0x4b, 0x12, 0x28, 0x02, 0x00, 0x00, 0x00, 0x15,
        0x68, 0x89, 0x0e, 0x00, 0x4b, 0x12, 0x00, 0x00, 0x71, 0x50, 0x83, 0x72, 0x0c, 0xe4,
    ];
    let parsed = Frame::deserialize_complete(&input).unwrap();
    assert_eq!(
        parsed,
        Frame {
            frame_pending: false,
            acknowledge_request: false,
            sequence_number: Some(1),
            destination: (PANID(0x7698), ShortAddress::broadcast()).into(),
            source: (PANID(0x7698), ShortAddress(0)).into(),
            frame_type: FrameType::Data,
            payload: input[9..].to_vec()
        }
    );

    // Beacon
    let input: [u8; 26] = [
        0x00, 0x80, 0x40, 0x98, 0x76, 0x00, 0x00, 0xff, 0xcf, 0x00, 0x00, 0x00, 0x22, 0x84, 0x15,
        0x68, 0x89, 0x0e, 0x00, 0x4b, 0x12, 0x00, 0xff, 0xff, 0xff, 0x00,
    ];
    let parsed = Frame::deserialize_complete(&input).unwrap();
    assert_eq!(
        parsed,
        Frame {
            frame_pending: false,
            acknowledge_request: false,
            sequence_number: Some(64),
            destination: AddressSpecification::None,
            source: (PANID(0x7698), ShortAddress(0)).into(),
            frame_type: FrameType::Beacon {
                beacon_order: 15,
                superframe_order: 15,
                final_cap_slot: 15,
                battery_life_extension: false,
                pan_coordinator: true,
                association_permit: true,
            },
            payload: input[11..].to_vec()
        }
    );
}

impl Serialize for Frame {
    fn serialize_to(&self, target: &mut Vec<u8>) -> SerializeResult<()> {
        let mut fsf = FrameControl(0);
        fsf.set_frame_type(self.frame_type.serialize_tag()?);
        fsf.set_security_enabled(0);
        fsf.set_frame_pending(self.frame_pending.into());
        fsf.set_acknowledge_request(self.acknowledge_request.into());
        let destination_pan: Option<PANID> = self.destination.into();
        let source_pan: Option<PANID> = self.source.into();
        let pan_id_compression = source_pan == destination_pan && source_pan.is_some();
        fsf.set_pan_id_compression(pan_id_compression.into());
        fsf.set_reserved(0);
        fsf.set_sequence_number_supression(self.sequence_number.is_none().into());
        fsf.set_information_elements_present(0);
        fsf.set_destination_addressing_mode(self.destination.serialize_tag()?);
        fsf.set_frame_version(0);
        fsf.set_source_addressing_mode(self.source.serialize_tag()?);
        fsf.serialize_to(target)?;
        if let Some(x) = self.sequence_number {
            x.serialize_to(target)?;
        }
        self.destination.serialize_to(false, target)?;
        self.source.serialize_to(pan_id_compression, target)?;
        self.frame_type.serialize_to(target)?;
        target.extend_from_slice(&self.payload);
        Ok(())
    }
}

#[test]
fn test_serialize_mac_frame() {
    let input = Frame {
        frame_pending: false,
        acknowledge_request: false,
        sequence_number: Some(64),
        destination: AddressSpecification::None,
        source: (PANID(0x7698), ShortAddress(0)).into(),
        frame_type: FrameType::Beacon {
            beacon_order: 15,
            superframe_order: 15,
            final_cap_slot: 15,
            battery_life_extension: false,
            pan_coordinator: true,
            association_permit: true,
        },
        payload: b"\x00\x22\x84\x15\x68\x89\x0e\x00\x4b\x12\x00\xff\xff\xff\x00".to_vec(),
    };
    assert_eq!(
        vec![
            0x00, 0x80, 0x40, 0x98, 0x76, 0x00, 0x00, 0xFF, 0xCF, 0x00, 0x00, 0x00, 0x22, 0x84,
            0x15, 0x68, 0x89, 0x0e, 0x00, 0x4b, 0x12, 0x00, 0xFF, 0xFF, 0xFF, 0x00
        ],
        input.serialize().unwrap()
    );

    let input = Frame {
        frame_pending: false,
        acknowledge_request: true,
        sequence_number: Some(10),
        destination: (PANID(0x7698), ExtendedAddress(0xd0cf5efffe1c6306)).into(),
        source: (PANID(0x7698), ExtendedAddress(0x00124b000e896815)).into(),
        frame_type: FrameType::Command(Command::AssociationResponse {
            short_address: ShortAddress(0x558b),
            status: AssociationResponseStatus::AssociationSuccessful,
        }),
        payload: vec![],
    };
    assert_eq!(
        vec![
            0x63, 0xcc, 0x0a, 0x98, 0x76, 0x06, 0x63, 0x1c, 0xfe, 0xff, 0x5e, 0xcf, 0xd0, 0x15,
            0x68, 0x89, 0x0e, 0x00, 0x4b, 0x12, 0x00, 0x02, 0x8b, 0x55, 0x00,
        ],
        input.serialize().unwrap()
    );
}
