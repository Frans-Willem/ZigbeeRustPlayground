use crate::ieee802154::{ExtendedAddress, ShortAddress, PANID};
use crate::parse_serialize::{
    ParseError, ParseFromBuf, ParseFromBufTagged, ParseResult, SerializeError, SerializeResult,
    SerializeToBuf, SerializeToBufTagged,
};
use bitfield::bitfield;
#[cfg(test)]
use bytes::IntoBuf;
use bytes::{Buf, BufMut, Bytes};
use std::convert::{TryFrom, TryInto};

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
default_parse_serialize_newtype!(FrameControl, u16);

#[test]
fn test_frame_control_parsing() {
    // Beacon request
    let input: [u8; 2] = [0x03, 0x08];
    let parsed = FrameControl::parse_from_buf(&mut input.into_buf()).unwrap();
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
    let parsed = FrameControl::parse_from_buf(&mut input.into_buf()).unwrap();
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
    let parsed = FrameControl::parse_from_buf(&mut input.into_buf()).unwrap();
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
    let parsed = FrameControl::parse_from_buf(&mut input.into_buf()).unwrap();
    let mut buf = vec![];
    parsed.serialize_to_buf(&mut buf).unwrap();
    assert_eq!(buf, input);

    let input: [u8; 2] = [0x00, 0x80];
    let parsed = FrameControl::parse_from_buf(&mut input.into_buf()).unwrap();
    let mut buf = vec![];
    parsed.serialize_to_buf(&mut buf).unwrap();
    assert_eq!(buf, input);
}

#[derive(Debug, PartialEq)]
pub enum AddressSpecification {
    None,
    Reserved,
    Short(ShortAddress),
    Extended(ExtendedAddress),
}

impl ParseFromBufTagged<u16> for AddressSpecification {
    fn parse_from_buf(mode: u16, buf: &mut Buf) -> ParseResult<AddressSpecification> {
        match mode {
            0 => Ok(AddressSpecification::None),
            1 => Ok(AddressSpecification::Reserved),
            2 => Ok(AddressSpecification::Short(ShortAddress::parse_from_buf(
                buf,
            )?)),
            3 => Ok(AddressSpecification::Extended(
                ExtendedAddress::parse_from_buf(buf)?,
            )),
            _ => Err(ParseError::UnexpectedData),
        }
    }
}

impl SerializeToBuf for AddressSpecification {
    fn serialize_to_buf(&self, buf: &mut BufMut) -> SerializeResult {
        match self {
            AddressSpecification::None => Ok(()),
            AddressSpecification::Reserved => Ok(()),
            AddressSpecification::Short(x) => x.serialize_to_buf(buf),
            AddressSpecification::Extended(x) => x.serialize_to_buf(buf),
        }
    }
}

impl SerializeToBufTagged<u16> for AddressSpecification {
    fn get_serialize_tag(&self) -> Result<u16, SerializeError> {
        Ok(match self {
            AddressSpecification::None => 0,
            AddressSpecification::Reserved => 1,
            AddressSpecification::Short(_) => 2,
            AddressSpecification::Extended(_) => 3,
        })
    }
}

impl From<ShortAddress> for AddressSpecification {
    fn from(item: ShortAddress) -> Self {
        AddressSpecification::Short(item)
    }
}
impl From<ExtendedAddress> for AddressSpecification {
    fn from(item: ExtendedAddress) -> Self {
        AddressSpecification::Extended(item)
    }
}
impl From<Option<ShortAddress>> for AddressSpecification where {
    fn from(item: Option<ShortAddress>) -> Self {
        match item {
            None => AddressSpecification::None,
            Some(x) => AddressSpecification::Short(x),
        }
    }
}
impl From<Option<ExtendedAddress>> for AddressSpecification where {
    fn from(item: Option<ExtendedAddress>) -> Self {
        match item {
            None => AddressSpecification::None,
            Some(x) => AddressSpecification::Extended(x),
        }
    }
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
default_parse_serialize_newtype!(SuperframeSpecification, u16);

#[derive(Debug, PartialEq, TryFromPrimitive, Copy, Clone)]
#[TryFromPrimitiveType = "u8"]
pub enum DeviceType {
    RFD = 0, // Reduced function device
    FFD = 1, // Full functioning device
}

#[derive(Debug, PartialEq, TryFromPrimitive, Copy, Clone)]
#[TryFromPrimitiveType = "u8"]
pub enum PowerSource {
    Battery = 0, // Not AC powered
    Powered = 1, // AC powered
}

#[derive(Debug, PartialEq, TryFromPrimitive, Copy, Clone)]
#[TryFromPrimitiveType = "u8"]
pub enum AssociationResponseStatus {
    AssociationSuccessful = 0,
    PANAtCapacity = 1,
    PANAccessDenied = 2,
    HoppingSequenceOffsetDuplication = 3,
    FastAssociationSuccessful = 0x80,
}
default_parse_serialize_enum!(AssociationResponseStatus, u8);

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
default_parse_serialize_newtype!(AssociationRequest, u8);

#[derive(Debug, PartialEq)]
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

impl ParseFromBuf for Command {
    fn parse_from_buf(buf: &mut Buf) -> Result<Command, ParseError> {
        let command_id = u8::parse_from_buf(buf)?;
        match command_id {
            1 => {
                let bf = AssociationRequest::parse_from_buf(buf)?;
                Ok(Command::AssociationRequest {
                    alternate_pan_coordinator: bf.alternate_pan_coordinator() > 0,
                    device_type: bf.device_type().try_into().unwrap(),
                    power_source: bf.power_source().try_into().unwrap(),
                    receive_on_when_idle: bf.receive_on_when_idle() > 0,
                    security_capability: bf.security_capability() > 0,
                    allocate_address: bf.allocate_address() > 0,
                })
            }
            2 => {
                let short_address = ShortAddress::parse_from_buf(buf)?;
                let status = AssociationResponseStatus::parse_from_buf(buf)?;
                Ok(Command::AssociationResponse {
                    short_address,
                    status,
                })
            }
            4 => Ok(Command::DataRequest),
            7 => Ok(Command::BeaconRequest),
            _ => Err(ParseError::Unimplemented("Command not implemented")),
        }
    }
}

#[derive(Debug, PartialEq)]
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

impl ParseFromBufTagged<u16> for FrameType {
    fn parse_from_buf(frame_type: u16, buf: &mut Buf) -> Result<FrameType, ParseError> {
        match frame_type {
            0 => {
                let superframe_spec = SuperframeSpecification::parse_from_buf(buf)?;
                let gts = u8::parse_from_buf(buf)?;
                let pending_addresses = u8::parse_from_buf(buf)?;
                if gts != 0 || pending_addresses != 0 {
                    Err(ParseError::Unimplemented(
                        "Beacon frame, GTS or pending addresses not empty",
                    ))
                } else {
                    Ok(FrameType::Beacon {
                        beacon_order: superframe_spec.beacon_order() as usize,
                        superframe_order: superframe_spec.superframe_order() as usize,
                        final_cap_slot: superframe_spec.final_cap_slot() as usize,
                        battery_life_extension: superframe_spec.battery_life_extension() != 0,
                        pan_coordinator: superframe_spec.pan_coordinator() != 0,
                        association_permit: superframe_spec.association_permit() != 0,
                    })
                }
            }
            1 => Ok(FrameType::Data),
            2 => Ok(FrameType::Ack),
            3 => Ok(FrameType::Command(Command::parse_from_buf(buf)?)),
            4 => Ok(FrameType::Reserved),
            _ => Err(if frame_type > 7 {
                ParseError::UnexpectedData
            } else {
                ParseError::Unimplemented("MAC Type not implemented")
            }),
        }
    }
}

impl SerializeToBuf for FrameType {
    fn serialize_to_buf(&self, buf: &mut BufMut) -> SerializeResult {
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
                ss.serialize_to_buf(buf)?;
                ss.set_beacon_order(
                    (*beacon_order)
                        .try_into()
                        .map_err(|_| SerializeError::Unimplemented)?,
                );
                ss.set_superframe_order(
                    (*superframe_order)
                        .try_into()
                        .map_err(|_| SerializeError::Unimplemented)?,
                );
                ss.set_final_cap_slot(
                    (*final_cap_slot)
                        .try_into()
                        .map_err(|_| SerializeError::Unimplemented)?,
                );
                ss.set_battery_life_extension((*battery_life_extension).into());
                ss.set_reserved(0);
                ss.set_pan_coordinator((*pan_coordinator).into());
                ss.set_association_permit((*association_permit).into());
                ss.serialize_to_buf(buf)?;
                (0 as u8).serialize_to_buf(buf)?;
                (0 as u8).serialize_to_buf(buf)?;
                Ok(())
            }
            FrameType::Data => Ok(()),
            FrameType::Ack => Ok(()),
            _ => Err(SerializeError::Unimplemented),
        }
    }
}

impl SerializeToBufTagged<u16> for FrameType {
    fn get_serialize_tag(&self) -> Result<u16, SerializeError> {
        match self {
            FrameType::Beacon { .. } => Ok(0),
            FrameType::Data => Ok(1),
            FrameType::Ack => Ok(2),
            FrameType::Command(_) => Ok(3),
            FrameType::Reserved => Ok(4),
            _ => Err(SerializeError::Unimplemented),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Frame {
    pub acknowledge_request: bool,
    pub sequence_number: Option<u8>,
    pub destination_pan: Option<PANID>,
    pub destination: AddressSpecification,
    pub source_pan: Option<PANID>,
    pub source: AddressSpecification,
    pub frame_type: FrameType,
    pub payload: Bytes,
}

impl ParseFromBuf for Frame {
    fn parse_from_buf(buf: &mut Buf) -> Result<Frame, ParseError> {
        let fsf = FrameControl::parse_from_buf(buf)?;
        let acknowledge_request = fsf.acknowledge_request() > 0;
        let sequence_number = if fsf.sequence_number_supression() != 0 {
            None
        } else {
            Some(u8::parse_from_buf(buf)?)
        };
        let destination_pan = if fsf.destination_addressing_mode() == 0 {
            None
        } else {
            Some(PANID::parse_from_buf(buf)?)
        };
        let destination =
            AddressSpecification::parse_from_buf(fsf.destination_addressing_mode(), buf)?;
        let source_pan = if fsf.source_addressing_mode() == 0 {
            None
        } else if fsf.pan_id_compression() != 0 {
            destination_pan.clone()
        } else {
            Some(PANID::parse_from_buf(buf)?)
        };
        let source = AddressSpecification::parse_from_buf(fsf.source_addressing_mode(), buf)?;
        let frame_type = FrameType::parse_from_buf(fsf.frame_type(), buf)?;
        let payload = buf.collect();
        Ok(Frame {
            acknowledge_request,
            sequence_number,
            destination_pan,
            destination,
            source_pan,
            source,
            frame_type,
            payload,
        })
    }
}

#[test]
fn test_parse_mac_frame() {
    // Beacon request
    let input: [u8; 8] = [0x03, 0x08, 0xa5, 0xFF, 0xFF, 0xFF, 0xFF, 0x07];
    let parsed = Frame::parse_from_buf(&mut input.into_buf()).unwrap();
    assert_eq!(
        parsed,
        Frame {
            acknowledge_request: false,
            sequence_number: Some(165),
            destination_pan: Some(PANID(0xFFFF)),
            destination: AddressSpecification::Short(ShortAddress(0xFFFF)),
            source_pan: None,
            source: AddressSpecification::None,
            frame_type: FrameType::Command(Command::BeaconRequest),
            payload: Bytes::new()
        }
    );

    // Link Status
    let input: [u8; 44] = [
        0x41, 0x88, 0x01, 0x98, 0x76, 0xFF, 0xFF, 0x00, 0x00, 0x09, 0x12, 0xFC, 0xFF, 0x00, 0x00,
        0x01, 0x13, 0x15, 0x68, 0x89, 0x0e, 0x00, 0x4b, 0x12, 0x28, 0x02, 0x00, 0x00, 0x00, 0x15,
        0x68, 0x89, 0x0e, 0x00, 0x4b, 0x12, 0x00, 0x00, 0x71, 0x50, 0x83, 0x72, 0x0c, 0xe4,
    ];
    let parsed = Frame::parse_from_buf(&mut input.into_buf()).unwrap();
    assert_eq!(
        parsed,
        Frame {
            acknowledge_request: false,
            sequence_number: Some(1),
            destination_pan: Some(PANID(0x7698)),
            destination: AddressSpecification::Short(ShortAddress(0xFFFF)),
            source_pan: Some(PANID(0x7698)),
            source: AddressSpecification::Short(ShortAddress(0)),
            frame_type: FrameType::Data,
            payload: Bytes::from(&input[9..])
        }
    );

    // Beacon
    let input: [u8; 26] = [
        0x00, 0x80, 0x40, 0x98, 0x76, 0x00, 0x00, 0xff, 0xcf, 0x00, 0x00, 0x00, 0x22, 0x84, 0x15,
        0x68, 0x89, 0x0e, 0x00, 0x4b, 0x12, 0x00, 0xff, 0xff, 0xff, 0x00,
    ];
    let parsed = Frame::parse_from_buf(&mut input.into_buf()).unwrap();
    assert_eq!(
        parsed,
        Frame {
            acknowledge_request: false,
            sequence_number: Some(64),
            source_pan: Some(PANID(0x7698)),
            source: AddressSpecification::Short(ShortAddress(0)),
            destination_pan: None,
            destination: AddressSpecification::None,
            frame_type: FrameType::Beacon {
                beacon_order: 15,
                superframe_order: 15,
                final_cap_slot: 15,
                battery_life_extension: false,
                pan_coordinator: true,
                association_permit: true,
            },
            payload: Bytes::from(&input[11..])
        }
    );
}

impl SerializeToBuf for Frame {
    fn serialize_to_buf(&self, buf: &mut BufMut) -> SerializeResult {
        let mut fsf = FrameControl(0);
        fsf.set_frame_type(self.frame_type.get_serialize_tag()?);
        fsf.set_security_enabled(0);
        fsf.set_frame_pending(0);
        fsf.set_acknowledge_request(self.acknowledge_request.into());
        fsf.set_pan_id_compression(
            (self.source_pan == self.destination_pan && self.source_pan.is_some()).into(),
        );
        fsf.set_reserved(0);
        fsf.set_sequence_number_supression(self.sequence_number.is_none().into());
        fsf.set_information_elements_present(0);
        fsf.set_destination_addressing_mode(self.destination.get_serialize_tag()?);
        fsf.set_frame_version(0);
        fsf.set_source_addressing_mode(self.source.get_serialize_tag()?);
        fsf.serialize_to_buf(buf)?;
        if let Some(x) = self.sequence_number {
            x.serialize_to_buf(buf)?;
        }
        if let Some(x) = self.destination_pan {
            x.serialize_to_buf(buf)?;
        }
        self.destination.serialize_to_buf(buf)?;
        if self.source_pan != self.destination_pan {
            if let Some(x) = self.source_pan {
                x.serialize_to_buf(buf)?;
            }
        }
        self.frame_type.serialize_to_buf(buf)?;
        self.payload.serialize_to_buf(buf)?;
        Ok(())
    }
}

#[test]
fn test_serialize_mac_frame() {
    let input = Frame {
        acknowledge_request: false,
        sequence_number: Some(64),
        destination_pan: None,
        destination: AddressSpecification::None,
        source_pan: PANID(0x7698).into(),
        source: ShortAddress(0).into(),
        frame_type: FrameType::Beacon {
            beacon_order: 15,
            superframe_order: 15,
            final_cap_slot: 15,
            battery_life_extension: false,
            pan_coordinator: true,
            association_permit: true,
        },
        payload: Bytes::from(&b"\x00\x22\x84\x15\x68\x89\x0e\x00\x4b\x12\x00\xff\xff\xff\x00"[..]),
    };
    let mut buf = vec![];
    input.serialize_to_buf(&mut buf).unwrap();
    assert_eq!(
        vec![
            0x00, 0x80, 0x40, 0x98, 0x76, 0x00, 0x00, 0xFF, 0xCF, 0x00, 0x00, 0x00, 0x22, 0x84,
            0x15, 0x68, 0x89, 0x0e, 0x00, 0x4b, 0x12, 0x00, 0xFF, 0xFF, 0xFF, 0x00
        ],
        buf
    );
}

impl Frame {
    pub fn create_ack(&self) -> Option<Frame> {
        if !self.acknowledge_request {
            None
        } else {
            Some(Frame {
                acknowledge_request: false,
                sequence_number: self.sequence_number,
                destination_pan: None,
                destination: AddressSpecification::None,
                source_pan: None,
                source: AddressSpecification::None,
                frame_type: FrameType::Ack,
                payload: Bytes::new(),
            })
        }
    }
}
