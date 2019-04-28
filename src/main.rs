#![feature(futures_api, async_await, await_macro, trait_alias)]
#![allow(dead_code)]
extern crate bitfield;
extern crate bytes;
extern crate tokio;
extern crate tokio_serial;
#[macro_use]
extern crate enum_tryfrom_derive;
extern crate enum_tryfrom;
#[macro_use]
extern crate futures;

#[macro_use]
mod parse_serialize;
mod cachemap;
mod delayqueue;
mod ieee802154;
mod radio_bridge;
use futures::compat::*;

use futures::task::{Spawn, SpawnExt};
use futures::{FutureExt, StreamExt, TryFutureExt};
use ieee802154::mac::service::Event as MACEvent;
use ieee802154::mac::service::Service as MACService;
use tokio::codec::Decoder;
use tokio::prelude::Future as _;
use tokio::prelude::Stream as _;
use tokio::runtime::Runtime;

use bytes::Bytes;

pub trait CloneSpawn: Spawn + Send + Sync {
    fn clone(&self) -> Box<CloneSpawn>;
}

impl<T: futures::task::Spawn + Clone + Send + Sync + 'static> CloneSpawn for T {
    fn clone(&self) -> Box<CloneSpawn> {
        Box::new(self.clone())
    }
}

fn on_mac_event(handle: &mut Box<CloneSpawn>, service: &MACService, event: MACEvent) {
    eprintln!("MAC event: {:?}", event);
    match event {
        MACEvent::BeaconRequest() => {
            let payload =
                Bytes::from(&b"\x00\x22\x84\x15\x68\x89\x0e\x00\x4b\x12\x00\xff\xff\xff\x00"[..]);
            println!("Sending beacon!");
            handle
                .spawn(service.send_beacon(payload).map(|res| {
                    println!("Sent beacon: {:?}", res);
                }))
                .unwrap();
        }
    }
}

struct MySpawner(tokio::runtime::TaskExecutor);

impl Spawn for MySpawner {
    fn spawn_obj(
        &mut self,
        fut: futures::future::FutureObj<'static, ()>,
    ) -> Result<(), futures::task::SpawnError> {
        let fut = fut.unit_error().boxed().compat();
        self.0.spawn(fut);
        Ok(())
    }
}

impl Clone for MySpawner {
    fn clone(&self) -> Self {
        MySpawner(self.0.clone())
    }
}

fn main() {
    let rt = Runtime::new().unwrap();
    let mut spawner: Box<CloneSpawn> = Box::new(MySpawner(rt.executor()));
    let settings = tokio_serial::SerialPortSettings::default();
    let port = tokio_serial::Serial::from_path("/dev/ttyACM0", &settings).unwrap();
    let (output_sink, output_stream) = radio_bridge::serial_protocol::Codec::new()
        .framed(port)
        .split();

    let output_sink = Box::new(output_sink.sink_compat());
    let output_stream = Box::new(output_stream.compat());
    let (service, incoming_packets) =
        radio_bridge::service::RadioBridgeService::new(output_sink, output_stream, &mut spawner);
    let service = MACService::new(
        spawner.clone(),
        service,
        Box::new(incoming_packets),
        25,
        ieee802154::ShortAddress(0),
        ieee802154::PANID(12345),
    );

    let service = service.map_err(|e| eprintln!("Unable to start MAC service: {:?}", e));

    let mut service_spawner = spawner.clone();
    let service = service
        .and_then(move |(macservice, macevents)| {
            macevents
                .for_each(move |event| {
                    futures::future::ready(on_mac_event(&mut service_spawner, &macservice, event))
                })
                .map(|x| Ok(x))
        })
        .map(|x| x.unwrap());
    spawner.spawn(service).unwrap();

    rt.shutdown_on_idle().wait().unwrap();
}
