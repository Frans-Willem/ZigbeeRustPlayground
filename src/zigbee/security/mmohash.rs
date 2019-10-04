use aes::Aes128;
use block_cipher_trait::BlockCipher;
use crypto_mac::Mac;
use digest::{FixedOutput, Input};
use generic_array::{sequence::GenericSequence, ArrayLength, GenericArray};
use hmac::Hmac;
use std::marker::PhantomData;

/**
 * An implementation of the Matyas-Meyer-Oseas hash function.
 */
#[derive(Clone)]
pub struct MMOHash<C, BlockSize>
where
    BlockSize: ArrayLength<u8>,
    C: BlockCipher<KeySize = BlockSize, BlockSize = BlockSize>,
    C::ParBlocks: ArrayLength<GenericArray<u8, BlockSize>>,
{
    hash: GenericArray<u8, BlockSize>,
    buffer: GenericArray<u8, BlockSize>,
    filled: usize,
    length: usize,
    phantom: PhantomData<C>,
    phantom2: PhantomData<BlockSize>,
}

impl<C, BlockSize> MMOHash<C, BlockSize>
where
    BlockSize: ArrayLength<u8>,
    C: BlockCipher<KeySize = BlockSize, BlockSize = BlockSize>,
    C::ParBlocks: ArrayLength<GenericArray<u8, BlockSize>>,
{
    fn process_block(&mut self) {
        let cipher = C::new(&self.hash);
        for i in 0..BlockSize::to_usize() {
            self.hash[i] = self.buffer[i];
        }
        cipher.encrypt_block(&mut self.hash);
        for i in 0..BlockSize::to_usize() {
            self.hash[i] ^= self.buffer[i];
            self.buffer[i] = 0;
        }
        self.filled = 0;
    }
}

impl<C, BlockSize> Default for MMOHash<C, BlockSize>
where
    BlockSize: ArrayLength<u8>,
    C: BlockCipher<KeySize = BlockSize, BlockSize = BlockSize>,
    C::ParBlocks: ArrayLength<GenericArray<u8, BlockSize>>,
{
    fn default() -> MMOHash<C, BlockSize> {
        MMOHash {
            hash: GenericArray::generate(|_| 0),
            buffer: GenericArray::generate(|_| 0),
            filled: 0,
            length: 0,
            phantom: PhantomData,
            phantom2: PhantomData,
        }
    }
}

impl<C, BlockSize> digest::Reset for MMOHash<C, BlockSize>
where
    BlockSize: ArrayLength<u8>,
    C: BlockCipher<KeySize = BlockSize, BlockSize = BlockSize>,
    C::ParBlocks: ArrayLength<GenericArray<u8, BlockSize>>,
{
    fn reset(&mut self) {
        for i in 0..BlockSize::to_usize() {
            self.hash[i] = 0;
            self.buffer[i] = 0;
        }
        self.filled = 0;
        self.length = 0;
    }
}

impl<C, BlockSize> digest::Input for MMOHash<C, BlockSize>
where
    BlockSize: ArrayLength<u8>,
    C: BlockCipher<KeySize = BlockSize, BlockSize = BlockSize>,
    C::ParBlocks: ArrayLength<GenericArray<u8, BlockSize>>,
{
    fn input<B: AsRef<[u8]>>(&mut self, data: B) {
        let data = data.as_ref();
        for i in 0..data.len() {
            self.buffer[self.filled] = data[i];
            self.filled += 1;
            if self.filled == BlockSize::to_usize() {
                self.process_block();
            }
        }
        self.length += data.len();
    }
}

impl<C, BlockSize> digest::BlockInput for MMOHash<C, BlockSize>
where
    BlockSize: ArrayLength<u8>,
    C: BlockCipher<KeySize = BlockSize, BlockSize = BlockSize>,
    C::ParBlocks: ArrayLength<GenericArray<u8, BlockSize>>,
{
    type BlockSize = BlockSize;
}

impl<C, BlockSize> digest::FixedOutput for MMOHash<C, BlockSize>
where
    BlockSize: ArrayLength<u8>,
    C: BlockCipher<KeySize = BlockSize, BlockSize = BlockSize>,
    C::ParBlocks: ArrayLength<GenericArray<u8, BlockSize>>,
{
    type OutputSize = BlockSize;

    fn fixed_result(mut self) -> GenericArray<u8, Self::OutputSize> {
        // Allocate some buffer space of twice the blocksize
        let mut padding_buffer = vec![0; BlockSize::to_usize() * 2];
        padding_buffer[0] = 0x80; // First write in the first 1-bit of padding
                                  // Message length in bits
        let length_in_bits = self.length * 8;
        // Calculate if it fits in blocksize (16) bits or double blocksize (32) bits
        let fits_small_suffix = (length_in_bits >> BlockSize::to_usize()) == 0;
        let fits_big_suffix = (length_in_bits >> 2 * BlockSize::to_usize()) == 0;
        assert!(fits_big_suffix); // If this doesn't fit, panic!

        // One big clusterfuck. We need to pad with a 1 bit, then a couple of 0 bits, and then
        // either a small suffix of BlockSize bits of length, or a big suffix of Blocksize*2 of
        // length + Blocksize 0 bits.
        // The end of this padding should coincide with the end of a block.
        let padding_bits_required = 1 + if fits_small_suffix {
            BlockSize::to_usize()
        } else {
            BlockSize::to_usize() * 3
        };
        let padding_bytes_required = (padding_bits_required + 7) / 8;
        let mut padding_bytes = BlockSize::to_usize() - self.filled;
        if padding_bytes < padding_bytes_required {
            padding_bytes += BlockSize::to_usize();
        }
        let mut shift = (padding_bytes as isize) * 8;
        if !fits_small_suffix {
            shift -= 8;
        }
        for i in 0..padding_bytes {
            shift -= 8;
            if shift < (std::mem::size_of::<usize>() * 8) as isize && shift > -8 {
                padding_buffer[i] |= ((length_in_bits >> shift) & 0xFF) as u8;
            }
        }
        self.input(&padding_buffer[0..padding_bytes]);
        assert_eq!(self.filled, 0);
        self.hash
    }
}

#[test]
fn test_cryptographic_hash() {
    let m = vec![0xC0];
    let mut digest: MMOHash<Aes128, _> = MMOHash::default();
    digest.input(&m);
    let digest = digest.fixed_result();
    assert_eq!(
        digest,
        arr![u8; 0xAE, 0x3A, 0x10, 0x2A, 0x28, 0xD4, 0x3E, 0xE0, 0xD4, 0xA0, 0x9E, 0x22, 0x78, 0x8B, 0x20, 0x6C]
    );

    let m = vec![
        0xC0, 0xC1, 0xC2, 0xC3, 0xC4, 0xC5, 0xC6, 0xC7, 0xC8, 0xC9, 0xCA, 0xCB, 0xCC, 0xCD, 0xCE,
        0xCF,
    ];
    let mut digest: MMOHash<Aes128, _> = MMOHash::default();
    digest.input(&m);
    let digest = digest.fixed_result();
    assert_eq!(
        digest,
        arr![u8; 0xA7, 0x97, 0x7E, 0x88, 0xBC, 0x0B, 0x61, 0xE8, 0x21, 0x08, 0x27, 0x10, 0x9A, 0x22, 0x8F, 0x2D]
    );
    let mut m = vec![0; 8191];
    for i in 0..m.len() {
        m[i] = (i & 0xFF) as u8;
    }
    let mut digest: MMOHash<Aes128, _> = MMOHash::default();
    digest.input(&m);
    let digest = digest.fixed_result();
    assert_eq!(
        digest,
        arr![u8; 0x24, 0xEC, 0x2F, 0xE7, 0x5B, 0xBF, 0xFC, 0xB3, 0x47, 0x89, 0xBC, 0x06, 0x10, 0xE7, 0xF1, 0x65]
    );
}

#[test]
fn test_keyed_hash() {
    let key = arr![u8; 0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4A, 0x4B, 0x4C, 0x4D, 0x4E, 0x4F];
    let m = vec![0xC0];
    let mut mac: Hmac<MMOHash<Aes128, _>> = Hmac::new(&key);
    mac.input(&m);
    let mac = mac.result().code();
    assert_eq!(
        mac,
        arr![u8; 0x45, 0x12, 0x80, 0x7B, 0xF9, 0x4C, 0xB3, 0x40, 0x0F, 0x0E, 0x2C, 0x25, 0xFB, 0x76, 0xE9, 0x99]
    );
}
