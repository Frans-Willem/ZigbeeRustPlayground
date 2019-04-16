extern crate bytes;
extern crate tokio;
extern crate tokio_core;
extern crate tokio_io;
extern crate tokio_serial;
extern crate tokio_service;
extern crate tokio_sync;

mod radio_bridge;

use std::sync::Arc;
use tokio::prelude::{Future, Stream};
use tokio_core::reactor::Core;

fn main() {
    let settings = tokio_serial::SerialPortSettings::default();
    let port = tokio_serial::Serial::from_path("/dev/ttyACM0", &settings).unwrap();
    let mut core = Core::new().unwrap();

    let (service, packet_stream) =
        radio_bridge::service::RadioBridgeService::new(port, core.handle());

    let packet_handler = packet_stream
        .for_each(|pkt| {
            println!("Got a packet of length {}", pkt.packet.len());
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
