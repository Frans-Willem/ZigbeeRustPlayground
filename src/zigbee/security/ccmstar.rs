use super::cbcmac::CbcMac;
use aead::{Aead, Payload};
use aes::Aes128;
use bitfield::bitfield;
use block_cipher_trait::BlockCipher;
use bytes::BufMut;
use crypto_mac::{Mac, MacResult};
use ctr::Ctr128;
use generic_array::typenum::{Unsigned, U16};
use generic_array::{arr, sequence::GenericSequence, ArrayLength, GenericArray};
use stream_cipher::SyncStreamCipher;
use subtle::ConstantTimeEq;

bitfield! {
    struct AuthDataFlag(u8);
    impl Debug;
    pub reserved, set_reserved: 7, 7;
    pub a_data, set_a_data: 6, 6;
    pub m, set_m: 5,3;
    pub l, set_l: 2,0;
}

bitfield! {
    struct NonceFlag(u8);
    impl Debug;
    pub reserved, set_reserved: 7, 6;
    pub zero, set_zero: 5, 3;
    pub l, set_l: 2, 0;
}

#[derive(Copy, Clone, Debug)]
pub enum CcmStarIntegrityCodeLen {
    None = 0,
    MIC4 = 1,
    MIC6 = 2,
    MIC8 = 3,
    MIC10 = 4,
    MIC12 = 5,
    MIC14 = 6,
    MIC15 = 7,
}

#[derive(Copy, Clone, Debug)]
pub enum CcmStarLengthSize {
    Len16 = 2,
    Len24 = 3,
    Len32 = 4,
    Len40 = 5,
    Len48 = 6,
    Len56 = 7,
    Len64 = 8,
}

pub struct CcmStar<C: BlockCipher> {
    cipher: C,
    integrity_code_len: CcmStarIntegrityCodeLen,
    length_size: CcmStarLengthSize,
}

impl<C: BlockCipher<BlockSize = U16> + Clone> CcmStar<C>
where
    C::ParBlocks: ArrayLength<GenericArray<u8, U16>>, // <-- this because Ctr128 requires it, I don't really see a reason for it.
{
    //type KeySize = <Aes128 as BlockCipher>::KeySize;
    pub fn from_cipher(
        cipher: C,
        integrity_code_len: CcmStarIntegrityCodeLen,
        length_size: CcmStarLengthSize,
    ) -> Self {
        CcmStar {
            cipher,
            integrity_code_len,
            length_size,
        }
    }

    fn calculate_tag<'msg, 'aad>(
        &self,
        plaintext: &Payload<'msg, 'aad>,
        nonce: &[u8],
    ) -> Result<MacResult<C::BlockSize>, aead::Error> {
        let mut auth_data = vec![];
        if plaintext.aad.len() == 0 {
            // No tag, don't do anything
        } else if plaintext.aad.len() < 0xFF00 {
            auth_data.put_u16_be(plaintext.aad.len() as u16);
        } else if plaintext.aad.len() <= std::u32::MAX as usize {
            auth_data.put_u16_be(0xFFFE);
            auth_data.put_u32_be(plaintext.aad.len() as u32);
        } else if plaintext.aad.len() <= std::u64::MAX as usize {
            auth_data.put_u16_be(0xFFFF);
            auth_data.put_u64_be(plaintext.aad.len() as u64);
        } else {
            return Err(aead::Error);
        }
        auth_data.put(plaintext.aad);
        while auth_data.len() % 16 != 0 {
            auth_data.put_u8(0);
        }
        auth_data.put(plaintext.msg);

        let mut auth_data_flag = AuthDataFlag(0);
        auth_data_flag.set_reserved(0);
        auth_data_flag.set_a_data((plaintext.aad.len() > 0) as u8);
        auth_data_flag.set_m(self.integrity_code_len as u8);
        auth_data_flag.set_l((self.length_size as u8) - 1); // L-1
        let mut auth_data_b0 = vec![];
        auth_data_b0.put_u8(auth_data_flag.0);
        auth_data_b0.put(nonce);
        // Check that plaintext fits in L bytes
        let length_size = self.length_size as usize;
        if (plaintext.msg.len() >> (length_size * 8)) != 0 {
            return Err(aead::Error);
        }
        for i in 0..length_size {
            auth_data_b0
                .put_u8(((plaintext.msg.len() >> ((length_size - i - 1) * 8)) & 0xFF) as u8);
        }
        if auth_data_b0.len() != C::BlockSize::to_usize() {
            return Err(aead::Error);
        }

        let mut mac = CbcMac::from_cipher(self.cipher.clone());
        mac.input(&auth_data_b0);
        mac.input(&auth_data);
        Ok(mac.result())
    }

    fn create_ctr(&self, nonce: &[u8]) -> Ctr128<C> {
        let mut counter_nonce = GenericArray::generate(|_| 0);
        let mut nonce_flag = NonceFlag(0);
        nonce_flag.set_reserved(0);
        nonce_flag.set_zero(0);
        nonce_flag.set_l((self.length_size as u8) - 1);
        counter_nonce[0] = nonce_flag.0;
        for i in 0..nonce.len() {
            counter_nonce[i + 1] = nonce[i]
        }

        Ctr128::from_cipher(self.cipher.clone(), &counter_nonce)
    }

    fn get_tag_len(&self) -> usize {
        if self.integrity_code_len as usize == 0 {
            0
        } else {
            ((self.integrity_code_len as usize) * 2 + 2)
        }
    }
}

impl<C: BlockCipher<BlockSize = U16> + Clone> Aead for CcmStar<C>
where
    C::ParBlocks: ArrayLength<GenericArray<u8, U16>>, // <-- this because Ctr128 requires it, I don't really see a reason for it.
{
    type NonceSize = generic_array::typenum::U15;
    type TagSize = generic_array::typenum::U16; // Maximum size of the tag

    type CiphertextOverhead = generic_array::typenum::U0; // TODO: Calculate this!
    fn encrypt<'msg, 'aad>(
        &self,
        nonce: &GenericArray<u8, Self::NonceSize>,
        plaintext: impl Into<Payload<'msg, 'aad>>,
    ) -> Result<Vec<u8>, aead::Error> {
        let nonce = &nonce[0..15 - (self.length_size as usize)]; // Drop a few bytes of nonce
        let plaintext: Payload<'msg, 'aad> = plaintext.into();

        let mut mac = self.calculate_tag(&plaintext, nonce)?.code();
        let mut ctr = self.create_ctr(nonce);
        ctr.apply_keystream(&mut mac);

        let mut output = plaintext.msg.to_vec();
        ctr.apply_keystream(&mut output);
        output.put(&mac[0..self.get_tag_len()]);

        Ok(output)
    }
    fn decrypt<'msg, 'aad>(
        &self,
        nonce: &GenericArray<u8, Self::NonceSize>,
        ciphertext: impl Into<Payload<'msg, 'aad>>,
    ) -> Result<Vec<u8>, aead::Error> {
        let nonce = &nonce[0..15 - (self.length_size as usize)]; // Drop a few bytes of nonce
        let ciphertext: Payload<'msg, 'aad> = ciphertext.into();
        let tag_len = self.get_tag_len();
        if ciphertext.msg.len() < tag_len {
            return Err(aead::Error);
        }
        let mut ctr = self.create_ctr(nonce);
        // First split off the tag
        let msg_len = ciphertext.msg.len() - tag_len;
        let mut tag: GenericArray<u8, C::BlockSize> = GenericArray::generate(|_| 0);
        for i in 0..tag_len {
            tag[i] = ciphertext.msg[msg_len + i];
        }

        ctr.apply_keystream(&mut tag);

        let mut plaintext = ciphertext.msg[0..msg_len].to_vec();
        ctr.apply_keystream(&mut plaintext);

        if tag_len > 0 {
            let mac = self
                .calculate_tag(
                    &Payload {
                        msg: &plaintext,
                        aad: ciphertext.aad,
                    },
                    nonce,
                )?
                .code();
            if !bool::from(mac[0..tag_len].ct_eq(&tag[0..tag_len])) {
                return Err(aead::Error);
            }
        }
        Ok(plaintext)
    }
}

#[test]
fn test_vectors_ccmstar() {
    let key = arr![u8;
        0xC0, 0xC1, 0xC2, 0xC3, 0xC4, 0xC5, 0xC6, 0xC7, 0xC8, 0xC9, 0xCA, 0xCB, 0xCC, 0xCD, 0xCE,
        0xCF,
    ];
    let nonce = arr![u8;
        0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7, 0x03, 0x02, 0x01, 0x00, 0x06, 0x00, 0x00,
    ];
    let m: [u8; 23] = [
        0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16,
        0x17, 0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E,
    ];
    let a: [u8; 8] = [0, 1, 2, 3, 4, 5, 6, 7];

    let checker = CcmStar::from_cipher(
        Aes128::new(&key),
        CcmStarIntegrityCodeLen::MIC8,
        CcmStarLengthSize::Len16,
    );
    let expected_ciphertext = vec![
        0x1A, 0x55, 0xA3, 0x6A, 0xBB, 0x6C, 0x61, 0x0D, 0x06, 0x6B, 0x33, 0x75, 0x64, 0x9C, 0xEF,
        0x10, 0xD4, 0x66, 0x4E, 0xCA, 0xD8, 0x54, 0xA8, 0x0A, 0x89, 0x5C, 0xC1, 0xD8, 0xFF, 0x94,
        0x69,
    ];
    let ciphertext = checker
        .encrypt(&nonce, Payload { msg: &m, aad: &a })
        .unwrap();
    assert_eq!(ciphertext, expected_ciphertext);

    // Check to see if the ciphertext we just got, decrypts to the plaintext we had before.
    let plaintext = checker
        .decrypt(
            &nonce,
            Payload {
                msg: &ciphertext,
                aad: &a,
            },
        )
        .unwrap();
    assert_eq!(plaintext, m);

    // Test to see if mangled ciphertext fails to decode.
    let mut mangled_ciphertext = ciphertext.clone();
    mangled_ciphertext[m.len()] ^= 0x8; // Mangle a bit just after the message length.

    assert_eq!(
        checker.decrypt(
            &nonce,
            Payload {
                msg: &mangled_ciphertext,
                aad: &a
            }
        ),
        Err(aead::Error)
    );
}
