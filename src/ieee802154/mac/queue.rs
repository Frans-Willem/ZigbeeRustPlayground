use crate::ieee802154::mac::data;
use crate::unique_key::UniqueKey;
use futures::stream::{FusedStream, Stream, StreamExt};
use futures::task::{Context, Poll, Waker};
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::Hash;
use std::pin::Pin;
use crate::ieee802154::mac::pendingtable::PendingTable;
use crate::ieee802154::{PANID, ShortAddress, ExtendedAddress};

#[derive(Clone, Debug)]
pub struct MacQueueEntry {
    pub key: UniqueKey,
    pub destination: Option<data::FullAddress>,
    pub source_mode: data::AddressingMode,
    pub acknowledge_request: bool,
    pub indirect: bool,
    pub content: data::FrameType,
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
                    //self.waiting_for_ack = true;
                    Some((head.clone(), self.is_pending_indirect()))
                } else {
                    self.queue.pop_front().map(|x| (x, self.is_pending_indirect()))
                }
            } else {
                None
            }
        } else {
            None
        }
    }

    fn acknowledge_timeout(&mut self) {
        self.waiting_for_ack = false;
    }

    fn acknowledge(&mut self, key: UniqueKey) {
        if self.waiting_for_ack {
            if let Some(head) = self.queue.front() {
                if head.key == key {
                    self.waiting_for_ack = false;
                    self.queue.pop_front();
                }
            }
        }
    }
}

pub struct MacQueue {
    frames: HashMap<UniqueKey, Option<data::FullAddress>>,
    device_queues: HashMap<Option<data::FullAddress>, MacDeviceQueue>,
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

    pub fn is_pending_indirect(&self, destination: &Option<data::FullAddress>,) -> bool {
        match destination {
            None => self.pending_none,
            Some(data::FullAddress { pan_id, address }) => {
                match address {

                    data::Address::Short(address) => self.pending_short.contains(&(pan_id.clone(), address.clone())),
                    data::Address::Extended(address) => self.pending_extended.contains(address),
                }
            }
        }
    }

    pub fn promote_pending(&mut self, destination: &Option<data::FullAddress>,) -> bool {
        match destination {
            None => self.pending_none,
            Some(data::FullAddress { pan_id, address }) => {
                match address {

                    data::Address::Short(address) => self.pending_short.promote(&(pan_id.clone(), address.clone())),
                    data::Address::Extended(address) => self.pending_extended.promote(address),
                }
            }
        }
    }

    fn set_pending(&mut self, destination: &Option<data::FullAddress>, pending: bool) {
        match destination {
            None => self.pending_none = pending,
            Some(data::FullAddress { pan_id, address }) => {
                match address {

                    data::Address::Short(address) => self.pending_short.set(&(pan_id.clone(), address.clone()), pending),
                    data::Address::Extended(address) => self.pending_extended.set(address, pending),
                }
            }
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
                    let destination = destination.clone();
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
        destination: &Option<data::FullAddress>,
    ) -> Option<MacQueueEntry> {
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
            return Poll::Ready(Some(MacQueueAction::SetPendingShort(update.key, update.index, update.value)));
        }
        if let Poll::Ready(Some(update)) = this.pending_extended.poll_next_unpin(cx) {
            return Poll::Ready(Some(MacQueueAction::SetPendingExtended(update.key, update.index, update.value)));
        }
        /*
        if let Some((index, address)) = this.pending_short.pop_update() {
            return Poll::Ready(Some(MacQueueAction::SetPendingShort(index, address)));
        }
        if let Some((index, address)) = this.pending_extended.pop_update() {
            return Poll::Ready(Some(MacQueueAction::SetPendingExtended(index, address)));
        }
        */
        this.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

impl FusedStream for MacQueue {
    fn is_terminated(&self) -> bool {
        false
    }
}
