use crate::delay_queue::DelayQueue;
use crate::ieee802154::frame;
use crate::ieee802154::mac::data::DataRequest;
use crate::ieee802154::pib::PIB;
use crate::unique_key::UniqueKey;
use crate::waker_store::WakerStore;
use futures::future::BoxFuture;
use std::collections::VecDeque;
use std::task::{Context, Poll};

enum DeviceQueueState {
    Idle {
        // Idle, ready to send next frame.
        datarequest: bool,
    },
    Sending {
        // Sending out a frame, waiting for result.
        send_key: UniqueKey,
        ack_requested: Option<u8>,
    },
    WaitingForAck {
        // Waiting for an Ack frame
        ack_requested: u8,
        timeout: BoxFuture<'static, ()>,
    },
    HaveResult {
        result: Result<Vec<u8>, DeviceQueueError>,
    }, // First entry in queue has result, ReportResult should be triggered.
}
struct DeviceQueueEntry {
    data: DataRequest,
    retries_left: usize,
    timeout: BoxFuture<'static, ()>,
}
pub struct DeviceQueue {
    state: DeviceQueueState,
    last_pending_reported: Option<bool>,
    entries: VecDeque<DeviceQueueEntry>,
    waker: WakerStore,
}

#[derive(Debug)]
pub enum DeviceQueueError {
    TransactionExpired, // Frame was not polled within the time allocated
    SendFailure,        // After several tries
    NoAck,              // After several tries
}

pub enum DeviceQueueAction {
    Empty(),                            // Device queue is empty, and should be discarded.
    SetPending(bool),                   // Pending bit should be set.
    SendFrame(UniqueKey, frame::Frame), // Frame should be sent out.
    ReportResult(UniqueKey, Result<Vec<u8>, DeviceQueueError>), // Frame was fully sent.
}

impl DeviceQueue {
    pub fn new() -> Self {
        Self {
            state: DeviceQueueState::Idle { datarequest: false },
            last_pending_reported: None,
            entries: VecDeque::new(),
            waker: WakerStore::new(),
        }
    }

    fn contains(&self, key: UniqueKey) -> bool {
        for entry in self.entries.iter() {
            if entry.data.key == key {
                return true;
            }
        }
        false
    }

    pub fn insert(&mut self, pib: &PIB, entry: DataRequest) -> bool {
        if self.contains(entry.key) {
            return false;
        }
        self.entries.push_back(DeviceQueueEntry {
            data: entry,
            retries_left: pib.mac_max_frame_retries as usize,
            timeout: Box::pin(async_std::task::sleep(pib.mac_transaction_persistence_time)),
        });
        self.waker.wake();
        true
    }

    pub fn process_datarequest(&mut self) {
        if let DeviceQueueState::Idle { .. } = self.state {
            self.state = DeviceQueueState::Idle { datarequest: true };
        }
        self.waker.wake();
    }

    fn create_frame(request: &DataRequest, pib: &mut PIB) -> (Option<u8>, frame::Frame) {
        let source = match request.source_mode {
            frame::AddressingMode::None => None,
            frame::AddressingMode::Reserved => None,
            frame::AddressingMode::Short => Some(pib.get_full_short_address()),
            frame::AddressingMode::Extended => Some(pib.get_full_extended_address()),
        };
        let sequence_nr = pib.next_data_sequence_nr();
        let frame = frame::Frame {
            frame_pending: false, // TODO: Check this?
            acknowledge_request: request.acknowledge_request,
            sequence_number: Some(sequence_nr),
            destination: request.destination,
            source,
            frame_type: request.content.clone(),
        };
        let ack_request = if request.acknowledge_request {
            Some(sequence_nr)
        } else {
            None
        };
        (ack_request, frame)
    }

    pub fn process_send_result(&mut self, key: UniqueKey, success: bool) {
        if let DeviceQueueState::Sending {
            send_key,
            ack_requested,
        } = self.state
        {
            if send_key == key {
                if success {
                    self.state = DeviceQueueState::HaveResult {
                        result: Ok(Vec::new()),
                    };
                } else {
                    if let Some(front_entry) = self.entries.front_mut() {
                        if front_entry.data.indirect {
                            self.state = DeviceQueueState::Idle { datarequest: false };
                        } else if front_entry.retries_left > 0 {
                            front_entry.retries_left = front_entry.retries_left - 1;
                            self.state = DeviceQueueState::Idle { datarequest: false };
                        } else {
                            self.state = DeviceQueueState::HaveResult {
                                result: Err(DeviceQueueError::SendFailure),
                            };
                        }
                    } else {
                        self.state = DeviceQueueState::Idle { datarequest: false };
                    }
                }
            }
        }
    }

    pub fn process_acknowledge(&mut self, seq_nr: Option<u8>, payload: &Vec<u8>) {
        if let DeviceQueueState::WaitingForAck { ack_requested, .. } = self.state {
            if seq_nr == Some(ack_requested) {
                self.state = DeviceQueueState::HaveResult {
                    result: Ok(payload.clone()),
                }
            }
        } else if let DeviceQueueState::Sending {
            send_key,
            ack_requested,
        } = self.state
        {
            if ack_requested == seq_nr {
                self.state = DeviceQueueState::Sending {
                    send_key,
                    ack_requested: None,
                };
            }
        }
    }

    pub fn poll_next_action(
        &mut self,
        pib: &mut PIB,
        cx: &mut Context<'_>,
    ) -> Poll<DeviceQueueAction> {
        let should_be_pending = self
            .entries
            .front()
            .map_or(false, |entry| entry.data.indirect);
        if self.last_pending_reported != Some(should_be_pending) {
            self.last_pending_reported = Some(should_be_pending);
            return Poll::Ready(DeviceQueueAction::SetPending(should_be_pending));
        }
        if let Some(front_entry) = self.entries.front() {
            // As we're moving stuff out of the state, replace it with the idle state
            // If another state is required, it should be set from the match arms.
            match std::mem::replace(
                &mut self.state,
                DeviceQueueState::Idle { datarequest: false },
            ) {
                DeviceQueueState::Idle { datarequest } => {
                    // If idle, and the front entry is not indirect, or a datarequest was received,
                    // send out a frame.
                    if !front_entry.data.indirect || datarequest {
                        let (ack_requested, frame) =
                            DeviceQueue::create_frame(&front_entry.data, pib);
                        let send_key = UniqueKey::new();
                        self.state = DeviceQueueState::Sending {
                            send_key,
                            ack_requested,
                        };
                        return Poll::Ready(DeviceQueueAction::SendFrame(send_key, frame));
                    }
                }
                DeviceQueueState::WaitingForAck {
                    ack_requested,
                    mut timeout,
                } => {
                    if let Poll::Ready(_) = timeout.as_mut().poll(cx) {
                        // If timeout expired
                        if front_entry.data.indirect {
                            // Do nothing, just go back to Idle, an attempt to send will be done
                            // later.
                        } else if front_entry.retries_left > 0 {
                            // Lower retries counter, and go back to idle to retry
                            let front_entry = self.entries.front_mut().unwrap();
                            front_entry.retries_left = front_entry.retries_left - 1;
                        } else {
                            // Remove entry, report result as failed.
                            let key = front_entry.data.key;
                            let result = Err(DeviceQueueError::NoAck);
                            self.entries.pop_front();
                            return Poll::Ready(DeviceQueueAction::ReportResult(key, result));
                        }
                    } else {
                        // No timeout, just keep waiting for the acknowledge
                        self.state = DeviceQueueState::WaitingForAck {
                            ack_requested,
                            timeout,
                        };
                    }
                }
                DeviceQueueState::HaveResult { result } => {
                    let key = front_entry.data.key;
                    self.entries.pop_front();
                    return Poll::Ready(DeviceQueueAction::ReportResult(key, result));
                }
                s => self.state = s,
            }
            // Timeout handling
            for index in (0..self.entries.len()) {
                let entry = &mut self.entries[index];
                if let Poll::Ready(_) = entry.timeout.as_mut().poll(cx) {
                    let key = entry.data.key;
                    let result = Err(DeviceQueueError::TransactionExpired);
                    self.entries.remove(index);
                    if index == 0 {
                        self.state = DeviceQueueState::Idle { datarequest: false };
                    }
                    return Poll::Ready(DeviceQueueAction::ReportResult(key, result));
                }
            }
            // Otherwise, just wait until something changes.
            self.waker.pend(cx)
        } else {
            self.state = DeviceQueueState::Idle { datarequest: false };
            Poll::Ready(DeviceQueueAction::Empty())
        }
    }
}
