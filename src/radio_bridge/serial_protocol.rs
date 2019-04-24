use bytes::{BigEndian, BufMut, ByteOrder, Bytes, BytesMut};
use std::io;
use tokio::codec::{Decoder, Encoder};

#[derive(Debug)]
pub struct Command {
    pub command_id: u8,
    pub request_id: u16,
    pub data: Bytes,
}

enum DecoderState {
    WaitingForMagic,
    WaitingForCommandId,
    WaitingForRequestId {
        command_id: u8,
    },
    WaitingForLength {
        command_id: u8,
        request_id: u16,
    },
    WaitingForData {
        command_id: u8,
        request_id: u16,
        length: usize,
    },
}

pub struct Codec {
    decoder_state: DecoderState,
}

impl Codec {
    pub fn new() -> Codec {
        Codec {
            decoder_state: DecoderState::WaitingForMagic,
        }
    }
}

static MAGIC_PREFIX: &[u8] = b"ZPB";

fn find_subsequence<T>(haystack: &[T], needle: &[T]) -> Option<usize>
where
    for<'a> &'a [T]: PartialEq,
{
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

impl Decoder for Codec {
    type Item = Command;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let mut keep_going = true;
        let mut retval: Option<Self::Item> = None;
        while retval.is_none() && keep_going && src.len() > 0 {
            self.decoder_state = match self.decoder_state {
                DecoderState::WaitingForMagic => {
                    if let Some(magic_offset) = find_subsequence(src, MAGIC_PREFIX) {
                        src.advance(magic_offset + MAGIC_PREFIX.len());
                        DecoderState::WaitingForCommandId
                    } else {
                        src.advance(src.len() - MAGIC_PREFIX.len());
                        keep_going = false;
                        DecoderState::WaitingForMagic
                    }
                }
                DecoderState::WaitingForCommandId => {
                    if src.len() < 1 {
                        keep_going = false;
                        DecoderState::WaitingForCommandId
                    } else {
                        let command_id = src[0];
                        src.advance(1);
                        DecoderState::WaitingForRequestId {
                            command_id: command_id,
                        }
                    }
                }
                DecoderState::WaitingForRequestId { command_id } => {
                    if src.len() < 2 {
                        keep_going = false;
                        DecoderState::WaitingForRequestId {
                            command_id: command_id,
                        }
                    } else {
                        let request_id = BigEndian::read_u16(src);
                        src.advance(2);
                        DecoderState::WaitingForLength {
                            command_id: command_id,
                            request_id: request_id,
                        }
                    }
                }
                DecoderState::WaitingForLength {
                    command_id,
                    request_id,
                } => {
                    if src.len() < 2 {
                        keep_going = false;
                        DecoderState::WaitingForLength {
                            command_id: command_id,
                            request_id: request_id,
                        }
                    } else {
                        let length = BigEndian::read_u16(src) as usize;
                        src.advance(2);
                        DecoderState::WaitingForData {
                            command_id: command_id,
                            request_id: request_id,
                            length: length,
                        }
                    }
                }
                DecoderState::WaitingForData {
                    command_id,
                    request_id,
                    length,
                } => {
                    if src.len() < length {
                        keep_going = false;
                        DecoderState::WaitingForData {
                            command_id: command_id,
                            request_id: request_id,
                            length: length,
                        }
                    } else {
                        retval = Some(Command {
                            command_id: command_id,
                            request_id: request_id,
                            data: src.split_to(length).freeze(),
                        });
                        DecoderState::WaitingForMagic
                    }
                }
            }
        }
        Ok(retval)
    }
}

impl Encoder for Codec {
    type Item = Command;
    type Error = io::Error;
    fn encode(&mut self, item: Self::Item, dst: &mut BytesMut) -> Result<(), Self::Error> {
        dst.extend_from_slice(MAGIC_PREFIX);
        dst.put_u8(item.command_id);
        dst.put_u16_be(item.request_id);
        let length = if item.data.len() > 0xFFFF {
            0xFFFF
        } else {
            item.data.len() as u16
        };
        dst.put_u16_be(length);
        dst.extend_from_slice(&item.data.as_ref()[0..length as usize]);
        Ok(())
    }
}
