extern crate bytes;
extern crate tokio;
extern crate tokio_core;
extern crate tokio_io;
extern crate tokio_serial;
extern crate tokio_service;
extern crate tokio_sync;

mod radio_bridge;

use bytes::{BufMut, BytesMut};
use tokio::prelude::{Future, Stream};
use tokio_core::reactor::Core;
use tokio_service::Service;

fn main() {
    let settings = tokio_serial::SerialPortSettings::default();
    let port = tokio_serial::Serial::from_path("/dev/ttyACM0", &settings).unwrap();
    let mut core = Core::new().unwrap();

    let (service, packet_stream) = radio_bridge::raw_service::RadioBridgeService::new(port, core.handle());

    core.handle().spawn(packet_stream.for_each(|pkt| {
        println!("Got a packet of length {}", pkt.packet.len());
        Ok(())
    }).map_err(|e| eprintln!("{:?}", e)));

    let mut data = BytesMut::new();
    data.put_u16_be(11);
    data.put_u16_be(8);
    let response = service.call(radio_bridge::raw_service::Request {
    command_id: radio_bridge::raw_service::RequestCommand::RadioGetObject,
    data: data.freeze(),
    });
    let response = core.run(response).unwrap();
    println!("Got ID {:?}", response);
}
