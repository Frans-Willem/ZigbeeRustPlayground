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
mod parse_serialize;
mod ieee802154;
mod radio_bridge;

use bytes::{BufMut, Bytes, IntoBuf};
use ieee802154::*;
use parse_serialize::{ParseFromBuf, SerializeToBuf};
use radio_bridge::service::RadioBridgeService;
use std::ops::Deref;
use std::sync::Arc;
use tokio::prelude::{Future, Stream};
use tokio_core::reactor::Core;

fn on_mac_frame(frame: ieee802154::MACFrame, service: &RadioBridgeService) {
    println!("== PARSED: {:?}", frame);
    match frame.frame_type {
        ieee802154::MACFrameType::Command(ieee802154::MACCommand::BeaconRequest) => {
            println!("Beacon request?");
            let response = MACFrame {
                sequence_number: Some(64),
                destination_pan: None,
                destination: AddressSpecification::None,
                source_pan: PANID(0x7698).into(),
                source: ShortAddress(0).into(),
                frame_type: MACFrameType::Beacon {
                    beacon_order: 15,
                    superframe_order: 15,
                    final_cap_slot: 15,
                    battery_life_extension: false,
                    pan_coordinator: true,
                    association_permit: true,
                },
                payload: Bytes::from(
                    &b"\x00\x22\x84\x15\x68\x89\x0e\x00\x4b\x12\x00\xff\xff\xff\x00"[..],
                ),
            };
            let mut buf = vec![];
            response.serialize_to_buf(&mut buf).unwrap();
            println!("Beacon response: {:?}", buf);
        }
        _ => (),
    }
}

fn on_packet(packet: Bytes, service: &RadioBridgeService) {
    println!("<< {:?}", packet);
    match ieee802154::MACFrame::parse_from_buf(&mut packet.into_buf()) {
        Ok(x) => on_mac_frame(x, service),
        Err(e) => println!("!! Unable to parse {:?}", e),
    }
}

fn main() {
    let settings = tokio_serial::SerialPortSettings::default();
    let port = tokio_serial::Serial::from_path("/dev/ttyACM0", &settings).unwrap();
    let mut core = Core::new().unwrap();

    let (service, packet_stream) =
        radio_bridge::service::RadioBridgeService::new(port, core.handle());
    let service = Arc::new(service);

    let packet_service = service.clone();
    let packet_handler = packet_stream
        .for_each(move |pkt| {
            on_packet(pkt.packet, packet_service.deref());
            Ok(())
        })
        .map_err(|e| eprintln!("{:?}", e));

    let setup_response = service
        .set_channel(25)
        .join(service.set_rx_mode(0))
        .join(service.on())
        .and_then(|_| Ok(println!("Setup complete")))
        .map_err(|e| eprintln!("{:?}", e));
    core.handle().spawn(setup_response);

    core.run(packet_handler).unwrap();
}
