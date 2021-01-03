use crate::ieee802154::frame;
use crate::ieee802154::frame::{AddressingMode, FrameType, FullAddress};
use crate::ieee802154::mac::combinedpendingtable::{
    CombinedPendingTable, CombinedPendingTableAction,
};
use crate::ieee802154::mac::devicequeue::{DeviceQueue, DeviceQueueAction, DeviceQueueError};
use crate::ieee802154::pib::PIB;
use crate::ieee802154::services::mcps;
use crate::ieee802154::{ExtendedAddress, ShortAddress, PANID};
use crate::unique_key::UniqueKey;
use crate::waker_store::WakerStore;
use bimap::BiMap;
use std::collections::HashMap;
use std::task::{Context, Poll};

#[derive(Debug)]
pub enum DataServiceAction {
    InitPendingTable(UniqueKey),
    SetPendingShort(UniqueKey, usize, Option<(PANID, ShortAddress)>),
    SetPendingExtended(UniqueKey, usize, Option<ExtendedAddress>),
    SendFrame(UniqueKey, frame::Frame),
    Confirm(mcps::Confirm),
}

impl From<CombinedPendingTableAction> for DataServiceAction {
    fn from(action: CombinedPendingTableAction) -> DataServiceAction {
        match action {
            CombinedPendingTableAction::Init(x) => DataServiceAction::InitPendingTable(x),
            CombinedPendingTableAction::UpdateShort(k, i, v) => {
                DataServiceAction::SetPendingShort(k, i, v)
            }
            CombinedPendingTableAction::UpdateExtended(k, i, v) => {
                DataServiceAction::SetPendingExtended(k, i, v)
            }
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
    msdu_handles: BiMap<mcps::MsduHandle, UniqueKey>,
    pending_table: CombinedPendingTable,
    waker: WakerStore,
}

impl DataService {
    pub fn new() -> Self {
        Self {
            queues: HashMap::new(),
            msdu_handles: BiMap::new(),
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

    pub fn remove(&mut self, key: UniqueKey) -> bool {
        let mut removed = false;
        for (_, queue) in self.queues.iter_mut() {
            removed = removed || queue.remove(key);
        }
        removed
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
                            if let Some((handle, _)) = self.msdu_handles.remove_by_right(&key) {
                                return Poll::Ready(DataServiceAction::Confirm(
                                    mcps::Confirm::Data(mcps::DataConfirm {
                                        msdu_handle: handle,
                                        ack_payload: result.map_err(|e| match e {
                                            DeviceQueueError::TransactionExpired => {
                                                mcps::Error::TransactionExpired
                                            }
                                            DeviceQueueError::SendFailure => {
                                                mcps::Error::ChannelAccessFailure
                                            }
                                            DeviceQueueError::NoAck => mcps::Error::NoAck,
                                        }),
                                    }),
                                ));
                            } else {
                                // Nothing to report to outside
                                continue 'retry;
                            }
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
            // TODO:
            // If the DataRequest was Ack'd with the pending bit not set,
            // the radio of the receiving device will turn off,
            // and sending the message won't work.
            // We should implement something to check that the pending bit was set
            // TODO2:
            // If a DataRequest is received, the device should be promoted in the PendingTable,
            // such that if the pending bit was not set now, it will be on the second request.
            if let Some(queue) = self.queues.get_mut(&frame.source) {
                queue.process_datarequest();
            }
        }
    }
}

impl DataService {
    pub fn process_mcps_request(
        &mut self,
        pib: &PIB,
        request: mcps::Request,
    ) -> Option<mcps::Confirm> {
        match request {
            mcps::Request::Data(r) => self.process_mcps_data_request(pib, r),
            mcps::Request::Purge(r) => Some(self.process_mcps_purge_request(r)),
        }
    }

    fn process_mcps_data_request(
        &mut self,
        pib: &PIB,
        request: mcps::DataRequest,
    ) -> Option<mcps::Confirm> {
        let key = UniqueKey::new();
        let internal_request = DataRequest {
            key,
            destination: request.destination,
            source_mode: request.source_addressing_mode,
            acknowledge_request: request.ack_tx,
            indirect: request.indirect_tx,
            content: frame::FrameType::Data(frame::Payload(request.msdu)),
        };
        self.insert(pib, internal_request);
        self.msdu_handles.insert(request.msdu_handle, key);
        None
    }

    fn process_mcps_purge_request(&mut self, request: mcps::PurgeRequest) -> mcps::Confirm {
        if let Some((handle, key)) = self.msdu_handles.remove_by_left(&request.msdu_handle) {
            self.remove(key);
            mcps::Confirm::Purge(mcps::PurgeConfirm {
                msdu_handle: handle,
                status: Ok(()),
            })
        } else {
            mcps::Confirm::Purge(mcps::PurgeConfirm {
                msdu_handle: request.msdu_handle,
                status: Err(mcps::Error::InvalidHandle),
            })
        }
    }

    pub fn process_mcps_response(&mut self, response: mcps::Response) {
        match response {}
    }
}
