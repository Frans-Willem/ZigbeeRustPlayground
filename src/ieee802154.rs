use bitfield::{bitfield, bitfield_bitrange, bitfield_fields, BitRange};
use bytes::{buf::FromBuf, Buf, Bytes, IntoBuf};
use std::result::Result;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShortAddress(u16);
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExtendedAddress(u64);
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PANID(u16);

#[derive(Debug)]
pub enum ParseError {
    InsufficientData,
    UnexpectedData,
    UnimplementedBehaviour,
}

pub trait ParseFromBuf: Sized {
    fn parse_from_buf(buf: &mut Buf) -> Result<Self, ParseError>;
}

impl ParseFromBuf for u16 {
    fn parse_from_buf(buf: &mut Buf) -> Result<u16, ParseError> {
        if buf.remaining() < 2 {
            Err(ParseError::InsufficientData)
        } else {
            Ok(buf.get_u16_le())
        }
    }
}

impl ParseFromBuf for u8 {
    fn parse_from_buf(buf: &mut Buf) -> Result<u8, ParseError> {
        if buf.remaining() < 1 {
            Err(ParseError::InsufficientData)
        } else {
            Ok(buf.get_u8())
        }
    }
}
impl ParseFromBuf for u64 {
    fn parse_from_buf(buf: &mut Buf) -> Result<u64, ParseError> {
        if buf.remaining() < 8 {
            Err(ParseError::InsufficientData)
        } else {
            Ok(buf.get_u64_le())
        }
    }
}

impl ParseFromBuf for PANID {
    fn parse_from_buf(buf: &mut Buf) -> Result<PANID, ParseError> {
        Ok(PANID(u16::parse_from_buf(buf)?))
    }
}
impl ParseFromBuf for ShortAddress {
    fn parse_from_buf(buf: &mut Buf) -> Result<ShortAddress, ParseError> {
        Ok(ShortAddress(u16::parse_from_buf(buf)?))
    }
}
impl ParseFromBuf for ExtendedAddress {
    fn parse_from_buf(buf: &mut Buf) -> Result<ExtendedAddress, ParseError> {
        Ok(ExtendedAddress(u64::parse_from_buf(buf)?))
    }
}

bitfield! {
    pub struct FrameControl(u16);
    impl Debug;
    pub frame_type, _: 2, 0;
    pub security_enabled, _: 3, 3;
    pub frame_pending, _: 4, 4;
    pub acknowledge_request, _: 5, 5;
    pub pan_id_compression, _: 6, 6;
    pub reserved, _: 7, 7;
    pub sequence_number_supression, _: 8, 8;
    pub information_elements_present, _: 9, 9;
    pub destination_addressing_mode, _: 11, 10;
    pub frame_version, _: 13, 12;
    pub source_addressing_mode, _: 15, 14;
}

impl ParseFromBuf for FrameControl {
    fn parse_from_buf(buf: &mut Buf) -> Result<FrameControl, ParseError> {
        Ok(FrameControl(u16::parse_from_buf(buf)?))
    }
}

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

#[derive(Debug, PartialEq)]
pub enum AddressSpecification {
    None,
    Reserved,
    Short(ShortAddress),
    Extended(ExtendedAddress),
}

impl AddressSpecification {
    fn parse_from_buf(mode: u16, buf: &mut Buf) -> Result<AddressSpecification, ParseError> {
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

bitfield! {
    pub struct SuperframeSpecification(u16);
    impl Debug;
    pub beacon_order, _: 3, 0;
    pub superframe_order, _: 7, 4;
    pub final_cap_slot, _: 11, 8;
    pub battery_life_extension, _: 12, 12;
    pub reserved, _: 13, 13;
    pub pan_coordinator, _: 14, 14;
    pub association_permit, _: 15, 15;
}

impl ParseFromBuf for SuperframeSpecification {
    fn parse_from_buf(buf: &mut Buf) -> Result<SuperframeSpecification, ParseError> {
        Ok(SuperframeSpecification(u16::parse_from_buf(buf)?))
    }
}

#[derive(Debug, PartialEq)]
pub enum MACDeviceType {
    RFD = 0,
}

#[derive(Debug, PartialEq)]
pub enum MACPowerSource {
    Battery = 0,
}

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
    DataRequest,   /* 0x04 */
    BeaconRequest, /* 0x07 */
}

impl ParseFromBuf for MACCommand {
    fn parse_from_buf(buf: &mut Buf) -> Result<MACCommand, ParseError> {
        let command_id = u8::parse_from_buf(buf)?;
        match command_id {
            4 => Ok(MACCommand::DataRequest),
            7 => Ok(MACCommand::BeaconRequest),
            _ => Err(ParseError::UnimplementedBehaviour),
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
}

impl MACFrameType {
    fn parse_from_buf(frame_type: u16, buf: &mut Buf) -> Result<MACFrameType, ParseError> {
        match frame_type {
            0 => {
                let superframe_spec = SuperframeSpecification::parse_from_buf(buf)?;
                let gts = u8::parse_from_buf(buf)?;
                let pending_addresses = u8::parse_from_buf(buf)?;
                if gts != 0 || pending_addresses != 0 {
                    Err(ParseError::UnimplementedBehaviour)
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
            _ => Err(ParseError::UnexpectedData),
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
