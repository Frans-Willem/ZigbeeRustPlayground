use crate::ieee802154::mac::commands::*;
use crate::ieee802154::mac::frame::*;
use crate::ieee802154::mac::mru_address_set::{MRUAddressSet, MRUAddressSetAction};
use crate::ieee802154::mac::queue::{Queue, QueueError, QueueEvent};
use crate::ieee802154::{ExtendedAddress, ShortAddress, PANID};
use crate::parse_serialize::{Deserialize, Serialize, SerializeError};
use crate::radio_bridge::service::Error as RadioError;
use crate::radio_bridge::service::IncomingPacket as RadioPacket;
use crate::radio_bridge::service::RadioBridgeService as RadioService;
use crate::radio_bridge::service::RadioRxMode;
use crate::CloneSpawn;
use bimap::BiHashMap;
use futures::future::{Future, FutureExt, TryFutureExt};
use futures::stream::Stream;
use futures::task::{Context, Poll, SpawnExt};
use std::collections::HashMap;
use std::pin::Pin;

struct NodeInformation {
    receiver_on_when_idle: bool,
}

pub struct Service {
    handle: Box<dyn CloneSpawn>,
    packet_stream: Box<dyn Stream<Item = RadioPacket> + Unpin + Send>,
    radio: RadioService,
    sequence_number: u8,
    pan_id: PANID,
    short_address: ShortAddress,
    extended_address: ExtendedAddress,
    associations: BiHashMap<ShortAddress, ExtendedAddress>,
    nodeinfo: HashMap<ExtendedAddress, NodeInformation>,
    queue: Queue,
    inflight:
        HashMap<AddressSpecification, Box<dyn Future<Output = Result<(), Error>> + Send + Unpin>>,
    pending_data_set: MRUAddressSet,
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
    Unimplemented,
    SerializationError(SerializeError),
    NodeNotAssociated,
    QueueError(QueueError),
}

impl From<QueueError> for Error {
    fn from(item: QueueError) -> Self {
        Error::QueueError(item)
    }
}

impl Service {
    pub fn new(
        handle: Box<dyn CloneSpawn>,
        radio: RadioService,
        packet_stream: Box<dyn Stream<Item = RadioPacket> + Send + Unpin>,
        channel: u16,
        short_address: ShortAddress,
        pan_id: PANID,
    ) -> impl Future<Output = Result<Service, Error>> + Send {
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
            let f6 = radio.init_pending_data_table();
            let f56 = futures::future::try_join(f5, f6);
            futures::future::try_join5(f1, f2, f3, f4, f56).map_ok(move |_| radio)
        });
        let turn_on = set_properties.and_then(|radio| radio.on().map_ok(move |_| radio));
        let extended_address = turn_on.and_then(|radio| {
            radio
                .get_long_address()
                .map_ok(move |extaddr| (extaddr, radio))
        });
        let service = extended_address.map_ok(move |(extended_address, radio)| {
            let extended_address = ExtendedAddress(extended_address);
            let mut associations = BiHashMap::new();
            associations.insert(short_address.clone(), extended_address.clone());

            println!("Created service!");
            Service {
                handle,
                packet_stream,
                radio: radio,
                sequence_number: 0, // TODO: Random number
                pan_id,
                short_address,
                extended_address,
                associations: associations,
                nodeinfo: HashMap::new(),
                queue: Queue::new(),
                inflight: HashMap::new(),
                pending_data_set: MRUAddressSet::new(8, 8),
            }
        });
        let service = service.map_err(|e| Error::RadioError(e));
        service
    }

    fn handle_frame(&mut self, frame: Frame, _rssi: u8, _link_quality: u8) -> Option<Event> {
        // TODO: Duplicate frame detection
        match frame.frame_type {
            FrameType::Command(Command::BeaconRequest) => Some(Event::BeaconRequest()),
            FrameType::Command(Command::DataRequest) => {
                let pending_frames = self.queue.on_data_request(frame.source);
                self.set_pending_data(&frame.source, pending_frames);
                None
            }
            FrameType::Command(Command::AssociationRequest(CommandAssociationRequest {
                receive_on_when_idle,
                ..
            })) => {
                if let AddressSpecification::Extended(_pan_id, ext_source) = frame.source {
                    // TODO: Check if this was meant for us...
                    Some(Event::AssociationRequest {
                        source: ext_source,
                        receive_on_when_idle,
                    })
                } else {
                    eprintln!("Association request without extended source address, confused...");
                    None
                }
            }
            FrameType::Ack => {
                if let Some(sequence_nr) = frame.sequence_number {
                    self.queue.on_ack(sequence_nr)
                }
                None
            }
            _ => {
                println!("Unhandled packet");
                None
            }
        }
    }

    fn handle_packet(&mut self, packet: RadioPacket) -> Option<Event> {
        match Frame::deserialize(&packet.packet) {
            Err(e) => {
                eprintln!("Unable to parse MAC packet, {:?}: {:?}", e, packet.packet);
                None
            }
            Ok((_, frame)) => self.handle_frame(frame, packet.rssi, packet.link_quality),
        }
    }

    fn fresh_sequence_number(&mut self) -> u8 {
        self.sequence_number = self.sequence_number + 1;
        self.sequence_number
    }

    fn send_frame_noqueue(&mut self, frame: Frame) -> impl Future<Output = Result<(), Error>> {
        let mut serialized = vec![];
        if let Err(e) = frame.serialize_to(&mut serialized) {
            Box::new(futures::future::err(Error::SerializationError(e)))
                as Box<dyn Future<Output = Result<(), Error>> + Unpin + Send>
        } else {
            Box::new(
                self.radio
                    .send(serialized.into())
                    .map_err(Error::RadioError),
            )
        }
    }

    pub fn send_beacon(&mut self, payload: Vec<u8>) -> impl Future<Output = Result<(), Error>> {
        //TODO: Implement this!
        let beacon = Frame {
            frame_pending: false,
            acknowledge_request: false,
            sequence_number: self.fresh_sequence_number().into(),
            destination: AddressSpecification::None,
            source: (self.pan_id.clone(), self.short_address.clone()).into(),
            frame_type: FrameType::Beacon(Beacon {
                beacon_order: 15,
                superframe_order: 15,
                final_cap_slot: 15,
                battery_life_extension: false,
                pan_coordinator: true,
                association_permit: true,
                pending_short_addresses: vec![],
                pending_long_addresses: vec![],
                payload,
            }),
        };
        self.send_frame_noqueue(beacon)
    }

    fn find_free_short_address(&self) -> ShortAddress {
        let mut retval: u16 = 0;
        while self.associations.contains_left(&ShortAddress(retval)) {
            retval = retval + 1
        }
        ShortAddress(retval)
    }

    pub fn associate(
        &mut self,
        address: ExtendedAddress,
        receiver_on_when_idle: bool,
    ) -> ShortAddress {
        self.nodeinfo.insert(
            address,
            NodeInformation {
                receiver_on_when_idle,
            },
        );
        match self.associations.get_by_right(&address) {
            Some(left) => left.clone(),
            None => {
                let short_address = self.find_free_short_address();
                self.associations.insert(short_address.clone(), address);
                short_address
            }
        }
    }

    fn get_nodeinfo_for_addrspec(&self, addr: &AddressSpecification) -> Option<&NodeInformation> {
        match addr {
            AddressSpecification::None => None,
            AddressSpecification::Short(panid, address) => {
                if panid.clone() == self.pan_id {
                    let extaddr = self.associations.get_by_left(address);
                    let nodeinfo = extaddr.and_then(|extaddr| self.nodeinfo.get(extaddr));
                    nodeinfo
                } else {
                    None
                }
            }
            AddressSpecification::Extended(_, address) => self.nodeinfo.get(address),
        }
    }

    fn handle_mru_address_set_action(&mut self, action: MRUAddressSetAction) {
        match action {
            MRUAddressSetAction::None => (),
            MRUAddressSetAction::SetShortSlot(slot, address) => {
                let address = address.map(|(panid, address)| (panid.as_u16(), address.as_u16()));
                self.handle
                    .spawn(self.radio.set_pending_data_short(slot, address).map(|res| {
                        if let Err(e) = res {
                            eprintln!("Unable to set or clear short address slot {:?}", e);
                        }
                    }))
                    .unwrap();
            }
            MRUAddressSetAction::SetExtendedSlot(slot, address) => {
                let address = address.map(|address| address.as_u64());
                self.handle
                    .spawn(self.radio.set_pending_data_ext(slot, address).map(|res| {
                        if let Err(e) = res {
                            eprintln!("Unable to set or clear short address slot {:?}", e);
                        }
                    }))
                    .unwrap();
            }
        }
    }

    fn set_pending_data(&mut self, address: &AddressSpecification, pending_frames: bool) {
        let action = match pending_frames {
            false => self.pending_data_set.remove(address),
            true => self.pending_data_set.insert(address),
        };
        self.handle_mru_address_set_action(action);
    }

    fn send_frame_queued(&mut self, frame: Frame) -> impl Future<Output = Result<(), Error>> {
        let receiver_on_when_idle = self
            .get_nodeinfo_for_addrspec(&frame.destination)
            .map(|x| x.receiver_on_when_idle)
            .unwrap_or(true);
        if !receiver_on_when_idle {
            self.set_pending_data(&frame.destination, true);
        }
        self.queue.enqueue(frame, receiver_on_when_idle).err_into()
    }

    pub fn send_association_response(
        &mut self,
        extended_address: ExtendedAddress,
    ) -> impl Future<Output = Result<(), Error>> {
        if let Some(short_address) = self.associations.get_by_right(&extended_address) {
            let short_address = short_address.clone();
            let frame = Frame {
                frame_pending: false,
                acknowledge_request: true,
                sequence_number: self.fresh_sequence_number().into(),
                destination: (self.pan_id, extended_address.clone()).into(),
                source: (self.pan_id, self.extended_address).into(),
                frame_type: FrameType::Command(Command::AssociationResponse(
                    CommandAssociationResponse {
                        short_address: short_address,
                        status: AssociationResponseStatus::AssociationSuccessful,
                    },
                )),
            };
            Box::new(self.send_frame_queued(frame))
        } else {
            Box::new(futures::future::err(Error::NodeNotAssociated))
                as Box<dyn Future<Output = Result<(), Error>> + Send + Unpin>
        }
    }

    /*
    pub fn send_data(
        &mut self,
        extended_address: ExtendedAddress,
        payload: Vec<u8>) impl Future<Output = Result<(), Error>> {
        if let Some(short_address) = self.associations.get_by_right(&extended_address) {
            let short_address = short_address.clone();
            let frame = Frame {
                frame_pending: false,
                acknowledge_request: true,
                sequence_number: self.fresh_sequence_number().into(),
                destination: (self.pan_id, short_address.clone()).into(),
                source: (self.pan_id, ShortAddress(0)).into(),
                frame_type: FrameType::Data(payload)


            }
        } else {
            Box::new(futures::future::err(Error::NodeNotAssociated))
                as Box<dyn Future<Output = Result<(), Error>> + Send + Unpin>
        }
    }
    */
}

impl Stream for Service {
    type Item = Event;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let uself = self.get_mut();
        // Parse incoming packets, and if needed, poop out events.
        while let Poll::Ready(packet) = Pin::new(&mut uself.packet_stream).poll_next(cx) {
            if let Some(packet) = packet {
                if let Some(event) = uself.handle_packet(packet) {
                    return Poll::Ready(Some(event));
                }
            } else {
                eprintln!("Radio packet stream finished :(");
            }
        }

        // Poll the queue
        while let Poll::Ready(outgoing) = Pin::new(&mut uself.queue).poll_next(cx) {
            match outgoing {
                Some(QueueEvent::OutgoingFrame(destination, frame)) => {
                    let fut = Box::new(uself.send_frame_noqueue(frame));
                    uself.inflight.insert(destination, fut);
                }
                Some(QueueEvent::QueueFlushed(destination)) => {
                    uself.set_pending_data(&destination, false);
                }
                None => {
                    eprintln!("Queue stream finished :(");
                }
            }
        }

        // Poll inflight packets
        let mut results = Vec::new();
        uself.inflight.retain(|key, value| {
            if let Poll::Ready(res) = Pin::new(value).poll(cx) {
                results.push((key.clone(), res));
                false
            } else {
                true
            }
        });
        for (key, value) in results.into_iter() {
            uself.queue.on_send_result(key, value.is_ok());
        }
        Poll::Pending
    }
}
