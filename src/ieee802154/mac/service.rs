use crate::ieee802154::mac::frame::*;
use crate::ieee802154::{ExtendedAddress, ShortAddress, PANID};
use crate::parse_serialize::{ParseFromBuf, SerializeToBuf};
use bytes::{Bytes, IntoBuf};
use futures::future::Future as _;
use futures::stream::Stream as _;
use futures::sync::mpsc;
use std::rc::Rc;
use std::sync::Mutex;
use tokio_core::reactor::Handle;

use crate::cachemap::CacheMap;
use crate::parse_serialize::Error as ParseError;
use crate::radio_bridge::service::Error as RadioError;
use crate::radio_bridge::service::IncomingPacket as RadioPacket;
use crate::radio_bridge::service::RadioBridgeService as RadioService;
use crate::radio_bridge::service::RadioRxMode;

struct InnerService {
    handle: Handle,
    radio: RadioService,
    event_sink: mpsc::UnboundedSender<Event>,
    sequence_number: Mutex<u8>,
    seen_messages: CacheMap<Frame, ()>,
}

pub struct Service {
    inner: Rc<InnerService>,
    pan_id: PANID,
    short_address: ShortAddress,
    extended_address: ExtendedAddress,
}

pub enum Event {
    BeaconRequest(),
}

#[derive(Debug)]
pub enum Error {
    RadioError(RadioError),
    MPSCError,
    SerializationError(ParseError),
}
impl From<RadioError> for Error {
    fn from(err: RadioError) -> Self {
        Error::RadioError(err)
    }
}

type Future<T> = futures::Future<Item = T, Error = Error>;
type BoxFuture<T> = Box<Future<T>>;
type Stream<T> = futures::Stream<Item = T, Error = Error>;
type BoxStream<T> = Box<Stream<T>>;

impl InnerService {
    fn new(handle: &Handle, radio: RadioService) -> (InnerService, mpsc::UnboundedReceiver<Event>) {
        let (sender, receiver) = mpsc::unbounded();
        (
            InnerService {
                handle: handle.clone(),
                radio,
                event_sink: sender,
                sequence_number: Mutex::new(0),
                seen_messages: CacheMap::new(handle.clone()),
            },
            receiver,
        )
    }
    fn push_event(&self, event: Event) {
        self.event_sink.unbounded_send(event).unwrap()
    }

    fn fresh_sequence_number(&self) -> u8 {
        let mut guard = self.sequence_number.lock().unwrap();
        let retval = *guard;
        *guard = *guard + 1;
        retval
    }

    fn on_incoming_frame(&self, frame: Frame, _rssi: u8, _link_quality: u8) {
        if let Some(_) =
            self.seen_messages
                .insert(frame.clone(), (), std::time::Duration::from_secs(2))
        {
            println!("Duplicate frame received!");
        } else {
            println!("<< {:?}", frame);
            match frame.frame_type {
                FrameType::Command(Command::BeaconRequest) => {
                    println!("Got a beacon requets :)");
                    self.push_event(Event::BeaconRequest())
                }
                _ => println!("Unhandled"),
            }
        }
    }

    fn on_incoming_packet(&self, packet: RadioPacket) {
        match Frame::parse_from_buf(&mut packet.packet.clone().into_buf()) {
            Err(e) => eprintln!("Unable to parse MAC packet, {:?}: {:?}", e, packet.packet),
            Ok(frame) => self.on_incoming_frame(frame, packet.rssi, packet.link_quality),
        }
    }

    fn send_raw_frame(&self, frame: Frame) -> BoxFuture<()> {
        // TODO: This should only return if both the send succeeds, and (if required) an Ack was
        // received.
        let mut serialized = vec![];
        if let Err(e) = frame.serialize_to_buf(&mut serialized) {
            Box::new(futures::future::err(Error::SerializationError(e)))
        } else {
            Box::new(
                self.radio
                    .send(serialized.into())
                    .map_err(Error::RadioError),
            )
        }
    }
}

impl Service {
    pub fn new(
        handle: Handle,
        radio: RadioService,
        packetstream: Box<futures::Stream<Item = RadioPacket, Error = RadioError>>,
        channel: u16,
        short_address: ShortAddress,
        pan_id: PANID,
    ) -> BoxFuture<(Service, BoxStream<Event>)> {
        let max_power = radio.get_tx_power_max().map(move |pwr| (pwr, radio));
        let set_properties = max_power.and_then(move |(max_power, radio)| {
            let f1 = radio.set_tx_power(max_power);
            let f2 = radio.set_channel(channel);
            let f3 = radio.set_short_address(short_address.as_u16());
            let f4 = radio.set_pan_id(pan_id.as_u16());
            let f5 = radio.set_rx_mode(RadioRxMode {
                address_filter: true,
                autoack: true,
                poll_mode: false,
            });
            f1.join5(f2, f3, f4, f5).map(move |_| radio)
        });
        let turn_on = set_properties.and_then(|radio| radio.on().map(move |_| radio));
        let extended_address = turn_on.and_then(|radio| {
            radio
                .get_long_address()
                .map(move |extaddr| (extaddr, radio))
        });
        let service = extended_address.map(move |(extended_address, radio)| {
            let (inner, events) = InnerService::new(&handle, radio);
            let inner = Rc::new(inner);
            let packet_inner = inner.clone();
            handle.spawn(
                packetstream
                    .for_each(move |packet| Ok(packet_inner.on_incoming_packet(packet)))
                    .map_err(|e| eprintln!("Radio packet error: {:?}", e)),
            );
            (
                Service {
                    inner,
                    pan_id,
                    short_address,
                    extended_address: ExtendedAddress(extended_address),
                },
                Box::new(events.map_err(|_| Error::MPSCError)) as BoxStream<Event>,
            )
        });
        let service = service.map_err(|e| Error::RadioError(e));
        let service = Box::new(service);
        service
    }

    fn fresh_sequence_number(&self) -> u8 {
        self.inner.fresh_sequence_number()
    }

    pub fn get_pan_id(&self) -> PANID {
        return self.pan_id;
    }

    pub fn get_short_address(&self) -> ShortAddress {
        return self.short_address;
    }

    fn send_raw_frame(&self, frame: Frame) -> BoxFuture<()> {
        self.inner.send_raw_frame(frame)
    }

    pub fn send_beacon(&self, payload: Bytes) -> BoxFuture<()> {
        let beacon = Frame {
            acknowledge_request: false,
            sequence_number: self.fresh_sequence_number().into(),
            destination_pan: None,
            destination: AddressSpecification::None,
            source_pan: self.get_pan_id().into(),
            source: self.get_short_address().into(),
            frame_type: FrameType::Beacon {
                beacon_order: 15,
                superframe_order: 15,
                final_cap_slot: 15,
                battery_life_extension: false,
                pan_coordinator: true,
                association_permit: true,
            },
            payload: payload,
        };
        self.send_raw_frame(beacon)
    }
}
