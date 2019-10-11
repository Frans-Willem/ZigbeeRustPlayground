use crate::ieee802154::ExtendedAddress;
use crate::parse_serialize::{
    Deserialize, DeserializeError, DeserializeResult, DeserializeTagged, Serialize, SerializeError,
    SerializeResult, SerializeTagged,
};
use generic_array::{typenum::U16, GenericArray};

#[derive(Eq, PartialEq, Debug)]
pub struct TrustCenterKeyDescriptor {
    key: GenericArray<u8, U16>,
    destination: ExtendedAddress,
    source: ExtendedAddress,
}
impl Serialize for TrustCenterKeyDescriptor {
    fn serialize_to(&self, target: &mut Vec<u8>) -> SerializeResult<()> {
        (self.key, self.destination, self.source).serialize_to(target)
    }
}
impl Deserialize for TrustCenterKeyDescriptor {
    fn deserialize(input: &[u8]) -> DeserializeResult<Self> {
        let (input, (key, destination, source)) =
            <(GenericArray<u8, U16>, ExtendedAddress, ExtendedAddress)>::deserialize(input)?;
        Ok((
            input,
            TrustCenterKeyDescriptor {
                key,
                destination,
                source,
            },
        ))
    }
}

#[derive(Eq, PartialEq, Debug)]
pub struct NetworkKeyDescriptor {
    key: GenericArray<u8, U16>,
    sequence_number: u8,
    destination: ExtendedAddress,
    source: ExtendedAddress,
}
impl Serialize for NetworkKeyDescriptor {
    fn serialize_to(&self, target: &mut Vec<u8>) -> SerializeResult<()> {
        (
            self.key,
            self.sequence_number,
            self.destination,
            self.source,
        )
            .serialize_to(target)
    }
}
impl Deserialize for NetworkKeyDescriptor {
    fn deserialize(input: &[u8]) -> DeserializeResult<Self> {
        let (input, (key, sequence_number, destination, source)) =
            <(GenericArray<u8, U16>, u8, ExtendedAddress, ExtendedAddress)>::deserialize(input)?;
        Ok((
            input,
            NetworkKeyDescriptor {
                key,
                sequence_number,
                destination,
                source,
            },
        ))
    }
}

#[derive(Eq, PartialEq, Debug)]
pub struct ApplicationKeyDescriptor {
    key: GenericArray<u8, U16>,
    partner: ExtendedAddress,
    initiator: bool,
}
impl Serialize for ApplicationKeyDescriptor {
    fn serialize_to(&self, target: &mut Vec<u8>) -> SerializeResult<()> {
        (self.key, self.partner, self.initiator as u8).serialize_to(target)
    }
}
impl Deserialize for ApplicationKeyDescriptor {
    fn deserialize(input: &[u8]) -> DeserializeResult<Self> {
        let (input, (key, partner, initiator)) =
            <(GenericArray<u8, U16>, ExtendedAddress, u8)>::deserialize(input)?;
        Ok((
            input,
            ApplicationKeyDescriptor {
                key,
                partner,
                initiator: initiator != 0,
            },
        ))
    }
}

#[derive(Eq, PartialEq, Debug)]
pub enum KeyDescriptor {
    TrustCenterMasterKey(TrustCenterKeyDescriptor), // 0
    StandardNetworkKey(NetworkKeyDescriptor),       // 1
    ApplicationMasterKey(ApplicationKeyDescriptor), // 2
    ApplicationLinkKey(ApplicationKeyDescriptor),   // 3
    UniqueTrustCenterLinkKey(TrustCenterKeyDescriptor), // 4
    HighSecurityNetworkKey(NetworkKeyDescriptor),   // 5
    Unknown(NetworkKeyDescriptor),                  // 6
}

impl Serialize for KeyDescriptor {
    fn serialize_to(&self, target: &mut Vec<u8>) -> SerializeResult<()> {
        match self {
            KeyDescriptor::TrustCenterMasterKey(x) => (0 as u8, x).serialize_to(target),
            KeyDescriptor::StandardNetworkKey(x) => (1 as u8, x).serialize_to(target),
            KeyDescriptor::ApplicationMasterKey(x) => (2 as u8, x).serialize_to(target),
            KeyDescriptor::ApplicationLinkKey(x) => (3 as u8, x).serialize_to(target),
            KeyDescriptor::UniqueTrustCenterLinkKey(x) => (4 as u8, x).serialize_to(target),
            KeyDescriptor::HighSecurityNetworkKey(x) => (5 as u8, x).serialize_to(target),
            KeyDescriptor::Unknown(x) => (6 as u8, x).serialize_to(target),
            _ => Err(SerializeError::Unimplemented("Not yet implemented")),
        }
    }
}

impl Deserialize for KeyDescriptor {
    fn deserialize(input: &[u8]) -> DeserializeResult<Self> {
        let (input, key_type) = u8::deserialize(input)?;
        match key_type {
            0 => nom::combinator::map(
                TrustCenterKeyDescriptor::deserialize,
                KeyDescriptor::TrustCenterMasterKey,
            )(input),
            1 => nom::combinator::map(
                NetworkKeyDescriptor::deserialize,
                KeyDescriptor::StandardNetworkKey,
            )(input),
            2 => nom::combinator::map(
                ApplicationKeyDescriptor::deserialize,
                KeyDescriptor::ApplicationMasterKey,
            )(input),
            3 => nom::combinator::map(
                ApplicationKeyDescriptor::deserialize,
                KeyDescriptor::ApplicationLinkKey,
            )(input),
            4 => nom::combinator::map(
                TrustCenterKeyDescriptor::deserialize,
                KeyDescriptor::UniqueTrustCenterLinkKey,
            )(input),
            5 => nom::combinator::map(
                NetworkKeyDescriptor::deserialize,
                KeyDescriptor::HighSecurityNetworkKey,
            )(input),
            6 => nom::combinator::map(NetworkKeyDescriptor::deserialize, KeyDescriptor::Unknown)(
                input,
            ),
            _ => DeserializeError::unexpected_data(input).into(),
        }
    }
}

#[derive(Eq, PartialEq, Debug)]
pub enum Command {
    TransportKey(KeyDescriptor),
}

impl Serialize for Command {
    fn serialize_to(&self, target: &mut Vec<u8>) -> SerializeResult<()> {
        match self {
            Command::TransportKey(key_descriptor) => (5 as u8, key_descriptor).serialize_to(target),
        }
    }
}

impl Deserialize for Command {
    fn deserialize(input: &[u8]) -> DeserializeResult<Self> {
        let (input, cmd_id) = u8::deserialize(input)?;
        match cmd_id {
            5 => nom::combinator::map(KeyDescriptor::deserialize, Command::TransportKey)(input),
            _ => DeserializeError::unexpected_data(input).into(),
        }
    }
}

#[test]
fn test_decode_transport_key() {
    let serialized = vec![
        0x05, 0x01, 0x41, 0x71, 0x61, 0x72, 0x61, 0x48, 0x75, 0x62, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x06, 0x63, 0x1c, 0xfe, 0xff, 0x5e, 0xcf, 0xd0, 0x15, 0x68, 0x89,
        0x0e, 0x00, 0x4b, 0x12, 0x00,
    ];
    let command = Command::TransportKey(KeyDescriptor::StandardNetworkKey(NetworkKeyDescriptor {
        key: arr![
        u8; 0x41, 0x71, 0x61, 0x72, 0x61, 0x48, 0x75, 0x62, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            ],
        sequence_number: 0,
        destination: ExtendedAddress(0xd0cf5efffe1c6306),
        source: ExtendedAddress(0x00124b000e896815),
    }));
    assert_eq!(command, Command::deserialize_complete(&serialized).unwrap());
    assert_eq!(command.serialize().unwrap(), serialized);
}
