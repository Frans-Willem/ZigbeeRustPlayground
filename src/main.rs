#![allow(dead_code)]
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
extern crate futures;

#[macro_use]
mod parse_serialize;
mod ackmap;
mod cachemap;
mod ieee802154;
mod radio_bridge;

use bytes::Bytes;
use ieee802154::mac::service::Event as MACEvent;
use ieee802154::mac::service::Service as MACService;
use tokio::prelude::{Future, Stream};
use tokio_core::reactor::{Core, Handle};

fn on_mac_event(handle: &Handle, service: &MACService, event: MACEvent) -> Result<(), ()> {
    eprintln!("MAC event: {:?}", event);
    match event {
        MACEvent::BeaconRequest() => {
            let payload =
                Bytes::from(&b"\x00\x22\x84\x15\x68\x89\x0e\x00\x4b\x12\x00\xff\xff\xff\x00"[..]);
            println!("Sending beacon!");
            handle.spawn(service.send_beacon(payload).then(|res| {
                println!("Sent beacon: {:?}", res);
                Ok(())
            }));
        }
    }
    Ok(())
}

fn main() {
    let settings = tokio_serial::SerialPortSettings::default();
    let port = tokio_serial::Serial::from_path("/dev/ttyACM0", &settings).unwrap();
    let mut core = Core::new().unwrap();

    let (service, packet_stream) =
        radio_bridge::service::RadioBridgeService::new(port, core.handle());

    let service = MACService::new(
        core.handle(),
        service,
        Box::new(packet_stream),
        25,
        ieee802154::ShortAddress(0),
        ieee802154::PANID(12345),
    );

    let handle = core.handle();
    let service = service.map_err(|e| eprintln!("Unable to start MAC service: {:?}", e));
    let service = service.and_then(move |(macservice, macevents)| {
        macevents.for_each(move |event| on_mac_event(&handle, &macservice, event))
    });

    core.run(service).unwrap();
}
