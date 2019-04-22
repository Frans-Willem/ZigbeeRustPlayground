use crate::ieee802154::mac::frame::*;
use crate::ieee802154::{ExtendedAddress, ShortAddress, PANID};
use bytes::Bytes;
use futures::future::Future as _;
use std::sync::Mutex;
use tokio_core::reactor::Handle;

use crate::radio_bridge::service::Error as RadioError;
use crate::radio_bridge::service::IncomingPacket as RadioPacket;
use crate::radio_bridge::service::RadioBridgeService as RadioService;
use crate::radio_bridge::service::RadioRxMode;

pub struct Service {
    pan_id: PANID,
    short_address: ShortAddress,
    extended_address: ExtendedAddress,
    sequence_number: Mutex<u8>,
}

pub enum Event {
    BeaconRequest(),
}

#[derive(Debug)]
pub enum Error {
    RadioError(RadioError),
}

type Future<T> = futures::Future<Item = T, Error = Error>;
type BoxFuture<T> = Box<Future<T>>;
type Stream<T> = futures::Stream<Item = T, Error = Error>;
type BoxStream<T> = Box<Stream<T>>;

impl Service {
    pub fn new(
        handle: Handle,
        radio: RadioService,
        packetstream: Box<futures::Stream<Item = RadioPacket, Error = RadioError>>,
        channel: u16,
        short_address: ShortAddress,
        pan_id: PANID,
    ) -> BoxFuture<(Service, Stream<Event>)> {
        let max_power = radio.get_tx_power_max().map(move |pwr| (pwr, radio));
        let set_properties = max_power.and_then(|(max_power, radio)| {
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
        unimplemented!();
    }

    fn fresh_sequence_number(&self) -> u8 {
        let mut guard = self.sequence_number.lock().unwrap();
        let retval = *guard;
        *guard = *guard + 1;
        retval
    }

    pub fn get_pan_id(&self) -> PANID {
        return self.pan_id;
    }

    pub fn get_short_address(&self) -> ShortAddress {
        return self.short_address;
    }

    fn send_raw_frame(&self, _frame: Frame) -> Box<Future<()>> {
        unimplemented!();
    }

    pub fn send_beacon(&self, payload: Bytes) -> Box<Future<()>> {
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
