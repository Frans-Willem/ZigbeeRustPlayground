#![allow(dead_code)]
use async_std::task;
use futures::prelude::{Sink, Stream};
use futures::sink::SinkExt;
use futures::stream::StreamExt;
mod radio;
mod tokenmap;
use futures::io::AsyncReadExt;
use std::os::unix::io::{FromRawFd, IntoRawFd};
use tokenmap::TokenMap;

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
    let mut map = TokenMap::new();
    let token = map.insert(());
    println!("Assigned token: {:?}", token);
    radio_requests
        .send(radio::RadioRequest::GetParam(
            Some(token),
            param,
            param_type,
        ))
        .await
        .unwrap_or(());
    loop {
        println!("Getting response?");
        if let Some(radio::RadioResponse::GetParam(Some(token), _, result)) =
            radio_responses.next().await
        {
            if let Some(_) = map.remove(token) {
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
        radio::RadioParam::TxPowerMax,
        radio::RadioParamType::U16,
    )
    .await
    .unwrap();
    println!("Maximum TX power: {:?}", max_tx_power);
}

struct AsyncStdSpawner();

impl futures::task::Spawn for AsyncStdSpawner {
    fn spawn_obj(
        &self,
        future: futures::task::FutureObj<'static, ()>,
    ) -> Result<(), futures::task::SpawnError> {
        async_std::task::spawn(future);
        Ok(())
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
    let (radio_requests, radio_responses) = radio::start_radio(AsyncStdSpawner(), portin, portout);
    println!("Done?");
    task::block_on(async_main(radio_requests, radio_responses));
}
