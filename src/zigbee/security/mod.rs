pub mod cbcmac;
pub mod ccmstar;
pub mod mmohash;

use crate::ieee802154::ExtendedAddress;
use crate::parse_serialize;
use crate::parse_serialize::{
    ParseFromBuf, ParseFromBufEx, ParseFromBufTagged, Result as ParseResult, SerializeToBuf,
    SerializeToBufEx, SerializeToBufTagged,
};
use aead::Aead;
use bitfield::bitfield;
use block_cipher_trait::BlockCipher;
use bytes::{Buf, BufMut, Bytes};
use crypto_mac::Mac;
use std::convert::TryFrom;
use std::result::Result;

#[derive(Clone, Copy)]
pub enum KeyIdentifier {
    Data,
    Network(u8), // = 1
    KeyTransport,
    KeyLoad,
}

pub struct KeyStore {
    data: Option<[u8; 16]>,
    network: Option<[u8; 16]>,
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

impl SerializeToBuf for KeyIdentifier {
    fn serialize_to_buf(&self, buf: &mut BufMut) -> ParseResult<()> {
        match self {
            KeyIdentifier::Network(key_sequence_number) => {
                key_sequence_number.serialize_to_buf(buf)
            }
            _ => Ok(()),
        }
    }
}

impl SerializeToBufTagged<u8> for KeyIdentifier {
    fn get_serialize_tag(&self) -> ParseResult<u8> {
        Ok(match self {
            KeyIdentifier::Data => 0,
            KeyIdentifier::Network(_) => 1,
            KeyIdentifier::KeyTransport => 2,
            KeyIdentifier::KeyLoad => 3,
        })
    }
}

impl ParseFromBufTagged<u8> for KeyIdentifier {
    fn parse_from_buf(tag: u8, buf: &mut Buf) -> ParseResult<Self> {
        match tag {
            0 => Ok(KeyIdentifier::Data),
            1 => Ok(KeyIdentifier::Network(u8::parse_from_buf(buf)?)),
            2 => Ok(KeyIdentifier::KeyTransport),
            3 => Ok(KeyIdentifier::KeyLoad),
            _ => Err(parse_serialize::Error::UnexpectedData),
        }
    }
}

pub struct SecuredData {
    key_identifier: KeyIdentifier,
    frame_counter: u32,
    extended_source: Option<ExtendedAddress>,
    payload: Bytes,
}

bitfield! {
    pub struct SecurityControl(u8);
    impl Debug;
    /* Security level is always set to 0 on the air */
    pub security_level, set_security_level: 2, 0;
    pub key_identifier, set_key_identifier: 4, 3;
    pub extended_nonce, set_extended_nonce: 5, 5;
    pub reserved, set_reserved: 7, 6;
}
default_parse_serialize_newtype!(SecurityControl, u8);

impl SerializeToBuf for SecuredData {
    fn serialize_to_buf(&self, buf: &mut BufMut) -> ParseResult<()> {
        let mut sc = SecurityControl(0);
        sc.set_security_level(0); // Always set as 0 on air.
        sc.set_key_identifier(self.key_identifier.get_serialize_tag()?);
        sc.set_extended_nonce(self.extended_source.is_some() as u8);
        sc.set_reserved(0);
        sc.serialize_to_buf(buf)?;
        self.frame_counter.serialize_to_buf(buf)?;
        if let Some(source) = self.extended_source {
            source.serialize_to_buf(buf)?;
        }
        self.key_identifier.serialize_to_buf(buf)?;
        self.payload.serialize_to_buf(buf)?;
        Ok(())
    }
}

impl ParseFromBuf for SecuredData {
    fn parse_from_buf(buf: &mut Buf) -> ParseResult<Self> {
        let sc = SecurityControl::parse_from_buf(buf)?;
        if sc.security_level() != 0 {
            // On the wire, this should always be set to 0
            return Err(parse_serialize::Error::UnexpectedData);
        }
        let frame_counter = u32::parse_from_buf(buf)?;
        let extended_source = if sc.extended_nonce() != 0 {
            Some(ExtendedAddress::parse_from_buf(buf)?)
        } else {
            None
        };
        let key_identifier = KeyIdentifier::parse_from_buf(sc.key_identifier(), buf)?;
        let payload = Bytes::from(buf.bytes());
        buf.advance(payload.len());
        Ok(SecuredData {
            key_identifier,
            frame_counter,
            extended_source,
            payload,
        })
    }
}

fn generate_encryption_key(key_identifier: KeyIdentifier, store: &KeyStore) -> Option<[u8; 16]> {
    match key_identifier {
        KeyIdentifier::Data => store.data,
        KeyIdentifier::Network(_) => {
            // Not implemented yet
            unimplemented!()
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
    let mut nonce = std::io::Cursor::new([0; 15]);
    source_address.serialize_to_buf(&mut nonce).ok()?;
    nonce.put_u32_le(frame_counter);
    let mut sc = SecurityControl(0);
    sc.set_security_level(security_level.into());
    sc.set_key_identifier(key_identifier.get_serialize_tag().ok()?);
    sc.set_extended_nonce(extended_nonce as u8);
    sc.set_reserved(0);
    nonce.put_u8(sc.0);
    Some(nonce.into_inner())
}

fn generate_associated_data(
    key_identifier: KeyIdentifier,
    frame_counter: u32,
    extended_source: Option<ExtendedAddress>,
    security_level: SecurityLevel,
    buf: &mut BufMut,
) -> ParseResult<()> {
    // This function is slightly different from the serialize_to_buf,
    // as in this case the security level *is* serialized.
    let mut sc = SecurityControl(0);
    sc.set_security_level(security_level.into());
    sc.set_key_identifier(key_identifier.get_serialize_tag()?);
    sc.set_extended_nonce(extended_source.is_some() as u8);
    sc.set_reserved(0);
    sc.serialize_to_buf(buf)?;
    frame_counter.serialize_to_buf(buf)?;
    if let Some(source) = extended_source {
        source.serialize_to_buf(buf)?;
    }
    key_identifier.serialize_to_buf(buf)?;
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
        buf: &mut BufMut,
    ) -> ParseResult<()> {
        generate_associated_data(
            self.key_identifier,
            self.frame_counter,
            self.extended_source,
            security_level,
            buf,
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
            let message = self.payload.slice_to(tag_start);
            let tag = self.payload.slice_from(tag_start);
            associated_data.put(message.as_ref());
            ccmstar
                .decrypt(
                    &nonce.into(),
                    aead::Payload {
                        msg: &tag,
                        aad: &associated_data,
                    },
                )
                .ok()?;
            Some(message.as_ref().into())
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
            associated_data.put(&plaintext);
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
            payload.put(&tag);
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
        network: None,
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
    let parsed = SecuredData::parse_from_vec(&secured_frame).unwrap();
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
    let recrypted = recrypted.serialize_as_vec().unwrap();
    assert_eq!(recrypted, secured_frame);
}
