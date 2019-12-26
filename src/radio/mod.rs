use cookie_factory::SerializeFn;
use futures::prelude::*;
use futures::ready;
use pin_project::pin_project;
use std::convert::TryInto;
use std::io::Write;
use std::pin::Pin;
use std::task::{Context, Poll};

static RADIO_MAGIC_PREFIX: &[u8] = b"ZPB";

#[derive(Debug)]
pub struct RawRadioMessage {
    pub command_id: u8,
    pub request_id: u16,
    pub data: Vec<u8>,
}

pub fn gen_raw_radio_message<'a, W: Write + 'a>(
    msg: &'a RawRadioMessage,
) -> impl SerializeFn<W> + 'a {
    let len = if msg.data.len() > u16::max_value() as usize {
        u16::max_value()
    } else {
        msg.data.len() as u16
    };
    cookie_factory::sequence::tuple((
        cookie_factory::combinator::slice(RADIO_MAGIC_PREFIX),
        cookie_factory::bytes::be_u8(msg.command_id),
        cookie_factory::bytes::be_u16(msg.request_id),
        cookie_factory::bytes::be_u16(len),
        cookie_factory::combinator::slice(&msg.data[0..len as usize]),
    ))
}

impl Into<Vec<u8>> for RawRadioMessage {
    fn into(self) -> Vec<u8> {
        let mut res = Vec::new();
        cookie_factory::gen(gen_raw_radio_message(&self), &mut res);
        res
    }
}

pub fn parse_raw_radio_message(input: &[u8]) -> nom::IResult<&[u8], RawRadioMessage> {
    let (input, (_, command_id, request_id, data_len)) = nom::sequence::tuple((
        nom::bytes::streaming::tag(RADIO_MAGIC_PREFIX),
        nom::number::streaming::be_u8,
        nom::number::streaming::be_u16,
        nom::number::streaming::be_u16,
    ))(input)?;
    let (input, data) = nom::bytes::streaming::take(data_len as usize)(input)?;
    Ok((
        input,
        RawRadioMessage {
            command_id,
            request_id,
            data: data.into(),
        },
    ))
}

#[pin_project]
pub struct RawRadioSink<T: AsyncWrite> {
    #[pin]
    target: T,
    buffer: Vec<u8>,
    written: usize,
}

impl<T: AsyncWrite> RawRadioSink<T> {
    pub fn new(target: T) -> RawRadioSink<T> {
        RawRadioSink {
            target,
            buffer: Vec::new(),
            written: 0,
        }
    }
}

impl<T: AsyncWrite> Sink<RawRadioMessage> for RawRadioSink<T> {
    type Error = async_std::io::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        let mut this = self.project();
        while *this.written < this.buffer.len() {
            match ready!(this
                .target
                .as_mut()
                .poll_write(cx, &this.buffer[*this.written..]))
            {
                Ok(num_sent) => *this.written += num_sent,
                Err(e) => return Poll::Ready(Err(e)),
            }
        }
        this.buffer.clear();
        *this.written = 0;
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: RawRadioMessage) -> Result<(), Self::Error> {
        let this = self.project();
        this.buffer.append(&mut item.into());
        Ok(())
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        if let Err(e) = ready!(self.as_mut().poll_ready(cx)) {
            Poll::Ready(Err(e))
        } else {
            let this = self.project();
            this.target.poll_flush(cx)
        }
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        if let Err(e) = ready!(self.as_mut().poll_ready(cx)) {
            Poll::Ready(Err(e))
        } else {
            let this = self.project();
            this.target.poll_close(cx)
        }
    }
}

#[pin_project]
pub struct RawRadioStream<T: AsyncRead> {
    #[pin]
    source: T,
    buffer: [u8; u16::max_value() as usize],
    buffer_filled: usize,
}

fn find_subsequence<T>(haystack: &[T], needle: &[T]) -> Option<usize>
where
    for<'a> &'a [T]: PartialEq,
{
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
impl<T: AsyncRead> RawRadioStream<T> {
    pub fn new(source: T) -> RawRadioStream<T> {
        RawRadioStream {
            source,
            buffer: [0; u16::max_value() as usize],
            buffer_filled: 0,
        }
    }
}

fn pop_raw_message(buffer: &mut [u8]) -> (usize, Option<RawRadioMessage>) {
    // Find prefix, if found, remove all before
    // If not found, only keep enough bytes in the buffer so we don't miss the tag next time.
    if buffer.len() < RADIO_MAGIC_PREFIX.len() {
        return (buffer.len(), None);
    }
    let buffer = match find_subsequence(buffer, RADIO_MAGIC_PREFIX) {
        None => {
            buffer.rotate_right(RADIO_MAGIC_PREFIX.len());
            return (RADIO_MAGIC_PREFIX.len(), None);
        }
        Some(index) => {
            buffer.rotate_left(index);
            let new_len = buffer.len() - index;
            &mut buffer[0..new_len]
        }
    };
    match parse_raw_radio_message(buffer) {
        Ok((remaining, message)) => {
            let remaining_len = remaining.len();
            buffer.rotate_right(remaining_len);
            (remaining_len, Some(message))
        }
        Err(nom::Err::Incomplete(_)) => (buffer.len(), None),
        Err(e) => panic!("{:?}", e),
    }
}

impl<T: AsyncRead> Stream for RawRadioStream<T> {
    type Item = RawRadioMessage;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        loop {
            let (new_length, message) = pop_raw_message(&mut this.buffer[0..*this.buffer_filled]);
            *this.buffer_filled = new_length;
            if let Some(message) = message {
                return Poll::Ready(Some(message));
            }
            let target_slice = &mut this.buffer[*this.buffer_filled..];
            if target_slice.len() == 0 {
                return Poll::Ready(None);
            }
            match ready!(this.source.as_mut().poll_read(cx, target_slice)) {
                Ok(read) => {
                    *this.buffer_filled += read;
                }
                Err(e) => return Poll::Ready(None),
            }
        }
    }
}
