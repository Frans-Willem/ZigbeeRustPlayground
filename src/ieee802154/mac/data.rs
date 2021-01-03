use crate::ieee802154::frame;
use crate::ieee802154::frame::{AddressingMode, FrameType, FullAddress};
use crate::ieee802154::mac::devicequeue::{DeviceQueue, DeviceQueueAction, DeviceQueueError};
use crate::ieee802154::pib::PIB;
use crate::ieee802154::{ExtendedAddress, ShortAddress, PANID};
use crate::ieee802154::services::mcps;
use crate::ieee802154::mac::combinedpendingtable::{CombinedPendingTable, CombinedPendingTableAction};
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

impl From<CombinedPendingTableAction> for DataServiceAction {
    fn from(action: CombinedPendingTableAction) -> DataServiceAction {
        match action {
            CombinedPendingTableAction::Init(x) => DataServiceAction::InitPendingTable(x),
            CombinedPendingTableAction::UpdateShort(k,i,v) => DataServiceAction::SetPendingShort(k,i,v),
            CombinedPendingTableAction::UpdateExtended(k,i,v) => DataServiceAction::SetPendingExtended(k,i,v),
        }
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
                return Poll::Ready(x.into());
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
        for (_destination, queue) in self.queues.iter_mut() {
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
        for (_destination, queue) in self.queues.iter_mut() {
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

impl DataService {
    pub fn process_mcps_request(&mut self, request: mcps::Request) -> Option<mcps::Confirm> {
        // TODO
        None
    }

    pub fn process_mcps_response(&mut self, response: mcps::Response) {
        match response 
        {
        }
    }
}
