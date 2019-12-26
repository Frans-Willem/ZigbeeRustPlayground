use async_std::fs::OpenOptions;
use async_std::task;
use futures::prelude::Sink;
use futures::ready;
use futures::sink::SinkExt;
use futures::stream::StreamExt;
use pin_project::pin_project;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::vec::Vec;
mod radio;
use futures::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use radio::*;
use std::os::unix::io::{FromRawFd, IntoRawFd};

fn main() {
    println!("Hello world!");
    let port = serialport::posix::TTYPort::open(
        std::path::Path::new(
            "/dev/serial/by-id/usb-Texas_Instruments_CC2531_USB_Dongle_00124B000E896815-if00",
        ),
        &serialport::SerialPortSettings::default(),
    )
    .unwrap();
    let port = unsafe { async_std::fs::File::from_raw_fd(port.into_raw_fd()) };

    let (mut portin, mut portout) = port.split();

    let mut sink = RawRadioSink::new(portout);
    let mut stream = RawRadioStream::new(portin);
    task::block_on(sink.send(RawRadioMessage {
        command_id: 6,
        request_id: 1,
        data: (16 as u16).to_be_bytes().as_ref().into(),
    }))
    .unwrap();
    task::block_on(sink.send(RawRadioMessage {
        command_id: 6,
        request_id: 2,
        data: (15 as u16).to_be_bytes().as_ref().into(),
    }))
    .unwrap();
    println!("Written!");
    loop {
        println!("Reading");
        let message_read = task::block_on(stream.next());
        println!("Message: {:?}", message_read);
        /*
                let mut buffer = [0; 32];
                let len = task::block_on(portin.read(&mut buffer)).unwrap();
                println!("{} {:?}", len, buffer);
        */
    }
    /*
     * b'ZPB'
     * u8 command_id
     * u16 request_id (big endian)
     * u16 length (big endian)
     * [u8; length] data
     */
}
