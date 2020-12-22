#![allow(dead_code)]
use async_std::task;
use futures::channel::mpsc;
use futures::prelude::{Sink, Stream};
use futures::stream::StreamExt;
use futures::task::SpawnExt;
mod async_std_executor;
mod ieee802154;
mod pack;
mod radio;
mod unique_key;
use futures::select;
use ieee802154::mac;
use ieee802154::{ShortAddress, PANID};
use libc;
use pcap;
use radio::{RadioPacket, RadioResponse};
use std::os::unix::io::{FromRawFd, IntoRawFd};
use std::sync::Mutex;

#[derive(Debug)]
enum MainloopInput {
    MlmeConfirm(mac::mlme::Confirm),
    MlmeIndication(mac::mlme::Indication),
}
async fn mainloop(
    mlme_requests: Box<dyn Sink<mac::mlme::Request, Error = mpsc::SendError> + Unpin + Send>,
    mlme_confirms: Box<dyn Stream<Item = mac::mlme::Confirm> + Unpin + Send>,
    mlme_indications: Box<dyn Stream<Item = mac::mlme::Indication> + Unpin + Send>,
) {
    let mut mlme_confirms = mlme_confirms.fuse();
    let mut mlme_indications = mlme_indications.fuse();
    while let Some(input) = select! {
        x = mlme_confirms.next() => x.map(MainloopInput::MlmeConfirm),
        x = mlme_indications.next() => x.map(MainloopInput::MlmeIndication),
    } {
        println!("Mainloop input: {:?}", input);
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
    let capture = Mutex::new(capture);
    /*let savefile = capture.savefile("test.pcap").unwrap();
    let savefile = Mutex::new(savefile);
    */

    /*
    let packet_data = vec![0x03, 0x08, 0xa5, 0xff, 0xff, 0xff, 0xff, 0x07, 0xc4, 0xeb];
    drop(savefile);
    */

    let radio_responses = radio_responses.map(move |response| {
        if let RadioResponse::OnPacket(packet) = &response {
            println!("Writing debug packet");
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

    let (mlme_requests_in, mlme_requests_out) = mpsc::unbounded();
    let (mlme_confirms_in, mlme_confirms_out) = mpsc::unbounded();
    let (mlme_indications_in, mlme_indications_out) = mpsc::unbounded();
    println!("Done?");
    exec.spawn(mac::service::start(
        mac::service::MacConfig {
            channel: 25,
            short_address: ShortAddress(0),
            pan_id: PANID(0x1234),
        },
        Box::new(radio_requests),
        Box::new(radio_responses),
        Box::new(mlme_requests_out),
        Box::new(mlme_confirms_in),
        Box::new(mlme_indications_in),
    ))
    .unwrap();
    exec.spawn(mainloop(
        Box::new(mlme_requests_in),
        Box::new(mlme_confirms_out),
        Box::new(mlme_indications_out),
    ))
    .unwrap();
    task::block_on(exec);
}
