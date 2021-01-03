#![allow(dead_code)]
use async_std::task;
use futures::channel::mpsc;
use futures::sink::{Sink, SinkExt};
use futures::stream::{Stream, StreamExt};
use futures::task::SpawnExt;
mod async_std_executor;
mod delay_queue;
mod ieee802154;
mod pack;
mod radio;
mod unique_key;
mod waker_store;
use futures::{future, select};
use ieee802154::frame;
use ieee802154::mac;
use ieee802154::pib::PIBProperty;
use ieee802154::services::{mcps, mlme};
use ieee802154::{ShortAddress, PANID};

use radio::{RadioRequest, RadioResponse};
use std::os::unix::io::{FromRawFd, IntoRawFd};
use std::sync::{Arc, Mutex};

async fn send_request(
    mlme_input: &mut (dyn Sink<mlme::Input, Error = mpsc::SendError> + Unpin + Send),
    request: mlme::Request,
) {
    mlme_input
        .send(mlme::Input::Request(request))
        .await
        .unwrap();
}
async fn send_response(
    mlme_input: &mut (dyn Sink<mlme::Input, Error = mpsc::SendError> + Unpin + Send),
    response: mlme::Response,
) {
    mlme_input
        .send(mlme::Input::Response(response))
        .await
        .unwrap();
}
async fn send_mcps_request(
    mcps_input: &mut (dyn Sink<mcps::Input, Error = mpsc::SendError> + Unpin + Send),
    request: mcps::Request,
) {
    mcps_input
        .send(mcps::Input::Request(request))
        .await
        .unwrap();
}

#[derive(Debug)]
enum MainloopInput {
    Mlme(mlme::Output),
    Mcps(mcps::Output),
}

/**
 * Normal startup described in 6.3.3.1 of 802.15.4-2015:
 * - MLME-RESET with SetDefaultPIB = TRUE
 * - MLME-START with PanCoordinator set to TRUE and CoordRealignment set to FALSE
 */

async fn mainloop(
    mut mlme_input: Box<dyn Sink<mlme::Input, Error = mpsc::SendError> + Unpin + Send>,
    mlme_output: Box<dyn Stream<Item = mlme::Output> + Unpin + Send>,
    mut mcps_input: Box<dyn Sink<mcps::Input, Error = mpsc::SendError> + Unpin + Send>,
    mcps_output: Box<dyn Stream<Item = mcps::Output> + Unpin + Send>,
) {
    send_request(
        mlme_input.as_mut(),
        mlme::Request::Reset(mlme::ResetRequest {
            set_default_pib: true,
        }),
    )
    .await;
    send_request(
        mlme_input.as_mut(),
        mlme::Request::Set(mlme::SetRequest {
            attribute: PIBProperty::PhyCurrentChannel,
            value: 25_u16.into(),
        }),
    )
    .await;
    send_request(
        mlme_input.as_mut(),
        mlme::Request::Set(mlme::SetRequest {
            attribute: PIBProperty::MacAssociationPermit,
            value: true.into(),
        }),
    )
    .await;
    send_request(
        mlme_input.as_mut(),
        mlme::Request::Set(mlme::SetRequest {
            attribute: PIBProperty::MacShortAddress,
            value: ShortAddress(0x0000).into(),
        }),
    )
    .await;
    send_request(
        mlme_input.as_mut(),
        mlme::Request::Set(mlme::SetRequest {
            attribute: PIBProperty::MacBeaconPayload,
            value: vec![
                0x00, 0x22, 0x84, 0x15, 0x68, 0x89, 0x0e, 0x00, 0x4b, 0x12, 0x00, 0xFF, 0xFF, 0xFF,
                0x00,
            ]
            .into(),
        }),
    )
    .await;
    send_request(
        mlme_input.as_mut(),
        mlme::Request::Set(mlme::SetRequest {
            attribute: PIBProperty::MacBeaconAutoRespond,
            value: true.into(),
        }),
    )
    .await;
    send_request(
        mlme_input.as_mut(),
        mlme::Request::Start(mlme::StartRequest {
            pan_id: PANID(0x1234),
            channel_number: 25,
            channel_page: 0,
            start_time: 0,
            beacon_order: 15,
            superframe_order: 15,
            pan_coordinator: true,
            battery_life_extension: false,
        }),
    )
    .await;
    let mut mlme_output = mlme_output.fuse();
    let mut mcps_output = mcps_output.fuse();
    while let Some(input) = select! {
        x = mlme_output.next() => x.map(MainloopInput::Mlme),
        x = mcps_output.next() => x.map(MainloopInput::Mcps),
    } {
        match input {
            MainloopInput::Mlme(mlme::Output::Indication(mlme::Indication::BeaconRequest {
                beacon_type,
                src_addr: _,
                dst_pan_id: _,
            })) => {
                println!("Beacon request!");
                let request = mlme::BeaconRequest {
                    beacon_type,
                    channel: 25,
                    channel_page: 0,
                    superframe_order: 15,
                    dst_addr: None,
                };
                send_request(mlme_input.as_mut(), mlme::Request::Beacon(request)).await;
            }
            MainloopInput::Mlme(mlme::Output::Indication(mlme::Indication::Associate {
                device_address,
                capability_information,
            })) => {
                let address = ShortAddress(0x4567);
                let mut data = Vec::new();
                let mut nwk_header = vec![
                    0x48, 0x00, // FCF
                    0x67, 0x45, // Destination
                    0x00, 0x00, // Source
                    0x1E, // Radius (30),
                    0x28, // Sequence
                ];
                let mut aps_header = vec![
                    0x21, 0x06, 0x10, 0x01, 0x00, 0x00, 0x00, 0xe3, 0xbd, 0x18, 0x74, 0x09, 0x2c,
                    0x2c, 0xa3, 0x58, 0x1d, 0x8a, 0x23, 0xb9, 0x6c, 0x3b, 0x80, 0xf0, 0xad, 0x27,
                    0x1c, 0x59, 0x8a, 0xdf, 0x27, 0xbc, 0x21, 0xc7, 0x47, 0xf0, 0x31, 0x74, 0x80,
                    0xbc, 0x8c, 0x53, 0x88, 0x11, 0x8f, 0x02,
                ];
                data.append(&mut nwk_header);
                data.append(&mut aps_header);
                send_response(
                    mlme_input.as_mut(),
                    mlme::Response::Associate {
                        device_address,
                        fast_association: capability_information.fast_association,
                        status: Ok(Some(address)),
                    },
                )
                .await;
                send_mcps_request(
                    mcps_input.as_mut(),
                    mcps::Request::Data(mcps::DataRequest {
                        source_addressing_mode: frame::AddressingMode::Short,
                        destination: Some(frame::FullAddress {
                            pan_id: PANID(0x1234),
                            address: address.into(),
                        }),
                        msdu: data,
                        msdu_handle: mcps::MsduHandle::new(),
                        ack_tx: true,
                        indirect_tx: true,
                    }),
                )
                .await
            }
            input => println!("Mainloop unhandled input: {:?}", input),
        }
    }
}

fn main() {
    println!("Hello world!");
    let portin = serialport::TTYPort::open(&serialport::new(
        "/dev/serial/by-id/usb-Texas_Instruments_CC2531_USB_Dongle_00124B000E896815-if00",
        115200,
    ))
    .unwrap();
    let portout = portin.try_clone_native().unwrap();
    let portin = unsafe { async_std::fs::File::from_raw_fd(portin.into_raw_fd()) };
    let portout = unsafe { async_std::fs::File::from_raw_fd(portout.into_raw_fd()) };

    let exec = async_std_executor::AsyncStdExecutor::new();
    let (radio_requests, radio_responses) = radio::start_radio(exec.clone(), portin, portout);

    let capture = pcap::Capture::dead(pcap::Linktype(195)).unwrap();
    let capture = Arc::new(Mutex::new(capture));
    let capture2 = capture.clone();

    let radio_responses = radio_responses.map(move |response| {
        if let RadioResponse::OnPacket(packet) = &response {
            let mut packet_data = packet.data.clone();
            packet_data.push(packet.rssi);
            packet_data.push(packet.link_quality | 0x80);
            let header = pcap::PacketHeader {
                ts: libc::timeval {
                    tv_sec: 0,
                    tv_usec: 0,
                },
                caplen: packet_data.len() as u32,
                len: packet_data.len() as u32,
            };
            let packet = pcap::Packet {
                header: &header,
                data: &packet_data,
            };
            let mut savefile = capture
                .lock()
                .unwrap()
                .savefile_append("test.pcap")
                .unwrap();
            savefile.write(&packet);
        }
        response
    });
    let capture = capture2;

    let radio_requests = radio_requests.with(move |request| {
        if let RadioRequest::SendPacket(_token, packet) = &request {
            let mut packet_data = packet.clone();
            packet_data.push(0);
            packet_data.push(0x80);
            let header = pcap::PacketHeader {
                ts: libc::timeval {
                    tv_sec: 0,
                    tv_usec: 0,
                },
                caplen: packet_data.len() as u32,
                len: packet_data.len() as u32,
            };
            let packet = pcap::Packet {
                header: &header,
                data: &packet_data,
            };
            let mut savefile = capture
                .lock()
                .unwrap()
                .savefile_append("test.pcap")
                .unwrap();
            savefile.write(&packet);
        }
        future::ready(Ok(request))
    });

    let (mlme_input_in, mlme_input_out) = mpsc::unbounded();
    let (mlme_output_in, mlme_output_out) = mpsc::unbounded();
    let (mcps_input_in, mcps_input_out) = mpsc::unbounded();
    let (mcps_output_in, mcps_output_out) = mpsc::unbounded();
    println!("Done?");
    exec.spawn(mac::service::start(
        Box::pin(radio_requests),
        Box::pin(radio_responses),
        Box::pin(mlme_input_out),
        Box::pin(mlme_output_in),
        Box::pin(mcps_input_out),
        Box::pin(mcps_output_in),
    ))
    .unwrap();
    exec.spawn(mainloop(
        Box::new(mlme_input_in),
        Box::new(mlme_output_out),
        Box::new(mcps_input_in),
        Box::new(mcps_output_out),
    ))
    .unwrap();
    task::block_on(exec);
}
