#![feature(futures_api, async_await, await_macro)]
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
//mod delayqueue;
//mod ackmap;
//mod cachemap;
mod radio_bridge;
mod ret_future;
//mod ieee802154;
//mod radio_bridge;
//
use futures::compat::*;
use futures::task::{Spawn, SpawnExt};
use futures::{FutureExt, SinkExt, StreamExt, TryFutureExt};
use tokio::codec::Decoder;
use tokio::prelude::Future as _;
use tokio::prelude::Stream as _;
use tokio::runtime::Runtime;

/*
use bytes::Bytes;
use ieee802154::mac::service::Event as MACEvent;
use ieee802154::mac::service::Service as MACService;
use tokio::prelude::{Future, Stream};
use tokio_core::reactor::{Core, Handle};

fn on_mac_event(handle: &Handle, service: &MACService, event: MACEvent) -> Result<(), ()> {
    eprintln!("MAC event: {:?}", event);
    match event {
        MACEvent::BeaconRequest() => {
            let payload =
                Bytes::from(&b"\x00\x22\x84\x15\x68\x89\x0e\x00\x4b\x12\x00\xff\xff\xff\x00"[..]);
            println!("Sending beacon!");
            handle.spawn(service.send_beacon(payload).then(|res| {
                println!("Sent beacon: {:?}", res);
                Ok(())
            }));
        }
    }
    Ok(())
}

fn main() {
    let settings = tokio_serial::SerialPortSettings::default();
    let port = tokio_serial::Serial::from_path("/dev/ttyACM0", &settings).unwrap();
    let mut core = Core::new().unwrap();

    let (service, packet_stream) =
        radio_bridge::service::RadioBridgeService::new(port, core.handle());

    let service = MACService::new(
        core.handle(),
        service,
        Box::new(packet_stream),
        25,
        ieee802154::ShortAddress(0),
        ieee802154::PANID(12345),
    );

    let handle = core.handle();
    let service = service.map_err(|e| eprintln!("Unable to start MAC service: {:?}", e));
    let service = service.and_then(move |(macservice, macevents)| {
        macevents.for_each(move |event| on_mac_event(&handle, &macservice, event))
    });

    core.run(service).unwrap();
}
*/

struct MySpawner(tokio::runtime::TaskExecutor);

impl futures::task::Spawn for MySpawner {
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

async fn play_with_service(service: radio_bridge::service::RadioBridgeService) {
    println!("Getting extended address");
    let extended_address = await!(service.get_long_address()).unwrap();
    println!("Extended address: {:X}", extended_address);
    println!("Turning on");
    await!(service.on()).unwrap();
    println!(
        "Min channel: {}",
        await!(service.get_channel_min()).unwrap()
    );
    println!(
        "Max channel: {}",
        await!(service.get_channel_max()).unwrap()
    );
}

fn main() {
    let rt = Runtime::new().unwrap();
    let mut spawner = MySpawner(rt.executor());
    let settings = tokio_serial::SerialPortSettings::default();
    let port = tokio_serial::Serial::from_path("/dev/ttyACM0", &settings).unwrap();
    let (output_sink, output_stream) = radio_bridge::serial_protocol::Codec::new()
        .framed(port)
        .split();
    /*
    let output_sink : Box<tokio::prelude::Sink<SinkItem=radio_bridge::serial_protocol::Command, SinkError=std::io::Error>> = Box::new(output_sink);
    */
    let output_sink = Box::new(output_sink.sink_compat());
    let output_stream = Box::new(output_stream.compat());
    let (raw_service, _incoming_packets) =
        radio_bridge::service::RadioBridgeService::new(output_sink, output_stream, &mut spawner);

    spawner.spawn(play_with_service(raw_service)).unwrap();

    rt.shutdown_on_idle().wait().unwrap();
}
