extern crate bitfield;
extern crate bytes;
extern crate tokio;
extern crate tokio_core;
extern crate tokio_io;
extern crate tokio_serial;
extern crate tokio_service;
extern crate tokio_sync;
#[macro_use]
extern crate enum_tryfrom_derive;
extern crate enum_tryfrom;

#[macro_use]
mod parse_serialize;
mod ieee802154;
mod radio_bridge;

use bytes::{Buf, Bytes, IntoBuf};
use parse_serialize::ParseFromBuf;
use std::sync::Arc;
use tokio::prelude::{Future, Stream};
use tokio_core::reactor::Core;

fn on_packet(packet: Bytes) {
    println!("<< {:?}", packet);
    match ieee802154::MACFrame::parse_from_buf(&mut packet.into_buf()) {
        Ok(x) => println!("== PARSED: {:?}", x),
        Err(e) => println!("!! Unable to parse {:?}", e),
    }
}

fn main() {
    let settings = tokio_serial::SerialPortSettings::default();
    let port = tokio_serial::Serial::from_path("/dev/ttyACM0", &settings).unwrap();
    let mut core = Core::new().unwrap();

    let (service, packet_stream) =
        radio_bridge::service::RadioBridgeService::new(port, core.handle());

    let packet_handler = packet_stream
        .for_each(|pkt| {
            on_packet(pkt.packet);
            Ok(())
        })
        .map_err(|e| eprintln!("{:?}", e));
    let service = Arc::new(service);

    let setup_response = service
        .set_channel(11)
        .join(service.set_rx_mode(0))
        .join(service.on())
        .and_then(|_| Ok(println!("Setup complete")))
        .map_err(|e| eprintln!("{:?}", e));
    core.handle().spawn(setup_response);

    core.run(packet_handler).unwrap();
}
