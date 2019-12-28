use async_std::fs::OpenOptions;
use async_std::task;
use futures::prelude::{Sink, Stream};
use futures::ready;
use futures::sink::SinkExt;
use futures::stream::StreamExt;
use pin_project::pin_project;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::vec::Vec;
mod radio;
mod tokenmap;
use futures::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use futures::task::SpawnExt;
use radio::raw::*;
use std::os::unix::io::{FromRawFd, IntoRawFd};
use tokenmap::{Token, TokenMap};

async fn radio_single_request<
    RQ: Sink<(Token, radio::RadioRequest)> + Unpin,
    RS: Stream<Item = radio::RadioIncoming> + Unpin,
>(
    radio_requests: &mut RQ,
    radio_responses: &mut RS,
    request: radio::RadioRequest,
) -> Result<radio::RadioResponse, radio::RadioError> {
    let mut map = TokenMap::new();
    let token = map.insert(());
    println!("Assigned token: {:?}", token);
    radio_requests.send((token, request)).await;
    loop {
        println!("Getting response?");
        if let Some(radio::RadioIncoming::Response(token, response)) = radio_responses.next().await
        {
            if let Some(_) = map.remove(token) {
                return response;
            }
        }
    }
}

async fn async_main<
    RQ: Sink<(Token, radio::RadioRequest)> + Unpin,
    RS: Stream<Item = radio::RadioIncoming> + Unpin,
>(
    mut radio_requests: RQ,
    mut radio_responses: RS,
) {
    println!("Async main go go go!");
    let max_tx_power = radio_single_request(
        &mut radio_requests,
        &mut radio_responses,
        radio::RadioRequest::GetMaxTxPower,
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

    let (mut portin, mut portout) = port.split();
    //let mut executor = futures::executor::ThreadPool::new().unwrap();
    let (radio_requests, radio_responses) = radio::start_radio(AsyncStdSpawner(), portin, portout);
    /*
        executor
            .spawn(async_main(radio_requests, radio_responses))
            .unwrap();
    */
    println!("Done?");
    //executor.run();
    task::block_on(async_main(radio_requests, radio_responses));
}
