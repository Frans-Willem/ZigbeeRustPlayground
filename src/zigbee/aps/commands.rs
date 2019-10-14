use crate::ieee802154::ExtendedAddress;
use crate::parse_serialize::{
    Deserialize, DeserializeError, DeserializeResult, Serialize,
};
use generic_array::{typenum::U16, GenericArray};

#[derive(Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct TrustCenterKeyDescriptor {
    key: GenericArray<u8, U16>,
    destination: ExtendedAddress,
    source: ExtendedAddress,
}

#[derive(Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct NetworkKeyDescriptor {
    key: GenericArray<u8, U16>,
    sequence_number: u8,
    destination: ExtendedAddress,
    source: ExtendedAddress,
}

#[derive(Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct ApplicationKeyDescriptor {
    key: GenericArray<u8, U16>,
    partner: ExtendedAddress,
    initiator: bool,
}

#[derive(Eq, PartialEq, Debug, Serialize)]
#[enum_tag_type(u8)]
pub enum KeyDescriptor {
    #[enum_tag(0)]
    TrustCenterMasterKey(TrustCenterKeyDescriptor),
    #[enum_tag(1)]
    StandardNetworkKey(NetworkKeyDescriptor),
    #[enum_tag(2)]
    ApplicationMasterKey(ApplicationKeyDescriptor),
    #[enum_tag(3)]
    ApplicationLinkKey(ApplicationKeyDescriptor),
    #[enum_tag(4)]
    UniqueTrustCenterLinkKey(TrustCenterKeyDescriptor),
    #[enum_tag(5)]
    HighSecurityNetworkKey(NetworkKeyDescriptor),
    #[enum_tag(7)]
    Unknown(NetworkKeyDescriptor),
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

#[derive(Eq, PartialEq, Debug, Serialize)]
#[enum_tag_type(u8)]
pub enum Command {
    #[enum_tag(5)]
    TransportKey(KeyDescriptor),
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
