use crate::ieee802154::frame;
use crate::ieee802154::frame::{Address, AddressingMode, FrameType, FullAddress};
use crate::ieee802154::mac::devicequeue::{DeviceQueue, DeviceQueueAction, DeviceQueueError};
use crate::ieee802154::mac::pendingtable::PendingTable;
use crate::ieee802154::pib::PIB;
use crate::ieee802154::{ExtendedAddress, ShortAddress, PANID};
use crate::unique_key::UniqueKey;
use crate::waker_store::WakerStore;
use std::collections::HashMap;
use std::task::{Context, Poll};

#[derive(Debug)]
pub enum DataServiceAction {
    InitPendingTable(UniqueKey),
    SetPendingShort(UniqueKey, usize, Option<(PANID, ShortAddress)>),
    SetPendingExtended(UniqueKey, usize, Option<ExtendedAddress>),
    SendFrame(UniqueKey, frame::Frame),
    ReportResult(UniqueKey, Result<Vec<u8>, DeviceQueueError>),
}

struct CombinedPendingTable {
    waker: WakerStore,
    initializing: Option<UniqueKey>,
    is_initialized: bool,
    none: bool,
    short: PendingTable<(PANID, ShortAddress)>,
    extended: PendingTable<ExtendedAddress>,
}

impl CombinedPendingTable {
    fn new() -> Self {
        Self {
            waker: WakerStore::new(),
            initializing: None,
            is_initialized: false,
            none: false,
            short: PendingTable::<(PANID, ShortAddress)>::new(8),
            extended: PendingTable::<ExtendedAddress>::new(8),
        }
    }

    fn report_init_result(&mut self, key: UniqueKey, result: bool) {
        if self.initializing == Some(key) {
            self.initializing = None;
            self.is_initialized = result;
            if result {
                // After a init pending table, the entire table should be clear on the device side
                self.short.assume_empty();
                self.extended.assume_empty();
            }
            self.waker.wake();
        }
    }

    fn set(&mut self, address: &Option<FullAddress>, inserted: bool) {
        match address {
            None => self.none = inserted,
            Some(FullAddress { pan_id, address }) => match address {
                Address::Short(address) => {
                    self.short.set(&(pan_id.clone(), address.clone()), inserted)
                }
                Address::Extended(address) => self.extended.set(address, inserted),
            },
        }
    }

    fn poll_action(&mut self, cx: &mut Context<'_>) -> Poll<DataServiceAction> {
        if self.initializing.is_some() {
            self.waker.pend(cx)
        } else if !self.is_initialized {
            let key = UniqueKey::new();
            self.initializing = Some(key);
            Poll::Ready(DataServiceAction::InitPendingTable(key))
        } else if let Poll::Ready(update) = self.short.poll_update(cx) {
            Poll::Ready(DataServiceAction::SetPendingShort(
                update.key,
                update.index,
                update.value,
            ))
        } else if let Poll::Ready(update) = self.extended.poll_update(cx) {
            Poll::Ready(DataServiceAction::SetPendingExtended(
                update.key,
                update.index,
                update.value,
            ))
        } else {
            Poll::Pending
        }
    }

    fn report_update_result(&mut self, key: UniqueKey, success: bool) {
        self.short.report_update_result(key, success);
        self.extended.report_update_result(key, success);
    }
}

pub struct DataRequest {
    pub key: UniqueKey,
    pub destination: Option<FullAddress>,
    pub source_mode: AddressingMode,
    pub acknowledge_request: bool,
    pub indirect: bool,
    pub content: FrameType,
}

pub struct DataService {
    queues: HashMap<Option<FullAddress>, DeviceQueue>,
    pending_table: CombinedPendingTable,
    waker: WakerStore,
}

impl DataService {
    pub fn new() -> Self {
        Self {
            queues: HashMap::new(),
            pending_table: CombinedPendingTable::new(),
            waker: WakerStore::new(),
        }
    }
    pub fn insert(&mut self, pib: &PIB, entry: DataRequest) {
        if let Some(existing) = self.queues.get_mut(&entry.destination) {
            existing.insert(pib, entry);
        } else {
            let mut new_queue = DeviceQueue::new();
            let destination = entry.destination;
            new_queue.insert(pib, entry);
            self.queues.insert(destination, new_queue);
            self.waker.wake();
        }
    }

    pub fn poll_action(&mut self, pib: &mut PIB, cx: &mut Context<'_>) -> Poll<DataServiceAction> {
        'retry: loop {
            if let Poll::Ready(x) = self.pending_table.poll_action(cx) {
                return Poll::Ready(x);
            }
            for (destination, queue) in self.queues.iter_mut() {
                if let Poll::Ready(x) = queue.poll_next_action(pib, cx) {
                    match x {
                        DeviceQueueAction::Empty() => {
                            let destination = *destination;
                            self.queues.remove(&destination);
                            continue 'retry;
                        }
                        DeviceQueueAction::SetPending(pending) => {
                            self.pending_table.set(destination, pending);
                            continue 'retry;
                        }
                        DeviceQueueAction::SendFrame(key, frame) => {
                            return Poll::Ready(DataServiceAction::SendFrame(key, frame));
                        }
                        DeviceQueueAction::ReportResult(key, result) => {
                            return Poll::Ready(DataServiceAction::ReportResult(key, result));
                        }
                    }
                }
            }
            return self.waker.pend(cx);
        }
    }
}

impl DataService {
    pub fn process_init_pending_table_result(&mut self, key: UniqueKey, success: bool) {
        self.pending_table.report_init_result(key, success)
    }
    pub fn process_set_pending_result(&mut self, key: UniqueKey, success: bool) {
        self.pending_table.report_update_result(key, success)
    }
    pub fn process_send_result(&mut self, key: UniqueKey, success: bool) {
        for (destination, queue) in self.queues.iter_mut() {
            queue.process_send_result(key, success);
        }
    }
}

impl DataService {
    pub fn process_frame(&mut self, pib: &PIB, frame: &frame::Frame) {
        match &frame.frame_type {
            frame::FrameType::Ack(payload) => self.process_frame_ack(frame, &payload),
            frame::FrameType::Command(frame::Command::DataRequest()) => {
                self.process_frame_data_request(pib, frame)
            }
            _ => (),
        }
    }

    fn process_frame_ack(&mut self, frame: &frame::Frame, payload: &frame::Payload) {
        for (destination, queue) in self.queues.iter_mut() {
            queue.process_acknowledge(frame.sequence_number, &payload.0);
        }
    }

    fn process_frame_data_request(&mut self, pib: &PIB, frame: &frame::Frame) {
        if frame.destination == Some(pib.get_full_short_address())
            || frame.destination == Some(pib.get_full_extended_address())
        {
            if let Some(queue) = self.queues.get_mut(&frame.source) {
                println!("Doing queue.process_datarequest");
                queue.process_datarequest();
            } else {
                println!("WARNING!: No queue for {:?}", frame.source);
            }
        } else {
            println!("WARNING!: Ignoring data request as destination does not match");
        }
    }
}