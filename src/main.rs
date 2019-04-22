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
use radio_bridge::service::{BoxServiceFuture, RadioBridgeService};
use std::ops::Deref;
use std::sync::Arc;
use tokio::prelude::{Future, Stream};
use tokio_core::reactor::{Core, Handle};

fn on_mac_frame(frame: ieee802154::MACFrame, handle: &Handle, service: &RadioBridgeService) {
    println!("== PARSED: {:?}", frame);
    if let Some(acknowledge) = frame.create_ack() {
        let mut buf = vec![];
        acknowledge.serialize_to_buf(&mut buf).unwrap();
        handle.spawn(
            service
                .send(buf.into())
                .and_then(|_| {
                    println!("Ack sent");
                    Ok(())
                })
                .map_err(|e| eprintln!("Send ACK error: {:?}", e)),
        );
    }
    match frame.frame_type {
        ieee802154::MACFrameType::Command(ieee802154::MACCommand::BeaconRequest) => {
            println!("Beacon request?");
            let response = MACFrame {
                acknowledge_request: false,
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
            handle.spawn(
                service
                    .send(buf.into())
                    .and_then(|_| {
                        println!("Sent!");
                        Ok(())
                    })
                    .map_err(|err| eprintln!("Send error: {:?}", err)),
            );
        }
        _ => (),
    }
}

fn on_packet(packet: Bytes, handle: &Handle, service: &RadioBridgeService) {
    match ieee802154::MACFrame::parse_from_buf(&mut packet.clone().into_buf()) {
        Ok(x) => on_mac_frame(x, handle, service),
        Err(e) => println!("!! {:?}, {:?}", packet, e),
    }
}

fn set_max_power(service: &Arc<RadioBridgeService>) -> BoxServiceFuture<()> {
    let service_copy = service.clone();
    Box::new(
        service
            .get_tx_power_max()
            .and_then(move |max_power| service_copy.set_tx_power(max_power)),
    )
}

fn main() {
    let settings = tokio_serial::SerialPortSettings::default();
    let port = tokio_serial::Serial::from_path("/dev/ttyACM0", &settings).unwrap();
    let mut core = Core::new().unwrap();

    let (service, packet_stream) =
        radio_bridge::service::RadioBridgeService::new(port, core.handle());
    let service = Arc::new(service);

    let packet_service = service.clone();
    let packet_handle = core.handle();
    let packet_handler = packet_stream
        .for_each(move |pkt| {
            on_packet(pkt.packet, &packet_handle, packet_service.deref());
            Ok(())
        })
        .map_err(|e| eprintln!("{:?}", e));

    let setup_response = service
        .set_channel(25)
        .join(service.set_rx_mode(radio_bridge::service::RadioRxMode {
            address_filter: false,
            autoack: false,
            poll_mode: false,
        }))
        .join(service.on())
        .join(set_max_power(&service))
        .and_then(|_| Ok(println!("Setup complete")))
        .map_err(|e| eprintln!("{:?}", e));
    core.handle().spawn(setup_response);

    core.run(packet_handler).unwrap();
}
