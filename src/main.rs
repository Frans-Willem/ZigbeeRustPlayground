use async_std::fs::OpenOptions;
use async_std::io::prelude::*;
use async_std::io::{Read, Write};
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
use radio::*;

fn main() {
    println!("Hello world!");
    let port = Arc::new(
        task::block_on(OpenOptions::new().read(true).write(true).open(
            "/dev/serial/by-id/usb-Texas_Instruments_CC2531_USB_Dongle_00124B000E896815-if00",
        ))
        .unwrap(),
    );
    let mut sink = RawRadioSink::new(port.as_ref());
    let mut stream = RawRadioStream::new(port.as_ref());
    println!("Creating request");
    let request = RawRadioMessage {
        command_id: 6, // GetValue
        request_id: 123,
        data: (16 as u16).to_be_bytes().as_ref().into(),
    };
    task::block_on(sink.send(request)).unwrap();
    println!("Written!");
    loop {
        println!("Reading");
        // let message_read = task::block_on(stream.next());
				let mut buffer = [0; 32];
			let len = 
				task::block_on(port.as_ref().read(&mut buffer)).unwrap();
println!("{} {:?}", len, buffer);
    }
    /*
     * b'ZPB'
     * u8 command_id
     * u16 request_id (big endian)
     * u16 length (big endian)
     * [u8; length] data
     */
}
