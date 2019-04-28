use bytes::Bytes;
use futures::channel::{mpsc, oneshot};
use futures::task::{Spawn, SpawnExt};
use futures::{Future, Sink, SinkExt, Stream, StreamExt, TryFutureExt, TryStreamExt};
use std::collections::HashMap;
use std::io;
use std::sync::{Arc, RwLock};

use crate::radio_bridge::serial_protocol;
use crate::ret_future::*;

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
    fn new_request(&mut self) -> (u16, impl Future<Output = Result<Bytes, Error>>) {
        let request_id = self.next_available_request_id;
        self.next_available_request_id = self.next_available_request_id + 1;
        let (sender, receiver) = oneshot::channel::<Result<Bytes, Error>>();

        self.in_flight.insert(request_id, sender);
        (
            request_id,
            return_try_future(
                receiver
                    .map_err(|e| Error::OneshotError(e))
                    .and_then(futures::future::ready),
            ),
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
) {
    if cmd.command_id == 0x80 {
        dispatcher
            .write()
            .unwrap()
            .resolve(cmd.request_id, cmd.data);
    } else if cmd.command_id == 0x81 {
        dispatcher.write().unwrap().reject(cmd.request_id, cmd.data);
    } else if cmd.command_id == 0xC0 {
        if cmd.data.len() > 2 {
            packet_output
                .unbounded_send(IncomingPacket {
                    packet: cmd.data.slice_to(cmd.data.len() - 2),
                    rssi: cmd.data[cmd.data.len() - 2],
                    link_quality: cmd.data[cmd.data.len() - 1],
                })
                .unwrap();
        } else {
            eprintln!("Packet received without postfix");
        }
    } else {
        eprintln!("Unexpected command received: {}", cmd.command_id as usize);
    }
}

pub struct RadioBridgeService {
    dispatcher: Arc<RwLock<Dispatcher>>,
    command_sink: mpsc::UnboundedSender<serial_protocol::Command>,
}

impl RadioBridgeService {
    pub fn new(
        serial_output: Box<Sink<serial_protocol::Command, SinkError = io::Error> + Unpin + Send>,
        serial_input: Box<
            Stream<Item = Result<serial_protocol::Command, io::Error>> + Unpin + Send,
        >,
        handle: &mut Spawn,
    ) -> (RadioBridgeService, mpsc::UnboundedReceiver<IncomingPacket>) {
        let (command_sink, mut outgoing_command_stream) =
            mpsc::unbounded::<serial_protocol::Command>();
        let (packet_output, packet_stream) = mpsc::unbounded::<IncomingPacket>();
        let mut serial_output = serial_output;
        /*
        // Debug code
        let mut outgoing_command_stream = outgoing_command_stream.map(|item| {
            eprintln!("Serial output: {:?}", item);
            item
        });
        let serial_input = serial_input.map(|item| {
            eprintln!("Serial input: {:?}", item);
            item
        });
        // End Debug code
        */
        handle
            .spawn(async move {
                await!(serial_output.send_all(&mut outgoing_command_stream)).unwrap();
            })
            .unwrap();
        let dispatcher = Arc::new(RwLock::new(Dispatcher::new()));
        let dispatcher_clone = dispatcher.clone();
        handle
            .spawn(async move {
                await!(serial_input.for_each(move |res| {
                    match res {
                        Ok(cmd) => handle_incoming_command(cmd, &dispatcher_clone, &packet_output),
                        Err(e) => eprintln!("Serial input error: {:?}", e),
                    }
                    futures::future::ready(())
                }));
            })
            .unwrap();
        (
            RadioBridgeService {
                dispatcher: dispatcher,
                command_sink: command_sink,
            },
            packet_stream,
        )
    }

    pub fn call(&self, request: Request) -> impl Future<Output = Result<Bytes, Error>> {
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
