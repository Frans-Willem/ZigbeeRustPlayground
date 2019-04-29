use crate::cachemap::CacheMap;
use crate::ieee802154::mac::frame::*;
use crate::ieee802154::{ExtendedAddress, ShortAddress, PANID};
use crate::parse_serialize::Error as ParseError;
use crate::parse_serialize::{ParseFromBuf, SerializeToBuf};
use crate::radio_bridge::service::Error as RadioError;
use crate::radio_bridge::service::IncomingPacket as RadioPacket;
use crate::radio_bridge::service::RadioBridgeService as RadioService;
use crate::radio_bridge::service::RadioRxMode;
use crate::CloneSpawn;
use bytes::{Bytes, IntoBuf};
use futures::channel::mpsc;
use futures::channel::oneshot;
use futures::future::TryFutureExt;
use futures::stream::Stream;
use futures::stream::StreamExt;
use futures::task::SpawnExt;
use std::collections::HashMap;
use std::collections::LinkedList;
use std::ops::DerefMut;
use std::sync::Arc;
use std::sync::Mutex;

#[derive(Debug)]
struct Association {
    short_address: ShortAddress,
    extended_address: ExtendedAddress,
    /**
     * Queue is None when no queue is needed (e.g. the device is receive-while-idle),
     * Otherwise this is a queue of data to be sent.
     */
    queue: Option<Mutex<LinkedList<oneshot::Sender<()>>>>,
}

impl Association {
    fn get_queue_slot(&self) -> impl Future<()> {
        match &self.queue {
            Some(queue) => {
                let (sender, receiver) = oneshot::channel::<()>();
                queue.lock().unwrap().push_back(sender);
                Box::new(receiver.err_into()) as Box<Future<()> + Unpin + Send>
            }
            None => Box::new(futures::future::ok(())),
        }
    }

    fn kick(&self) {
        if let Some(front) = self
            .queue
            .as_ref()
            .and_then(|queue_ref| queue_ref.lock().unwrap().pop_front())
        {
            if let Err(_) = front.send(()) {
                eprintln!("Warning: queue dropped")
            }
        }
    }
}

struct Associations {
    by_short: HashMap<ShortAddress, Arc<Association>>,
    by_extended: HashMap<ExtendedAddress, Arc<Association>>,
}

impl Associations {
    fn new() -> Associations {
        Associations {
            by_short: HashMap::new(),
            by_extended: HashMap::new(),
        }
    }

    fn find_free_short_address(&mut self) -> ShortAddress {
        let mut short_address: u16 = 0x558B;
        // TODO: We should add ourselves in this map, so we don't have to explicitly check for
        // shortaddr 0.
        while self.by_short.contains_key(&ShortAddress(short_address)) || short_address == 0 {
            short_address = short_address + 1;
        }
        eprintln!("Assigning short address {:?}", short_address);
        ShortAddress(short_address)
    }

    fn add(
        &mut self,
        address: ExtendedAddress,
        receive_on_when_idle: bool,
        request_short_address: Option<ShortAddress>,
    ) -> ShortAddress {
        let previous_value = self.by_extended.get(&address);
        let mut new_queue = if !receive_on_when_idle {
            Some(LinkedList::new())
        } else {
            None
        };
        let short_address = match previous_value {
            Some(current_association) => {
                let current_association = current_association.clone();
                // Try to copy old queue
                if let Some(new_queue) = new_queue.as_mut() {
                    if let Some(current_queue) = &current_association.queue {
                        let mut guard = current_queue.lock().unwrap();
                        std::mem::swap(guard.deref_mut(), new_queue);
                    }
                }
                let short_address = if let Some(request_short_address) = request_short_address {
                    if request_short_address == current_association.short_address
                        || !self.by_short.contains_key(&request_short_address)
                    {
                        request_short_address
                    } else {
                        self.find_free_short_address()
                    }
                } else {
                    current_association.short_address.clone()
                };
                if short_address != current_association.short_address {
                    self.by_short.remove(&current_association.short_address);
                }
                short_address
            }
            None => {
                if let Some(request_short_address) = request_short_address {
                    if !self.by_short.contains_key(&request_short_address) {
                        request_short_address
                    } else {
                        self.find_free_short_address()
                    }
                } else {
                    self.find_free_short_address()
                }
            }
        };
        let new_association = Association {
            short_address,
            extended_address: address.clone(),
            queue: new_queue.map(Mutex::new),
        };
        let new_association = Arc::new(new_association);
        self.by_short
            .insert(new_association.short_address, new_association.clone());
        self.by_extended
            .insert(new_association.extended_address, new_association);
        short_address
    }

    fn get_queue_slot_shortaddr(&mut self, address: &ShortAddress) -> impl Future<()> {
        match self.by_short.get(address) {
            Some(association) => Box::new(association.get_queue_slot()),
            None => {
                eprintln!("Attempting to queue data to unassociated device!");
                Box::new(futures::future::ok(())) as Box<Future<()> + Unpin + Send>
            }
        }
    }
    fn get_queue_slot_extaddr(&mut self, address: &ExtendedAddress) -> impl Future<()> {
        match self.by_extended.get(address) {
            Some(association) => Box::new(association.get_queue_slot()),
            None => {
                eprintln!("Attempting to queue data to unassociated device!");
                Box::new(futures::future::ok(())) as Box<Future<()> + Unpin + Send>
            }
        }
    }
    fn get_queue_slot_addrspec(&mut self, address: &AddressSpecification) -> impl Future<()> {
        match address {
            AddressSpecification::None => {
                Box::new(futures::future::ok(())) as Box<Future<()> + Unpin + Send>
            }
            AddressSpecification::Reserved => {
                Box::new(futures::future::ok(())) as Box<Future<()> + Unpin + Send>
            }
            AddressSpecification::Short(address) => {
                Box::new(self.get_queue_slot_shortaddr(address))
            }
            AddressSpecification::Extended(address) => {
                Box::new(self.get_queue_slot_extaddr(address))
            }
        }
    }

    fn kick_shortaddr(&mut self, address: &ShortAddress) {
        if let Some(association) = self.by_short.get(address) {
            association.kick()
        }
    }

    fn kick_extaddr(&mut self, address: &ExtendedAddress) {
        if let Some(association) = self.by_extended.get(address) {
            association.kick()
        }
    }

    fn kick_addrspec(&mut self, address: &AddressSpecification) {
        match address {
            AddressSpecification::None => (),
            AddressSpecification::Reserved => (),
            AddressSpecification::Short(address) => self.kick_shortaddr(address),
            AddressSpecification::Extended(address) => self.kick_extaddr(address),
        }
    }
}

struct InnerService {
    handle: Box<CloneSpawn>,
    radio: RadioService,
    event_sink: mpsc::UnboundedSender<Event>,
    sequence_number: Mutex<u8>,
    seen_messages: CacheMap<Frame, ()>,
    associations: Mutex<Associations>,
}

pub struct Service {
    inner: Arc<InnerService>,
    pan_id: PANID,
    short_address: ShortAddress,
    extended_address: ExtendedAddress,
}

#[derive(Debug, Eq, PartialEq)]
pub enum Event {
    BeaconRequest(),
    AssociationRequest {
        source: ExtendedAddress,
        receive_on_when_idle: bool,
    },
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
impl From<oneshot::Canceled> for Error {
    fn from(_err: oneshot::Canceled) -> Self {
        Error::MPSCError
    }
}

pub trait Future<T> = futures::Future<Output = Result<T, Error>>;

impl InnerService {
    fn new(
        spawner: Box<CloneSpawn>,
        radio: RadioService,
    ) -> (InnerService, mpsc::UnboundedReceiver<Event>) {
        let (sender, receiver) = mpsc::unbounded();
        let seen_messages = CacheMap::new(spawner.clone());
        (
            InnerService {
                handle: spawner,
                radio,
                event_sink: sender,
                sequence_number: Mutex::new(0),
                seen_messages,
                associations: Mutex::new(Associations::new()),
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
            //println!("Duplicate frame received!");
        } else {
            println!("<< {:?}", frame);
            match frame.frame_type {
                FrameType::Command(Command::BeaconRequest) => {
                    println!("Got a beacon requets :)");
                    self.push_event(Event::BeaconRequest())
                }
                FrameType::Command(Command::DataRequest) => {
                    // TODO: Check if this frame was actually meant for me...
                    self.associations
                        .lock()
                        .unwrap()
                        .kick_addrspec(&frame.source)
                }
                FrameType::Command(Command::AssociationRequest {
                    receive_on_when_idle,
                    ..
                }) => {
                    // TODO: Check if this was meant for me...
                    if let AddressSpecification::Extended(ext_source) = frame.source {
                        self.push_event(Event::AssociationRequest {
                            source: ext_source,
                            receive_on_when_idle,
                        })
                    } else {
                        eprintln!(
                            "Association request without extended source address, confused..."
                        )
                    }
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

    fn get_queue_slot(&self, addr: &AddressSpecification) -> impl Future<()> {
        self.associations
            .lock()
            .unwrap()
            .get_queue_slot_addrspec(addr)
    }

    fn send_raw_frame(&self, frame: Frame) -> impl Future<()> {
        // TODO: This should only return if both the send succeeds, and (if required) an Ack was
        // received.
        let mut serialized = vec![];
        if let Err(e) = frame.serialize_to_buf(&mut serialized) {
            Box::new(futures::future::err(Error::SerializationError(e)))
                as Box<Future<()> + Unpin + Send>
        } else {
            Box::new(
                self.radio
                    .send(serialized.into())
                    .map_err(Error::RadioError),
            ) as Box<Future<()> + Unpin + Send>
        }
    }
}

impl Service {
    pub fn new(
        handle: Box<CloneSpawn>,
        radio: RadioService,
        packetstream: Box<futures::Stream<Item = RadioPacket> + Unpin + Send>,
        channel: u16,
        short_address: ShortAddress,
        pan_id: PANID,
    ) -> impl Future<(
        Service,
        Box<dyn Stream<Item = Event> + Send + Unpin + 'static>,
    )> {
        let max_power = radio.get_tx_power_max().map_ok(move |pwr| (pwr, radio));
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
            futures::future::try_join5(f1, f2, f3, f4, f5).map_ok(move |_| radio)
        });
        let turn_on = set_properties.and_then(|radio| radio.on().map_ok(move |_| radio));
        let extended_address = turn_on.and_then(|radio| {
            radio
                .get_long_address()
                .map_ok(move |extaddr| (extaddr, radio))
        });
        let mut handle = handle;
        let service = extended_address.map_ok(move |(extended_address, radio)| {
            let (inner, events) = InnerService::new(handle.clone(), radio);
            let inner = Arc::new(inner);
            let packet_inner = inner.clone();
            handle
                .spawn(packetstream.for_each(move |packet| {
                    futures::future::ready(packet_inner.on_incoming_packet(packet))
                }))
                .unwrap();
            (
                Service {
                    inner,
                    pan_id,
                    short_address,
                    extended_address: ExtendedAddress(extended_address),
                },
                Box::new(events) as Box<Stream<Item = Event> + Send + Unpin>,
            )
        });
        let service = service.map_err(|e| Error::RadioError(e));
        //let service = Box::new(service);
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

    fn send_raw_frame(&self, frame: Frame) -> impl futures::Future<Output = Result<(), Error>> {
        let inner_copy = self.inner.clone();
        self.inner
            .get_queue_slot(&frame.destination)
            .and_then(move |_| inner_copy.send_raw_frame(frame))
    }

    pub fn send_beacon(&self, payload: Bytes) -> impl futures::Future<Output = Result<(), Error>> {
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

    pub fn associate(&self, address: ExtendedAddress, receive_on_when_idle: bool) -> ShortAddress {
        let mut associations = self.inner.associations.lock().unwrap();
        associations.add(address, receive_on_when_idle, None)
    }

    pub fn send_association_response(
        &self,
        target: ExtendedAddress,
        short_address: ShortAddress,
    ) -> impl Future<()> {
        let response = Frame {
            acknowledge_request: true,
            sequence_number: self.fresh_sequence_number().into(),
            destination_pan: Some(self.pan_id),
            destination: target.into(),
            source_pan: Some(self.pan_id),
            source: self.extended_address.into(),
            frame_type: FrameType::Command(Command::AssociationResponse {
                short_address: short_address,
                status: AssociationResponseStatus::AssociationSuccessful,
            }),
            payload: Bytes::new(),
        };
        self.send_raw_frame(response)
    }
}
