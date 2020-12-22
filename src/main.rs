#![allow(dead_code)]
use async_std::task;
use futures::channel::mpsc;
use futures::prelude::{Sink, Stream};
use futures::task::SpawnExt;
mod async_std_executor;
mod ieee802154;
mod pack;
mod radio;
mod unique_key;
use ieee802154::mac;
use ieee802154::{ShortAddress, PANID};
use std::os::unix::io::{FromRawFd, IntoRawFd};

async fn async_main<
    RQ: Sink<radio::RadioRequest> + Unpin,
    RS: Stream<Item = radio::RadioResponse> + Unpin,
>(
    mut radio_requests: RQ,
    mut radio_responses: RS,
) {
    /*
    println!("Async main go go go!");
    let max_tx_power = radio_get_param(
        &mut radio_requests,
        &mut radio_responses,
        radio::RadioParam::LongAddress,
        radio::RadioParamType::U64,
    )
    .await
    .unwrap();
    if let radio::RadioParamValue::U64(v) = max_tx_power {
        println!("Address: {:X}", v);
    }
    */
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
    task::block_on(exec);
}
