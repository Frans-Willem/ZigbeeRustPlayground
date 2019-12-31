#![allow(dead_code)]
use async_std::task;
use futures::prelude::{Sink, Stream};
use futures::sink::SinkExt;
use futures::stream::StreamExt;
use futures::task::SpawnExt;
mod async_std_executor;
mod parse_serialize;
mod radio;
mod unique_key;
use futures::io::AsyncReadExt;
use std::os::unix::io::{FromRawFd, IntoRawFd};
use unique_key::UniqueKey;

/**
 * Quickly gets a parameter from the radio,
 * ignoring all other responses received before the get-response.
 */
async fn radio_get_param<
    RQ: Sink<radio::RadioRequest> + Unpin,
    RS: Stream<Item = radio::RadioResponse> + Unpin,
>(
    radio_requests: &mut RQ,
    radio_responses: &mut RS,
    param: radio::RadioParam,
    param_type: radio::RadioParamType,
) -> Result<radio::RadioParamValue, radio::RadioError> {
    let token = UniqueKey::new();
    radio_requests
        .send(radio::RadioRequest::GetParam(token, param, param_type))
        .await
        .unwrap_or(());
    loop {
        if let Some(radio::RadioResponse::GetParam(response_token, _, result)) =
            radio_responses.next().await
        {
            if token == response_token {
                return result;
            }
        }
    }
}

async fn async_main<
    RQ: Sink<radio::RadioRequest> + Unpin,
    RS: Stream<Item = radio::RadioResponse> + Unpin,
>(
    mut radio_requests: RQ,
    mut radio_responses: RS,
) {
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
}

fn main() {
    println!("Hello world!");
    let port = serialport::posix::TTYPort::open(
        std::path::Path::new(
            "/dev/serial/by-id/usb-Texas_Instruments_CC2531_USB_Dongle_00124B000E896815-if00",
        ),
        &serialport::SerialPortSettings::default(),
    )
    .unwrap();
    let port = unsafe { async_std::fs::File::from_raw_fd(port.into_raw_fd()) };

    let (portin, portout) = port.split();
    let exec = async_std_executor::AsyncStdExecutor::new();
    let (radio_requests, radio_responses) = radio::start_radio(exec.clone(), portin, portout);
    println!("Done?");
    exec.spawn(async_main(radio_requests, radio_responses))
        .unwrap();
    task::block_on(exec);
}
