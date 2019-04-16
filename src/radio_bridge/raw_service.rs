use bytes::Bytes;
use futures::sync::{mpsc, oneshot};
use futures::{Future, Sink, Stream};
use std::collections::HashMap;
use std::io;
use std::sync::{Arc, RwLock};
use tokio::codec::Decoder;
use tokio_core::reactor::Handle;
use tokio_serial::Serial;
use tokio_service::Service;

use crate::radio_bridge::serial_protocol;

pub enum RequestCommand {
    RadioPrepare = 0,
    RadioTransmit,
    RadioSend,
    RadioChannelClear,
    RadioOn,
    RadioOff,
    RadioGetValue,
    RadioSetValue,
    RadioGetObject,
    RadioSetObject,
}

pub struct Request {
    pub command_id: RequestCommand,
    pub data: Bytes,
}

#[derive(Debug)]
pub enum Error {
    BridgeError(Bytes),
    OneshotError(oneshot::Canceled),
}

struct Dispatcher {
    next_available_request_id: u16,
    in_flight: HashMap<u16, oneshot::Sender<Result<Bytes, Error>>>,
}

impl Dispatcher {
    fn new() -> Dispatcher {
        Dispatcher {
            next_available_request_id: 0,
            in_flight: HashMap::new(),
        }
    }
    fn new_request(&mut self) -> (u16, Box<Future<Item = Bytes, Error = Error>>) {
        let request_id = self.next_available_request_id;
        self.next_available_request_id = self.next_available_request_id + 1;
        let (sender, receiver) = oneshot::channel::<Result<Bytes, Error>>();

        self.in_flight.insert(request_id, sender);
        (
            request_id,
            Box::new(receiver.map_err(|e| Error::OneshotError(e)).and_then(|e| e)),
        )
    }
    fn resolve(&mut self, request_id: u16, data: Bytes) {
        if let Some(x) = self.in_flight.remove(&request_id) {
            if let Err(_) = x.send(Ok(data)) {
                eprintln!("Warning: Response to request {} was dropped", request_id)
            }
        }
    }
    fn reject(&mut self, request_id: u16, error: Bytes) {
        if let Some(x) = self.in_flight.remove(&request_id) {
            if let Err(_) = x.send(Err(Error::BridgeError(error))) {
                eprintln!("Warning: Rejection of request {} was dropped", request_id)
            }
        }
    }
}

#[derive(Debug)]
enum SinkError {
    Unknown,
    IO(io::Error),
}

pub struct IncomingPacket {
    pub packet: Bytes,
    pub rssi: u8,
    pub link_quality: u8,
}

fn handle_incoming_command(
    cmd: serial_protocol::Command,
    dispatcher: &Arc<RwLock<Dispatcher>>,
    packet_output: &mpsc::UnboundedSender<IncomingPacket>,
) -> Result<(), ()> {
    if cmd.command_id == 0x80 {
        dispatcher
            .write()
            .unwrap()
            .resolve(cmd.request_id, cmd.data);
        Ok(())
    } else if cmd.command_id == 0x81 {
        dispatcher.write().unwrap().reject(cmd.request_id, cmd.data);
        Ok(())
    } else if cmd.command_id == 0xC0 {
        if cmd.data.len() > 2 {
            packet_output
                .unbounded_send(IncomingPacket {
                    packet: cmd.data.slice_to(cmd.data.len() - 2),
                    rssi: cmd.data[cmd.data.len() - 2],
                    link_quality: cmd.data[cmd.data.len() - 1],
                })
                .map_err(|e| eprintln!("{:?}", e))
        } else {
            eprintln!("Packet received without postfix");
            Ok(())
        }
    } else {
        eprintln!("Unexpected command received: {}", cmd.command_id as usize);
        Ok(())
    }
}

pub struct RadioBridgeService {
    dispatcher: Arc<RwLock<Dispatcher>>,
    command_sink: mpsc::UnboundedSender<serial_protocol::Command>,
}

impl RadioBridgeService {
    pub fn new(
        port: Serial,
        handle: Handle,
    ) -> (RadioBridgeService, mpsc::UnboundedReceiver<IncomingPacket>) {
        let (output_sink, output_stream) = serial_protocol::Codec::new().framed(port).split();
        let (command_sink, receiver) = mpsc::unbounded::<serial_protocol::Command>();
        let (packet_output, packet_stream) = mpsc::unbounded::<IncomingPacket>();
        handle.spawn(
            output_sink
                .sink_map_err(|e| SinkError::IO(e))
                .send_all(receiver.map_err(|_| SinkError::Unknown))
                .map_err(|e| println!("{:?}", e))
                .map(|_| ()),
        );
        let dispatcher = Arc::new(RwLock::new(Dispatcher::new()));
        let dispatcher_clone = dispatcher.clone();
        handle.spawn(
            output_stream
                .map_err(|e| println!("{:?}", e))
                .for_each(move |cmd| {
                    handle_incoming_command(cmd, &dispatcher_clone, &packet_output)
                }),
        );
        (
            RadioBridgeService {
                dispatcher: dispatcher,
                command_sink: command_sink,
            },
            packet_stream,
        )
    }
}

impl Service for RadioBridgeService {
    type Request = Request;
    type Response = Bytes;
    type Error = Error;
    type Future = Box<Future<Item = Self::Response, Error = Self::Error>>;

    fn call(&self, request: Self::Request) -> Self::Future {
        let (request_id, result) = self.dispatcher.write().unwrap().new_request();
        self.command_sink
            .unbounded_send(serial_protocol::Command {
                command_id: request.command_id as u8,
                request_id: request_id,
                data: request.data,
            })
            .unwrap();
        result
    }
}
