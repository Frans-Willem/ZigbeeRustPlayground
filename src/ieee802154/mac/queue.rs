use crate::ieee802154::frame;
use crate::unique_key::UniqueKey;
use futures::stream::{FusedStream, Stream, StreamExt};
use futures::task::{Context, Poll, Waker};
use std::collections::{HashMap, VecDeque};

use crate::ieee802154::mac::pendingtable::PendingTable;
use crate::ieee802154::{ExtendedAddress, ShortAddress, PANID};
use std::pin::Pin;

// TODO:
// - Check MacQueue wake behaviour.

#[derive(Clone, Debug)]
pub struct MacQueueEntry {
    pub key: UniqueKey,
    pub destination: Option<frame::FullAddress>,
    pub source_mode: frame::AddressingMode,
    pub acknowledge_request: bool,
    pub indirect: bool,
    pub content: frame::FrameType,
}

struct MacDeviceQueue {
    queue: VecDeque<MacQueueEntry>,
    waiting_for_ack: bool,
}

impl MacDeviceQueue {
    fn new() -> Self {
        MacDeviceQueue {
            queue: VecDeque::new(),
            waiting_for_ack: false,
        }
    }

    fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    fn is_pending_indirect(&self) -> bool {
        if self.waiting_for_ack {
            if let Some(next) = self.queue.get(1) {
                next.indirect
            } else {
                false
            }
        } else if let Some(head) = self.queue.front() {
            head.indirect
        } else {
            false
        }
    }

    fn insert(&mut self, entry: MacQueueEntry) {
        self.queue.push_back(entry)
    }

    fn purge(&mut self, key: UniqueKey) {
        if let (true, Some(head)) = (self.waiting_for_ack, self.queue.front()) {
            if head.key == key {
                self.waiting_for_ack = false
            }
        }
        self.queue.retain(|e| e.key != key)
    }

    fn pop_to_send(&mut self, datarequest: bool) -> Option<(MacQueueEntry, bool)> {
        if self.waiting_for_ack {
            None
        } else if let Some(head) = self.queue.front() {
            if head.indirect == datarequest {
                if head.acknowledge_request {
                    self.waiting_for_ack = true;
                    Some((head.clone(), self.is_pending_indirect()))
                } else {
                    self.queue
                        .pop_front()
                        .map(|x| (x, self.is_pending_indirect()))
                }
            } else {
                None
            }
        } else {
            None
        }
    }
    fn acknowledge_timeout(&mut self, key: UniqueKey) {
        // NOTE: For successful acknowledgements, purge is just (ab)used
        if self.waiting_for_ack {
            if let Some(head) = self.queue.front() {
                if head.key == key {
                    self.waiting_for_ack = false;
                }
            }
        }
    }
}

pub struct MacQueue {
    frames: HashMap<UniqueKey, Option<frame::FullAddress>>,
    device_queues: HashMap<Option<frame::FullAddress>, MacDeviceQueue>,
    waker: Option<Waker>,
    pending_none: bool,
    pending_short: PendingTable<(PANID, ShortAddress)>,
    pending_extended: PendingTable<ExtendedAddress>,
}

impl MacQueue {
    pub fn new() -> MacQueue {
        MacQueue {
            frames: HashMap::new(),
            device_queues: HashMap::new(),
            waker: Option::None,
            pending_none: false,
            pending_short: PendingTable::new(8),
            pending_extended: PendingTable::new(8),
        }
    }

    pub fn is_pending_indirect(&self, destination: &Option<frame::FullAddress>) -> bool {
        match destination {
            None => self.pending_none,
            Some(frame::FullAddress { pan_id, address }) => match address {
                frame::Address::Short(address) => self.pending_short.contains(&(*pan_id, *address)),
                frame::Address::Extended(address) => self.pending_extended.contains(address),
            },
        }
    }

    pub fn promote_pending(&mut self, destination: &Option<frame::FullAddress>) -> bool {
        match destination {
            None => self.pending_none,
            Some(frame::FullAddress { pan_id, address }) => match address {
                frame::Address::Short(address) => self.pending_short.promote(&(*pan_id, *address)),
                frame::Address::Extended(address) => self.pending_extended.promote(address),
            },
        }
    }

    fn set_pending(&mut self, destination: &Option<frame::FullAddress>, pending: bool) {
        match destination {
            None => self.pending_none = pending,
            Some(frame::FullAddress { pan_id, address }) => match address {
                frame::Address::Short(address) => {
                    self.pending_short.set(&(*pan_id, *address), pending)
                }
                frame::Address::Extended(address) => self.pending_extended.set(address, pending),
            },
        }
    }

    fn wake(&mut self) {
        if let Some(waker) = self.waker.take() {
            waker.wake()
        }
    }

    pub fn purge(&mut self, key: UniqueKey) -> bool {
        if let Some(destination) = self.frames.remove(&key) {
            let mut is_pending_indirect = false;
            if let Some(device_queue) = self.device_queues.get_mut(&destination) {
                device_queue.purge(key);
                if device_queue.is_empty() {
                    self.device_queues.remove(&destination);
                } else {
                    is_pending_indirect = device_queue.is_pending_indirect();
                }
                self.wake()
            }
            self.set_pending(&destination, is_pending_indirect);
            true
        } else {
            false
        }
    }

    fn pop_to_send(&mut self) -> Option<MacQueueEntry> {
        let mut ret = None;
        for (destination, device_queue) in self.device_queues.iter_mut() {
            ret = device_queue.pop_to_send(false);
            if let Some((to_send, _)) = &ret {
                if device_queue.is_empty() {
                    let destination = *destination;
                    self.device_queues.remove(&destination);
                }
                if to_send.acknowledge_request {
                    self.frames.remove(&to_send.key);
                }
                break;
            }
        }
        if let Some((to_send, is_pending_indirect)) = ret {
            self.set_pending(&to_send.destination, is_pending_indirect);
            self.wake();
            Some(to_send)
        } else {
            None
        }
    }

    pub fn pop_datarequest(
        &mut self,
        destination: &Option<frame::FullAddress>,
    ) -> Option<MacQueueEntry> {
        // TODO: If the pending_data bit wasn't set in the DataRequest ACK,
        // the device's radio will probably turn off.
        // We should check if the device is in the current pending data table, and has been
        // correctly flushed to the radio device, before attempting to send a packet.
        let mut ret = None;
        if let Some(device_queue) = self.device_queues.get_mut(destination) {
            ret = device_queue.pop_to_send(true);
            if device_queue.is_empty() {
                self.device_queues.remove(destination);
            }
            if let Some((to_send, _)) = &ret {
                if !to_send.acknowledge_request {
                    self.frames.remove(&to_send.key);
                }
            }
        }
        if let Some((to_send, is_pending_indirect)) = ret {
            self.set_pending(&to_send.destination, is_pending_indirect);
            self.wake();
            Some(to_send)
        } else {
            None
        }
    }

    pub fn insert(&mut self, entry: MacQueueEntry) -> bool {
        let key = entry.key;
        let destination = entry.destination;
        if let Some(old_destination) = self.frames.insert(key, destination) {
            self.frames.insert(key, old_destination);
            false
        } else {
            let is_pending_indirect;
            if let Some(device_queue) = self.device_queues.get_mut(&destination) {
                device_queue.insert(entry);
                is_pending_indirect = device_queue.is_pending_indirect();
            } else {
                let mut new_queue = MacDeviceQueue::new();
                new_queue.insert(entry);
                is_pending_indirect = new_queue.is_pending_indirect();
                self.device_queues.insert(destination, new_queue);
            }
            self.set_pending(&destination, is_pending_indirect);
            self.wake();
            true
        }
    }

    pub fn report_set_pending_short_result(&mut self, key: UniqueKey, result: bool) {
        self.pending_short.report_update_result(key, result)
    }
    pub fn report_set_pending_extended_result(&mut self, key: UniqueKey, result: bool) {
        self.pending_extended.report_update_result(key, result)
    }

    pub fn acknowledge(&mut self, key: UniqueKey) {
        self.purge(key);
    }

    pub fn acknowledge_timeout(&mut self, key: UniqueKey) {
        if let Some(destination) = self.frames.get(&key) {
            let mut is_pending_indirect = false;
            if let Some(device_queue) = self.device_queues.get_mut(&destination) {
                device_queue.acknowledge_timeout(key);
                is_pending_indirect = device_queue.is_pending_indirect();
            }
            let destination = *destination;
            self.set_pending(&destination, is_pending_indirect);
            self.wake();
        }
    }
}

#[derive(Debug)]
pub enum MacQueueAction {
    Send(MacQueueEntry),
    SetPendingShort(UniqueKey, usize, Option<(PANID, ShortAddress)>),
    SetPendingExtended(UniqueKey, usize, Option<ExtendedAddress>),
}

impl Stream for MacQueue {
    type Item = MacQueueAction;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = Pin::into_inner(self);
        if let Some(item) = this.pop_to_send() {
            return Poll::Ready(Some(MacQueueAction::Send(item)));
        }
        if let Poll::Ready(Some(update)) = this.pending_short.poll_next_unpin(cx) {
            return Poll::Ready(Some(MacQueueAction::SetPendingShort(
                update.key,
                update.index,
                update.value,
            )));
        }
        if let Poll::Ready(Some(update)) = this.pending_extended.poll_next_unpin(cx) {
            return Poll::Ready(Some(MacQueueAction::SetPendingExtended(
                update.key,
                update.index,
                update.value,
            )));
        }
        this.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

impl FusedStream for MacQueue {
    fn is_terminated(&self) -> bool {
        false
    }
}
