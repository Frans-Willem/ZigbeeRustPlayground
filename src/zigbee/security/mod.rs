pub mod cbcmac;
pub mod ccmstar;
pub mod mmohash;

use crate::ieee802154::ExtendedAddress;
use crate::parse_serialize::{
    Deserialize, DeserializeError, DeserializeResult, DeserializeTagged, Serialize,
    SerializeResult, SerializeTagged,
};
use aead::Aead;
use bitfield::bitfield;
use block_cipher_trait::BlockCipher;
use crypto_mac::Mac;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::convert::TryInto;
use std::result::Result;

#[derive(Eq, PartialEq, Debug)]
pub enum Securable<T> {
    Secured(SecuredData),
    Unsecured(T),
}

impl<T> Securable<T> {
    pub fn is_secured(&self) -> bool {
        match self {
            Securable::Secured(_) => true,
            Securable::Unsecured(_) => false,
        }
    }
}

impl<T: Serialize> SerializeTagged for Securable<T> {
    type TagType = bool;

    fn serialize_tag(&self) -> SerializeResult<bool> {
        Ok(self.is_secured())
    }
    fn serialize_data_to(&self, target: &mut Vec<u8>) -> SerializeResult<()> {
        match self {
            Securable::Secured(data) => data.serialize_to(target),
            Securable::Unsecured(data) => data.serialize_to(target),
        }
    }
}

impl<T: Deserialize> DeserializeTagged for Securable<T> {
    type TagType = bool;

    fn deserialize(tag: bool, input: &[u8]) -> DeserializeResult<Securable<T>> {
        match tag {
            true => nom::combinator::map(SecuredData::deserialize, Securable::Secured)(input),
            false => nom::combinator::map(T::deserialize, Securable::Unsecured)(input),
        }
    }
}

/**
 * Secured data, but serialization depends on a tag that is stored unsecured.
 */
#[derive(Eq, PartialEq, Debug)]
pub enum SecurableTagged<TagType: Copy, T> {
    Secured(TagType, SecuredData),
    Unsecured(T),
}

impl<TagType, T> SerializeTagged for SecurableTagged<TagType, T>
where
    TagType: Copy,
    T: SerializeTagged<TagType = TagType>,
{
    type TagType = (bool, TagType);

    fn serialize_tag(&self) -> SerializeResult<Self::TagType> {
        Ok(match self {
            SecurableTagged::Secured(tag, _) => (true, *tag),
            SecurableTagged::Unsecured(data) => (false, data.serialize_tag()?),
        })
    }
    fn serialize_data_to(&self, target: &mut Vec<u8>) -> SerializeResult<()> {
        match self {
            SecurableTagged::Secured(_, data) => data.serialize_to(target),
            SecurableTagged::Unsecured(data) => SerializeTagged::serialize_data_to(data, target),
        }
    }
}

impl<TagType, T> DeserializeTagged for SecurableTagged<TagType, T>
where
    TagType: Copy,
    T: DeserializeTagged<TagType = TagType>,
{
    type TagType = (bool, TagType);
    fn deserialize(tag: (bool, TagType), input: &[u8]) -> DeserializeResult<Self> {
        match tag.0 {
            true => {
                let (input, data) = SecuredData::deserialize(input)?;
                Ok((input, SecurableTagged::Secured(tag.1, data)))
            }
            false => {
                let (input, data) = T::deserialize(tag.1, input)?;
                Ok((input, SecurableTagged::Unsecured(data)))
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum KeyIdentifier {
    Data,
    Network(u8), // = 1
    KeyTransport,
    KeyLoad,
}

pub struct KeyStore {
    data: Option<[u8; 16]>,
    network: HashMap<u8, [u8; 16]>,
    key_transport: Option<[u8; 16]>,
    key_load: Option<[u8; 16]>,
}

#[derive(Clone, Copy, TryFromPrimitive)]
#[TryFromPrimitiveType = "u8"]
pub enum MessageIntegrityCodeLen {
    None = 0,
    MIC32 = 1,
    MIC64 = 2,
    MIC128 = 3,
}

impl Into<ccmstar::CcmStarIntegrityCodeLen> for MessageIntegrityCodeLen {
    fn into(self) -> ccmstar::CcmStarIntegrityCodeLen {
        match self {
            MessageIntegrityCodeLen::None => ccmstar::CcmStarIntegrityCodeLen::None,
            MessageIntegrityCodeLen::MIC32 => ccmstar::CcmStarIntegrityCodeLen::MIC4,
            MessageIntegrityCodeLen::MIC64 => ccmstar::CcmStarIntegrityCodeLen::MIC8,
            MessageIntegrityCodeLen::MIC128 => ccmstar::CcmStarIntegrityCodeLen::MIC16,
        }
    }
}

#[derive(Clone, Copy)]
pub struct SecurityLevel {
    encryption: bool,
    mig_len: MessageIntegrityCodeLen,
}

impl Into<u8> for SecurityLevel {
    fn into(self) -> u8 {
        if self.encryption {
            (self.mig_len as u8) | 4
        } else {
            (self.mig_len as u8)
        }
    }
}

impl SerializeTagged for KeyIdentifier {
    type TagType = u8;
    fn serialize_tag(&self) -> SerializeResult<u8> {
        Ok(match self {
            KeyIdentifier::Data => 0,
            KeyIdentifier::Network(_) => 1,
            KeyIdentifier::KeyTransport => 2,
            KeyIdentifier::KeyLoad => 3,
        })
    }
    fn serialize_data_to(&self, target: &mut Vec<u8>) -> SerializeResult<()> {
        match self {
            KeyIdentifier::Network(key_sequence_number) => key_sequence_number.serialize_to(target),
            _ => Ok(()),
        }
    }
}

impl DeserializeTagged for KeyIdentifier {
    type TagType = u8;
    fn deserialize(tag: u8, input: &[u8]) -> DeserializeResult<Self> {
        match tag {
            0 => Ok((input, KeyIdentifier::Data)),
            1 => nom::combinator::map(u8::deserialize, KeyIdentifier::Network)(input),
            2 => Ok((input, KeyIdentifier::KeyTransport)),
            3 => Ok((input, KeyIdentifier::KeyLoad)),
            _ => DeserializeError::unexpected_data(input).into(),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct SecuredData {
    pub key_identifier: KeyIdentifier,
    pub frame_counter: u32,
    pub extended_source: Option<ExtendedAddress>,
    pub payload: Vec<u8>,
}

bitfield! {
    #[derive(Serialize, Deserialize)]
    pub struct SecurityControl(u8);
    impl Debug;
    /* Security level is always set to 0 on the air */
    pub security_level, set_security_level: 2, 0;
    pub key_identifier, set_key_identifier: 4, 3;
    pub extended_nonce, set_extended_nonce: 5, 5;
    pub reserved, set_reserved: 7, 6;
}

impl Serialize for SecuredData {
    fn serialize_to(&self, target: &mut Vec<u8>) -> SerializeResult<()> {
        let mut sc = SecurityControl(0);
        sc.set_security_level(0); // Always set as 0 on air.
        sc.set_key_identifier(self.key_identifier.serialize_tag()?);
        sc.set_extended_nonce(self.extended_source.is_some() as u8);
        sc.set_reserved(0);
        sc.serialize_to(target)?;
        self.frame_counter.serialize_to(target)?;
        if let Some(source) = self.extended_source {
            source.serialize_to(target)?;
        }
        self.key_identifier.serialize_data_to(target)?;
        target.extend_from_slice(&self.payload);
        Ok(())
    }
}

impl Deserialize for SecuredData {
    fn deserialize(input: &[u8]) -> DeserializeResult<Self> {
        let (input, sc) = SecurityControl::deserialize(input)?;
        if sc.security_level() != 0 {
            // On the wire, this should always be set to 0
            return DeserializeError::unexpected_data(input).into();
        }
        let (input, frame_counter) = u32::deserialize(input)?;
        let (input, extended_source) =
            nom::combinator::cond(sc.extended_nonce() != 0, ExtendedAddress::deserialize)(input)?;
        let (input, key_identifier) = KeyIdentifier::deserialize(sc.key_identifier(), input)?;
        let (input, payload) = nom::combinator::rest(input)?;
        Ok((
            input,
            SecuredData {
                key_identifier,
                frame_counter,
                extended_source,
                payload: payload.to_vec(),
            },
        ))
    }
}

fn generate_encryption_key(key_identifier: KeyIdentifier, store: &KeyStore) -> Option<[u8; 16]> {
    match key_identifier {
        KeyIdentifier::Data => store.data,
        KeyIdentifier::Network(key_sequence_number) => {
            store.network.get(&key_sequence_number).cloned()
        }
        KeyIdentifier::KeyTransport => {
            let mut mac: hmac::Hmac<mmohash::MMOHash<aes::Aes128, _>> =
                hmac::Hmac::new(&store.key_transport?.into());
            mac.input(&[0; 1]);
            Some(mac.result().code().into())
        }
        KeyIdentifier::KeyLoad => {
            let mut mac: hmac::Hmac<mmohash::MMOHash<aes::Aes128, _>> =
                hmac::Hmac::new(&store.key_transport?.into());
            mac.input(&[2; 1]);
            Some(mac.result().code().into())
        }
    }
}

fn generate_nonce(
    key_identifier: KeyIdentifier,
    frame_counter: u32,
    extended_nonce: bool,
    security_level: SecurityLevel,
    source_address: ExtendedAddress,
) -> Option<[u8; 15]> {
    let mut nonce = vec![];
    source_address.serialize_to(&mut nonce).ok()?;
    frame_counter.serialize_to(&mut nonce).ok()?;
    let mut sc = SecurityControl(0);
    sc.set_security_level(security_level.into());
    sc.set_key_identifier(key_identifier.serialize_tag().ok()?);
    sc.set_extended_nonce(extended_nonce as u8);
    sc.set_reserved(0);
    sc.serialize_to(&mut nonce).ok()?;
    nonce.resize(15, 0);
    nonce.as_slice().try_into().ok()
}

fn generate_associated_data(
    key_identifier: KeyIdentifier,
    frame_counter: u32,
    extended_source: Option<ExtendedAddress>,
    security_level: SecurityLevel,
    target: &mut Vec<u8>,
) -> SerializeResult<()> {
    // This function is slightly different from the serialize_to_buf,
    // as in this case the security level *is* serialized.
    let mut sc = SecurityControl(0);
    sc.set_security_level(security_level.into());
    sc.set_key_identifier(key_identifier.serialize_tag()?);
    sc.set_extended_nonce(extended_source.is_some() as u8);
    sc.set_reserved(0);
    sc.serialize_to(target)?;
    frame_counter.serialize_to(target)?;
    if let Some(source) = extended_source {
        source.serialize_to(target)?;
    }
    key_identifier.serialize_data_to(target)?;
    Ok(())
}

impl SecuredData {
    fn generate_nonce(
        &self,
        security_level: SecurityLevel,
        source_address: ExtendedAddress,
    ) -> Option<[u8; 15]> {
        generate_nonce(
            self.key_identifier,
            self.frame_counter,
            self.extended_source.is_some(),
            security_level,
            self.extended_source.unwrap_or(source_address),
        )
    }
    fn generate_associated_data(
        &self,
        security_level: SecurityLevel,
        target: &mut Vec<u8>,
    ) -> SerializeResult<()> {
        generate_associated_data(
            self.key_identifier,
            self.frame_counter,
            self.extended_source,
            security_level,
            target,
        )
    }
    pub fn decrypt(
        &self,
        mut associated_data: Vec<u8>,
        security_level: SecurityLevel,
        source_address: ExtendedAddress,
        store: &KeyStore,
    ) -> Option<Vec<u8>> {
        let key = generate_encryption_key(self.key_identifier, store)?;
        let nonce = self.generate_nonce(security_level, source_address)?;
        let ccmstar = ccmstar::CcmStar::from_cipher(
            aes::Aes128::new(&key.into()),
            security_level.mig_len.into(),
            ccmstar::CcmStarLengthSize::Len16,
        );
        self.generate_associated_data(security_level, &mut associated_data)
            .ok()?;
        if security_level.encryption {
            ccmstar
                .decrypt(
                    &nonce.into(),
                    aead::Payload {
                        msg: &self.payload,
                        aad: &associated_data,
                    },
                )
                .ok()
        } else {
            let tag_size = ccmstar.get_tag_len();
            let payload_len = self.payload.len();
            if payload_len < tag_size {
                return None; // Payload not big enough
            }
            let tag_start = payload_len - tag_size;
            let message = &self.payload[0..tag_start];
            let tag = &self.payload[tag_start..];
            associated_data.extend_from_slice(message);
            ccmstar
                .decrypt(
                    &nonce.into(),
                    aead::Payload {
                        msg: &tag,
                        aad: &associated_data,
                    },
                )
                .ok()?;
            Some(message.into())
        }
    }

    // TODO: Option<> should be changed to a Result<>
    pub fn encrypt(
        plaintext: Vec<u8>,
        mut associated_data: Vec<u8>,
        security_level: SecurityLevel,
        key_identifier: KeyIdentifier,
        frame_counter: u32,
        extended_source: bool,
        source_address: ExtendedAddress,
        store: &KeyStore,
    ) -> Option<SecuredData> {
        let key = generate_encryption_key(key_identifier, store)?;
        let nonce = generate_nonce(
            key_identifier,
            frame_counter,
            extended_source,
            security_level,
            source_address,
        )?;
        let ccmstar = ccmstar::CcmStar::from_cipher(
            aes::Aes128::new(&key.into()),
            security_level.mig_len.into(),
            ccmstar::CcmStarLengthSize::Len16,
        );
        generate_associated_data(
            key_identifier,
            frame_counter,
            if extended_source {
                Some(source_address)
            } else {
                None
            },
            security_level,
            &mut associated_data,
        )
        .ok()?;
        let payload = if security_level.encryption {
            ccmstar
                .encrypt(
                    &nonce.into(),
                    aead::Payload {
                        msg: &plaintext,
                        aad: &associated_data,
                    },
                )
                .ok()?
        } else {
            associated_data.extend_from_slice(&plaintext);
            let empty: [u8; 0] = [];
            let tag = ccmstar
                .encrypt(
                    &nonce.into(),
                    aead::Payload {
                        msg: &empty,
                        aad: &associated_data,
                    },
                )
                .ok()?;
            let mut payload = plaintext.clone();
            payload.extend(tag);
            payload
        };
        Some(SecuredData {
            key_identifier,
            frame_counter,
            extended_source: if extended_source {
                Some(source_address)
            } else {
                None
            },
            payload: payload.into(),
        })
    }
}

#[test]
fn test_decode_transport_key() {
    let keystore = KeyStore {
        data: None,
        network: HashMap::new(),
        key_transport: Some([
            0x5a, 0x69, 0x67, 0x42, 0x65, 0x65, 0x41, 0x6c, 0x6c, 0x69, 0x61, 0x6e, 0x63, 0x65,
            0x30, 0x39,
        ]),
        key_load: None,
    };
    let secured_frame = vec![
        0x10, 0x01, 0x00, 0x00, 0x00, 0xe3, 0xbd, 0x18, 0x74, 0x09, 0x2c, 0x2c, 0xa3, 0x58, 0x1d,
        0x8a, 0x23, 0xb9, 0x6c, 0x3b, 0x80, 0xf0, 0xad, 0x27, 0x1c, 0x59, 0x8a, 0xdf, 0x27, 0xbc,
        0x21, 0xc7, 0x47, 0xf0, 0x31, 0x74, 0x80, 0xbc, 0x8c, 0x53, 0x88, 0x11, 0x8f, 0x02,
    ];
    let parsed = SecuredData::deserialize_complete(&secured_frame).unwrap();
    let header = vec![0x21, 0x06]; // Application Support Layer header
    let source_address = ExtendedAddress(0x00124b000e896815);
    let expected_plaintext = vec![
        0x05, 0x01, 0x41, 0x71, 0x61, 0x72, 0x61, 0x48, 0x75, 0x62, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x06, 0x63, 0x1c, 0xfe, 0xff, 0x5e, 0xcf, 0xd0, 0x15, 0x68, 0x89,
        0x0e, 0x00, 0x4b, 0x12, 0x00,
    ];
    let security_level = SecurityLevel {
        encryption: true,
        mig_len: MessageIntegrityCodeLen::MIC32,
    };
    let decrypted = parsed
        .decrypt(header.clone(), security_level, source_address, &keystore)
        .unwrap();
    assert_eq!(decrypted, expected_plaintext);

    let recrypted = SecuredData::encrypt(
        decrypted,
        header.clone(),
        security_level,
        KeyIdentifier::KeyTransport,
        1,
        false,
        source_address,
        &keystore,
    )
    .unwrap();
    let recrypted = recrypted.serialize().unwrap();
    assert_eq!(recrypted, secured_frame);
}

#[test]
fn test_crypt_device_announcement() {
    let keystore = KeyStore {
        data: None,
        network: [(
            0,
            [
                0x41, 0x71, 0x61, 0x72, 0x61, 0x48, 0x75, 0x62, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00,
            ],
        )]
        .iter()
        .cloned()
        .collect(),
        key_transport: Some([
            0x5a, 0x69, 0x67, 0x42, 0x65, 0x65, 0x41, 0x6c, 0x6c, 0x69, 0x61, 0x6e, 0x63, 0x65,
            0x30, 0x39,
        ]),
        key_load: None,
    };
    let ciphertext = SecuredData {
        key_identifier: KeyIdentifier::Network(0),
        frame_counter: 0,
        extended_source: Some(ExtendedAddress(0xd0cf5efffe1c6306)),
        payload: vec![
            0x6c, 0x41, 0xb1, 0x8d, 0x1c, 0xf1, 0x21, 0xc4, 0x53, 0xc8, 0xd9, 0xcf, 0xa5, 0xf2,
            0xbc, 0x17, 0x9c, 0xfb, 0xee, 0x40, 0x03, 0x78, 0x23, 0x2d,
        ],
    };
    let header = vec![
        0x08, 0x12, 0xfd, 0xff, 0x8b, 0x55, 0x1e, 0xfb, 0x06, 0x63, 0x1c, 0xfe, 0xff, 0x5e, 0xcf,
        0xd0,
    ];
    let source_address = ExtendedAddress(0xd0cf5efffe1c6306);
    let security_level = SecurityLevel {
        encryption: true,
        mig_len: MessageIntegrityCodeLen::MIC32,
    };
    let expected_plaintext = vec![
        0x08, 0x00, 0x13, 0x00, 0x00, 0x00, 0x00, 0x96, 0x81, 0x8b, 0x55, 0x06, 0x63, 0x1c, 0xfe,
        0xff, 0x5e, 0xcf, 0xd0, 0x80,
    ];
    assert_eq!(
        ciphertext
            .decrypt(header.clone(), security_level, source_address, &keystore)
            .unwrap(),
        expected_plaintext
    );
}
