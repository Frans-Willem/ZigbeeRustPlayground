use crate::ieee802154::{ExtendedAddress, ShortAddress, PANID};
use crate::parse_serialize::Error as ParseError;
use crate::parse_serialize::Result as ParseResult;
use crate::parse_serialize::{
    ParseFromBuf, ParseFromBufTagged, SerializeToBuf, SerializeToBufTagged,
};
use bitfield::bitfield;
#[cfg(test)]
use bytes::IntoBuf;
use bytes::{Buf, BufMut, Bytes};
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
default_parse_serialize_enum!(AssociationResponseStatus, u8);

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
    pub payload: Bytes,
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
                payload: Bytes::new(),
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

/*=== Serialization & parsing ===*/
impl AddressSpecification {
    fn parse_from_buf(
        buf: &mut Buf,
        addressing_mode: u16,
        use_panid: Option<PANID>,
    ) -> Result<AddressSpecification, ParseError> {
        match addressing_mode {
            0 => Ok(AddressSpecification::None),
            1 => Err(ParseError::Unimplemented(
                "Unable to parse Frame with 'Reserved' address specification",
            ))?,
            2 => {
                let panid = match use_panid {
                    None => PANID::parse_from_buf(buf)?,
                    Some(panid) => panid,
                };
                let address = ShortAddress::parse_from_buf(buf)?;
                Ok(AddressSpecification::Short(panid, address))
            }
            3 => {
                let panid = match use_panid {
                    None => PANID::parse_from_buf(buf)?,
                    Some(panid) => panid,
                };
                let address = ExtendedAddress::parse_from_buf(buf)?;
                Ok(AddressSpecification::Extended(panid, address))
            }
            _ => Err(ParseError::UnexpectedData),
        }
    }

    fn serialize_to_buf(&self, buf: &mut BufMut, skip_panid: bool) -> Result<(), ParseError> {
        match self {
            AddressSpecification::None => Ok(()),
            AddressSpecification::Short(panid, address) => {
                if !skip_panid {
                    panid.serialize_to_buf(buf)?;
                }
                address.serialize_to_buf(buf)
            }
            AddressSpecification::Extended(panid, address) => {
                if !skip_panid {
                    panid.serialize_to_buf(buf)?;
                }
                address.serialize_to_buf(buf)
            }
        }
    }

    fn get_serialize_tag(&self) -> Result<u16, ParseError> {
        Ok(match self {
            AddressSpecification::None => 0,
            AddressSpecification::Short(_, _) => 2,
            AddressSpecification::Extended(_, _) => 3,
        })
    }
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

impl SerializeToBuf for Command {
    fn serialize_to_buf(&self, buf: &mut BufMut) -> ParseResult<()> {
        match self {
            Command::AssociationResponse {
                short_address,
                status,
            } => {
                (2 as u8).serialize_to_buf(buf)?;
                short_address.serialize_to_buf(buf)?;
                status.serialize_to_buf(buf)
            }
            _ => Err(ParseError::Unimplemented(
                "Serialization of command not implemented",
            )),
        }
    }
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
    fn serialize_to_buf(&self, buf: &mut BufMut) -> ParseResult<()> {
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
                        .map_err(|_| ParseError::Unimplemented("Beacon order is too big"))?,
                );
                ss.set_superframe_order(
                    (*superframe_order)
                        .try_into()
                        .map_err(|_| ParseError::Unimplemented("Superframe order is too big"))?,
                );
                ss.set_final_cap_slot(
                    (*final_cap_slot)
                        .try_into()
                        .map_err(|_| ParseError::Unimplemented("Final cap slot is too big"))?,
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
            FrameType::Command(cmd) => cmd.serialize_to_buf(buf),
            _ => Err(ParseError::Unimplemented("Frametype not implemented")),
        }
    }
}

impl SerializeToBufTagged<u16> for FrameType {
    fn get_serialize_tag(&self) -> Result<u16, ParseError> {
        match self {
            FrameType::Beacon { .. } => Ok(0),
            FrameType::Data => Ok(1),
            FrameType::Ack => Ok(2),
            FrameType::Command(_) => Ok(3),
            FrameType::Reserved => Ok(4),
            _ => Err(ParseError::Unimplemented("FrameType not implemented")),
        }
    }
}

impl ParseFromBuf for Frame {
    fn parse_from_buf(buf: &mut Buf) -> Result<Frame, ParseError> {
        let fsf = FrameControl::parse_from_buf(buf)?;
        let frame_pending = fsf.frame_pending() > 0;
        let acknowledge_request = fsf.acknowledge_request() > 0;
        let sequence_number = if fsf.sequence_number_supression() != 0 {
            None
        } else {
            Some(u8::parse_from_buf(buf)?)
        };
        let destination =
            AddressSpecification::parse_from_buf(buf, fsf.destination_addressing_mode(), None)?;
        let source_pan_compression: Option<PANID> = if fsf.pan_id_compression() != 0 {
            destination.into()
        } else {
            None
        };
        let source = AddressSpecification::parse_from_buf(
            buf,
            fsf.source_addressing_mode(),
            source_pan_compression,
        )?;
        let frame_type = FrameType::parse_from_buf(fsf.frame_type(), buf)?;
        let payload = buf.collect();
        Ok(Frame {
            frame_pending,
            acknowledge_request,
            sequence_number,
            destination,
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
            frame_pending: false,
            acknowledge_request: false,
            sequence_number: Some(165),
            destination: (PANID::broadcast(), ShortAddress::broadcast()).into(),
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
            frame_pending: false,
            acknowledge_request: false,
            sequence_number: Some(1),
            destination: (PANID(0x7698), ShortAddress::broadcast()).into(),
            source: (PANID(0x7698), ShortAddress(0)).into(),
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
            payload: Bytes::from(&input[11..])
        }
    );
}

impl SerializeToBuf for Frame {
    fn serialize_to_buf(&self, buf: &mut BufMut) -> ParseResult<()> {
        let mut fsf = FrameControl(0);
        fsf.set_frame_type(self.frame_type.get_serialize_tag()?);
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
        fsf.set_destination_addressing_mode(self.destination.get_serialize_tag()?);
        fsf.set_frame_version(0);
        fsf.set_source_addressing_mode(self.source.get_serialize_tag()?);
        fsf.serialize_to_buf(buf)?;
        if let Some(x) = self.sequence_number {
            x.serialize_to_buf(buf)?;
        }
        self.destination.serialize_to_buf(buf, false)?;
        self.source.serialize_to_buf(buf, pan_id_compression)?;
        self.frame_type.serialize_to_buf(buf)?;
        self.payload.serialize_to_buf(buf)?;
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
        payload: Bytes::new(),
    };
    let mut buf = vec![];
    input.serialize_to_buf(&mut buf).unwrap();
    assert_eq!(
        vec![
            0x63, 0xcc, 0x0a, 0x98, 0x76, 0x06, 0x63, 0x1c, 0xfe, 0xff, 0x5e, 0xcf, 0xd0, 0x15,
            0x68, 0x89, 0x0e, 0x00, 0x4b, 0x12, 0x00, 0x02, 0x8b, 0x55, 0x00,
        ],
        buf
    );
}
