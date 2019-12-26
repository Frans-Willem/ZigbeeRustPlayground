use futures::prelude::*;
use futures::ready;
use pin_project::pin_project;
use std::convert::TryInto;
use std::pin::Pin;
use std::task::{Context, Poll};

static RADIO_MAGIC_PREFIX: &[u8] = b"ZPB";
#[derive(Debug)]
pub struct RawRadioMessage {
    pub command_id: u8,
    pub request_id: u16,
    pub data: Vec<u8>,
}

impl Into<Vec<u8>> for RawRadioMessage {
    fn into(self) -> Vec<u8> {
        let mut res = Vec::new();
        res.extend_from_slice(RADIO_MAGIC_PREFIX);
        res.push(self.command_id);
        res.extend_from_slice(&self.request_id.to_be_bytes());
        let length: u16 = if self.data.len() > u16::max_value() as usize {
            u16::max_value()
        } else {
            self.data.len() as u16
        };
        res.extend_from_slice(&length.to_be_bytes());
        res.extend_from_slice(&self.data[0..length as usize]);
        res
    }
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
        println!("Start send?");
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
println!("pop_raw_message: {:?}", buffer);
    if buffer.len() < RADIO_MAGIC_PREFIX.len() {
println!("Too short");
        return (buffer.len(), None);
    }
    // Find prefix, if found, remove all before
    // If not found, only keep last x characters in buffer.
    let buffer = match find_subsequence(buffer, RADIO_MAGIC_PREFIX) {
        None => {
						println!("No prefix found");
            buffer.rotate_right(RADIO_MAGIC_PREFIX.len());
            return (RADIO_MAGIC_PREFIX.len(), None);
        }
        Some(index) => {
						println!("Prefix found at {}", index);
            buffer.rotate_left(index);
            let new_len = buffer.len() - index;
            &mut buffer[0..new_len]
        }
    };
		println!("Buffer now: {:?}", buffer);
    if buffer.len() < RADIO_MAGIC_PREFIX.len() + 1 + 2 + 2 {
				println!("Too short");
        return (buffer.len(), None);
    }
    let command_id = buffer[RADIO_MAGIC_PREFIX.len()];
    let request_id = u16::from_be_bytes(
        buffer[RADIO_MAGIC_PREFIX.len() + 1..RADIO_MAGIC_PREFIX.len() + 3]
            .try_into()
            .unwrap(),
    );
    let data_length = u16::from_be_bytes(
        buffer[RADIO_MAGIC_PREFIX.len() + 3..RADIO_MAGIC_PREFIX.len() + 5]
            .try_into()
            .unwrap(),
    ) as usize;
    let message_end = RADIO_MAGIC_PREFIX.len() + 1 + 2 + 2 + data_length;
		println!("Decoded: {} {} {} {}", command_id, request_id, data_length, message_end);
    if buffer.len() < message_end {
println!("Too short");
        return (buffer.len(), None);
    }
    let data: Vec<u8> = buffer[RADIO_MAGIC_PREFIX.len() + 5..message_end].into();
    buffer.rotate_left(message_end);
		println!("New buffer {:?}", buffer);
    (
        buffer.len() - message_end,
        Some(RawRadioMessage {
            command_id,
            request_id,
            data,
        }),
    )
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
println!("Poll_read {}", target_slice.len());
            match ready!(this.source.as_mut().poll_read(cx, target_slice)) {
                Ok(read) => {
println!("Read {} bytes", read);
*this.buffer_filled += read;
},
                Err(e) => return Poll::Ready(None),
            }
        }
    }
}
