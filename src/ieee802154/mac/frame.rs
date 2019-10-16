use crate::ieee802154::mac::commands::*;
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

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct Beacon {
    pub beacon_order: usize,
    pub superframe_order: usize,
    pub final_cap_slot: usize,
    pub battery_life_extension: bool,
    pub pan_coordinator: bool,
    pub association_permit: bool,
    pub pending_short_addresses: Vec<ShortAddress>,
    pub pending_long_addresses: Vec<ExtendedAddress>,
    pub payload: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, SerializeTagged, DeserializeTagged)]
#[enum_tag_type(u16)]
pub enum FrameType {
    #[enum_tag(0)]
    Beacon(Beacon),
    #[enum_tag(1)]
    Data(Vec<u8>),
    #[enum_tag(2)]
    Ack,
    #[enum_tag(3)]
    Command(Command),
    #[enum_tag(4)]
    Reserved,
    #[enum_tag(5)]
    Multipurpose,
    #[enum_tag(6)]
    Fragment,
    #[enum_tag(7)]
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
    #[derive(Serialize, Deserialize)]
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
    #[derive(Serialize, Deserialize)]
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

impl Deserialize for Beacon {
    fn deserialize(input: &[u8]) -> DeserializeResult<Beacon> {
        let (input, (superframe_spec, gts, pending_addresses)) =
            <(SuperframeSpecification, u8, u8)>::deserialize(input)?;
        if gts != 0 || pending_addresses != 0 {
            Err(nom::Err::Error(DeserializeError::unimplemented(
                input,
                "Beacon frame, GTS or pending addresses not empty",
            )))
        } else {
            let (input, payload) = <Vec<u8>>::deserialize(input)?;
            Ok((
                input,
                Beacon {
                    beacon_order: superframe_spec.beacon_order() as usize,
                    superframe_order: superframe_spec.superframe_order() as usize,
                    final_cap_slot: superframe_spec.final_cap_slot() as usize,
                    battery_life_extension: superframe_spec.battery_life_extension() != 0,
                    pan_coordinator: superframe_spec.pan_coordinator() != 0,
                    association_permit: superframe_spec.association_permit() != 0,
                    pending_short_addresses: vec![],
                    pending_long_addresses: vec![],
                    payload,
                },
            ))
        }
    }
}

impl Serialize for Beacon {
    fn serialize_to(&self, target: &mut Vec<u8>) -> SerializeResult<()> {
        if self.beacon_order > 0xF {
            return Err(SerializeError::Unimplemented("Beacon order is too big"));
        }
        if self.superframe_order > 0xF {
            return Err(SerializeError::Unimplemented("Superframe order is too big"));
        }
        if self.final_cap_slot > 0xF {
            return Err(SerializeError::Unimplemented("Final cap slot is too big"));
        }
        let mut ss = SuperframeSpecification(0);
        ss.set_beacon_order(self.beacon_order as u16);
        ss.set_superframe_order(self.superframe_order as u16);
        ss.set_final_cap_slot(self.final_cap_slot as u16);
        ss.set_battery_life_extension((self.battery_life_extension).into());
        ss.set_reserved(0);
        ss.set_pan_coordinator((self.pan_coordinator).into());
        ss.set_association_permit((self.association_permit).into());
        (ss, 0 as u8, 0 as u8).serialize_to(target)?;
        self.payload.serialize_to(target)
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
        let (input, frame_type) = FrameType::deserialize_data(fsf.frame_type(), input)?;
        Ok((
            input,
            Frame {
                frame_pending,
                acknowledge_request,
                sequence_number,
                destination,
                source,
                frame_type,
            },
        ))
    }
}

#[test]
fn test_beacon_request() {
    // Beacon request
    let serialized = vec![0x03, 0x08, 0xa5, 0xFF, 0xFF, 0xFF, 0xFF, 0x07];
    let data = Frame {
        frame_pending: false,
        acknowledge_request: false,
        sequence_number: Some(165),
        destination: (PANID::broadcast(), ShortAddress::broadcast()).into(),
        source: AddressSpecification::None,
        frame_type: FrameType::Command(Command::BeaconRequest),
    };
    assert_eq!(Frame::deserialize_complete(&serialized).unwrap(), data);
    assert_eq!(data.serialize().unwrap(), serialized);
}
fn test_mac_link_status() {
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
            frame_type: FrameType::Data(input[9..].to_vec()),
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
            frame_type: FrameType::Beacon(Beacon {
                beacon_order: 15,
                superframe_order: 15,
                final_cap_slot: 15,
                battery_life_extension: false,
                pan_coordinator: true,
                association_permit: true,
                pending_long_addresses: vec![],
                pending_short_addresses: vec![],
                payload: input[11..].to_vec(),
            }),
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
        self.frame_type.serialize_data_to(target)?;
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
        frame_type: FrameType::Beacon(Beacon {
            beacon_order: 15,
            superframe_order: 15,
            final_cap_slot: 15,
            battery_life_extension: false,
            pan_coordinator: true,
            association_permit: true,
            pending_short_addresses: vec![],
            pending_long_addresses: vec![],
            payload: b"\x00\x22\x84\x15\x68\x89\x0e\x00\x4b\x12\x00\xff\xff\xff\x00".to_vec(),
        }),
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
        frame_type: FrameType::Command(Command::AssociationResponse(CommandAssociationResponse {
            short_address: ShortAddress(0x558b),
            status: AssociationResponseStatus::AssociationSuccessful,
        })),
    };
    assert_eq!(
        vec![
            0x63, 0xcc, 0x0a, 0x98, 0x76, 0x06, 0x63, 0x1c, 0xfe, 0xff, 0x5e, 0xcf, 0xd0, 0x15,
            0x68, 0x89, 0x0e, 0x00, 0x4b, 0x12, 0x00, 0x02, 0x8b, 0x55, 0x00,
        ],
        input.serialize().unwrap()
    );
}
