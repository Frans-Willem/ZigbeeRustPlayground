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
use ieee802154::mac;
use ieee802154::pib::PIBProperty;
use ieee802154::services::mlme;
use ieee802154::{ShortAddress, PANID};

use radio::{RadioRequest, RadioResponse};
use std::os::unix::io::{FromRawFd, IntoRawFd};
use std::sync::{Arc, Mutex};

#[derive(Debug)]
enum MainloopInput {
    MlmeConfirm(mlme::Confirm),
    MlmeIndication(mlme::Indication),
}

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

/**
 * Normal startup described in 6.3.3.1 of 802.15.4-2015:
 * - MLME-RESET with SetDefaultPIB = TRUE
 * - MLME-START with PanCoordinator set to TRUE and CoordRealignment set to FALSE
 */

async fn mainloop(
    mut mlme_input: Box<dyn Sink<mlme::Input, Error = mpsc::SendError> + Unpin + Send>,
    mlme_output: Box<dyn Stream<Item = mlme::Output> + Unpin + Send>,
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
    while let Some(input) = select! {
        x = mlme_output.next() => x,
    } {
        match input {
            mlme::Output::Indication(mlme::Indication::BeaconRequest {
                beacon_type,
                src_addr: _,
                dst_pan_id: _,
            }) => {
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
            mlme::Output::Indication(mlme::Indication::Associate {
                device_address,
                capability_information,
            }) => {
                let address = ShortAddress(0x4567);
                send_response(
                    mlme_input.as_mut(),
                    mlme::Response::Associate {
                        device_address,
                        fast_association: capability_information.fast_association,
                        status: Ok(Some(address)),
                    },
                )
                .await;
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
            packet_data.push(0 | 0x80);
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
    println!("Done?");
    exec.spawn(mac::service::start(
        Box::new(radio_requests),
        Box::new(radio_responses),
        Box::new(mlme_input_out),
        Box::new(mlme_output_in),
    ))
    .unwrap();
    exec.spawn(mainloop(Box::new(mlme_input_in), Box::new(mlme_output_out)))
        .unwrap();
    task::block_on(exec);
}
