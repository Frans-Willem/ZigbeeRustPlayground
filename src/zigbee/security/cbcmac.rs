use aes::Aes128;
use block_cipher_trait::BlockCipher;
use crypto_mac::{Mac, MacResult};
use generic_array::{arr, sequence::GenericSequence, GenericArray};
/**
 * Note: an attempt to implement using block_modes::Cbc got overly complex,
 * as Cbc does not implement clone. Thus I've manually implemented the Cbc steps.
 */

#[derive(Clone)]
pub struct CbcMac<C>
where
    C: BlockCipher + Clone,
{
    cipher: C,
    buffer: GenericArray<u8, <C as BlockCipher>::BlockSize>,
    filled: usize, // How many bytes of buffer have been filled.
}

impl<C: BlockCipher + Clone> CbcMac<C> {
    pub fn from_cipher(cipher: C) -> CbcMac<C> {
        CbcMac {
            cipher,
            buffer: GenericArray::generate(|_| 0),
            filled: 0,
        }
    }
}

impl<C: BlockCipher + Clone> Mac for CbcMac<C> {
    type KeySize = <C as BlockCipher>::KeySize;
    type OutputSize = <C as BlockCipher>::BlockSize;

    fn new(key: &GenericArray<u8, <C as BlockCipher>::KeySize>) -> CbcMac<C> {
        CbcMac {
            cipher: C::new(&key),
            buffer: GenericArray::generate(|_| 0),
            filled: 0,
        }
    }
    fn input(&mut self, data: &[u8]) {
        for i in 0..data.len() {
            self.buffer[self.filled] ^= data[i];
            self.filled += 1;
            if self.filled == self.buffer.len() {
                self.cipher.encrypt_block(&mut self.buffer);
                self.filled = 0;
            }
        }
    }
    fn reset(&mut self) {
        self.filled = 0;
        for i in 0..self.buffer.len() {
            self.buffer[i] = 0;
        }
    }
    fn result(self) -> MacResult<Self::OutputSize> {
        let mut buffer = self.buffer;
        if self.filled > 0 {
            // Theoretically, we should XOR the remaining bytes with the padding,
            // But seeing as the padding is 0, there's nothing to do.
            self.cipher.encrypt_block(&mut buffer);
        }
        MacResult::new(buffer)
    }
}

#[test]
fn test_vectors_cbc_mac() {
    let key = arr![u8;
        0xC0, 0xC1, 0xC2, 0xC3, 0xC4, 0xC5, 0xC6, 0xC7, 0xC8, 0xC9, 0xCA, 0xCB, 0xCC, 0xCD, 0xCE,
        0xCF,
    ];
    let data: [u8; 64] = [
        0x59, 0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7, 0x03, 0x02, 0x01, 0x00, 0x06, 0x00,
        0x17, 0x00, 0x08, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10, 0x11, 0x12, 0x13, 0x14,
        0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00,
    ];
    let expected_mac = arr![u8;
        0xB9, 0xD7, 0x89, 0x67, 0x04, 0xBC, 0xFA, 0x20, 0xB2, 0x10, 0x36, 0x74, 0x45, 0xF9, 0x83,
        0xD6,
    ];
    let mut mac: CbcMac<Aes128> = CbcMac::new(&key);
    mac.input(&data);
    let result = mac.result();
    assert!(result.eq(&MacResult::new(expected_mac)));
}
