use bitfield::bitfield;
use bytes::{Buf, BufMut, Bytes};
use std::convert::{TryFrom, TryInto};
use std::result::Result;
#[cfg(test)]
use bytes::{IntoBuf};

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
    pub struct SuperframeSpecification(u16);
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

#[derive(Debug, PartialEq)]
pub enum MACDeviceType {
    RFD = 0, // Reduced function device
    FFD = 1, // Full functioning device
}

#[derive(Debug, PartialEq)]
pub enum MACPowerSource {
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

#[derive(Debug, PartialEq)]
pub enum MACCommand {
    AssociationRequest {
        /* 0x01 */
        alternate_pan_coordinator: bool,
        device_type: MACDeviceType,
        power_source: MACPowerSource,
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

impl ParseFromBuf for MACCommand {
    fn parse_from_buf(buf: &mut Buf) -> Result<MACCommand, ParseError> {
        let command_id = u8::parse_from_buf(buf)?;
        match command_id {
            2 => {
                let short_address = ShortAddress::parse_from_buf(buf)?;
                let status = AssociationResponseStatus::parse_from_buf(buf)?;
                Ok(MACCommand::AssociationResponse {
                    short_address,
                    status,
                })
            }
            4 => Ok(MACCommand::DataRequest),
            7 => Ok(MACCommand::BeaconRequest),
            _ => Err(ParseError::Unimplemented),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum MACFrameType {
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
    Command(MACCommand),
    Reserved,
    Multipurpose,
    Fragment,
    Extended,
}

impl ParseFromBufTagged<u16> for MACFrameType {
    fn parse_from_buf(frame_type: u16, buf: &mut Buf) -> Result<MACFrameType, ParseError> {
        match frame_type {
            0 => {
                let superframe_spec = SuperframeSpecification::parse_from_buf(buf)?;
                let gts = u8::parse_from_buf(buf)?;
                let pending_addresses = u8::parse_from_buf(buf)?;
                if gts != 0 || pending_addresses != 0 {
                    Err(ParseError::Unimplemented)
                } else {
                    Ok(MACFrameType::Beacon {
                        beacon_order: superframe_spec.beacon_order() as usize,
                        superframe_order: superframe_spec.superframe_order() as usize,
                        final_cap_slot: superframe_spec.final_cap_slot() as usize,
                        battery_life_extension: superframe_spec.battery_life_extension() != 0,
                        pan_coordinator: superframe_spec.pan_coordinator() != 0,
                        association_permit: superframe_spec.association_permit() != 0,
                    })
                }
            }
            1 => Ok(MACFrameType::Data),
            2 => Ok(MACFrameType::Ack),
            3 => Ok(MACFrameType::Command(MACCommand::parse_from_buf(buf)?)),
            4 => Ok(MACFrameType::Reserved),
            _ => Err(if frame_type > 7 {
                ParseError::UnexpectedData
            } else {
                ParseError::Unimplemented
            }),
        }
    }
}

impl SerializeToBuf for MACFrameType {
    fn serialize_to_buf(&self, buf: &mut BufMut) -> SerializeResult {
        match self {
            MACFrameType::Beacon {
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
            MACFrameType::Data => Ok(()),
            MACFrameType::Ack => Ok(()),
            _ => Err(SerializeError::Unimplemented),
        }
    }
}

impl SerializeToBufTagged<u16> for MACFrameType {
    fn get_serialize_tag(&self) -> Result<u16, SerializeError> {
        match self {
            MACFrameType::Beacon { .. } => Ok(0),
            MACFrameType::Data => Ok(1),
            MACFrameType::Ack => Ok(2),
            MACFrameType::Command(_) => Ok(3),
            MACFrameType::Reserved => Ok(4),
            _ => Err(SerializeError::Unimplemented),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct MACFrame {
    pub sequence_number: Option<u8>,
    pub destination_pan: Option<PANID>,
    pub destination: AddressSpecification,
    pub source_pan: Option<PANID>,
    pub source: AddressSpecification,
    pub frame_type: MACFrameType,
    pub payload: Bytes,
}

impl ParseFromBuf for MACFrame {
    fn parse_from_buf(buf: &mut Buf) -> Result<MACFrame, ParseError> {
        let fsf = FrameControl::parse_from_buf(buf)?;
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
        let frame_type = MACFrameType::parse_from_buf(fsf.frame_type(), buf)?;
        let payload = buf.collect();
        Ok(MACFrame {
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
    let parsed = MACFrame::parse_from_buf(&mut input.into_buf()).unwrap();
    assert_eq!(
        parsed,
        MACFrame {
            sequence_number: Some(165),
            destination_pan: Some(PANID(0xFFFF)),
            destination: AddressSpecification::Short(ShortAddress(0xFFFF)),
            source_pan: None,
            source: AddressSpecification::None,
            frame_type: MACFrameType::Command(MACCommand::BeaconRequest),
            payload: Bytes::new()
        }
    );

    // Link Status
    let input: [u8; 44] = [
        0x41, 0x88, 0x01, 0x98, 0x76, 0xFF, 0xFF, 0x00, 0x00, 0x09, 0x12, 0xFC, 0xFF, 0x00, 0x00,
        0x01, 0x13, 0x15, 0x68, 0x89, 0x0e, 0x00, 0x4b, 0x12, 0x28, 0x02, 0x00, 0x00, 0x00, 0x15,
        0x68, 0x89, 0x0e, 0x00, 0x4b, 0x12, 0x00, 0x00, 0x71, 0x50, 0x83, 0x72, 0x0c, 0xe4,
    ];
    let parsed = MACFrame::parse_from_buf(&mut input.into_buf()).unwrap();
    assert_eq!(
        parsed,
        MACFrame {
            sequence_number: Some(1),
            destination_pan: Some(PANID(0x7698)),
            destination: AddressSpecification::Short(ShortAddress(0xFFFF)),
            source_pan: Some(PANID(0x7698)),
            source: AddressSpecification::Short(ShortAddress(0)),
            frame_type: MACFrameType::Data,
            payload: Bytes::from(&input[9..])
        }
    );

    // Beacon
    let input: [u8; 26] = [
        0x00, 0x80, 0x40, 0x98, 0x76, 0x00, 0x00, 0xff, 0xcf, 0x00, 0x00, 0x00, 0x22, 0x84, 0x15,
        0x68, 0x89, 0x0e, 0x00, 0x4b, 0x12, 0x00, 0xff, 0xff, 0xff, 0x00,
    ];
    let parsed = MACFrame::parse_from_buf(&mut input.into_buf()).unwrap();
    assert_eq!(
        parsed,
        MACFrame {
            sequence_number: Some(64),
            source_pan: Some(PANID(0x7698)),
            source: AddressSpecification::Short(ShortAddress(0)),
            destination_pan: None,
            destination: AddressSpecification::None,
            frame_type: MACFrameType::Beacon {
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

impl SerializeToBuf for MACFrame {
    fn serialize_to_buf(&self, buf: &mut BufMut) -> SerializeResult {
        let mut fsf = FrameControl(0);
        fsf.set_frame_type(self.frame_type.get_serialize_tag()?);
        fsf.set_security_enabled(0);
        fsf.set_frame_pending(0);
        fsf.set_acknowledge_request(0);
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
    let input = MACFrame {
        sequence_number: Some(64),
        destination_pan: None,
        destination: AddressSpecification::None,
        source_pan: PANID(0x7698).into(),
        source: ShortAddress(0).into(),
        frame_type: MACFrameType::Beacon {
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
    assert_eq!(vec![0x00, 0x80, 0x40, 0x98, 0x76, 0x00, 0x00, 0xFF, 0xCF, 0x00, 0x00, 0x00, 0x22, 0x84, 0x15, 0x68, 0x89, 0x0e, 0x00, 0x4b, 0x12, 0x00, 0xFF, 0xFF, 0xFF, 0x00], buf);
}
