use crate::ieee802154::frame::{Address, FullAddress};
use crate::ieee802154::mac::pendingtable::PendingTable;
use crate::ieee802154::{ExtendedAddress, ShortAddress, PANID};
use crate::unique_key::UniqueKey;
use crate::waker_store::WakerStore;
use std::task::{Context, Poll};

pub enum CombinedPendingTableAction {
    Init(UniqueKey),
    UpdateShort(UniqueKey, usize, Option<(PANID, ShortAddress)>),
    UpdateExtended(UniqueKey, usize, Option<ExtendedAddress>),
}

pub struct CombinedPendingTable {
    waker: WakerStore,
    initializing: Option<UniqueKey>,
    is_initialized: bool,
    none: bool,
    short: PendingTable<(PANID, ShortAddress)>,
    extended: PendingTable<ExtendedAddress>,
}

impl CombinedPendingTable {
    pub fn new() -> Self {
        Self {
            waker: WakerStore::new(),
            initializing: None,
            is_initialized: false,
            none: false,
            short: PendingTable::<(PANID, ShortAddress)>::new(8),
            extended: PendingTable::<ExtendedAddress>::new(8),
        }
    }

    pub fn report_init_result(&mut self, key: UniqueKey, result: bool) {
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

    pub fn set(&mut self, address: &Option<FullAddress>, inserted: bool) {
        match address {
            None => self.none = inserted,
            Some(FullAddress { pan_id, address }) => match address {
                Address::Short(address) => self.short.set(&(*pan_id, *address), inserted),
                Address::Extended(address) => self.extended.set(address, inserted),
            },
        }
    }

    pub fn poll_action(&mut self, cx: &mut Context<'_>) -> Poll<CombinedPendingTableAction> {
        if self.initializing.is_some() {
            self.waker.pend(cx)
        } else if !self.is_initialized {
            let key = UniqueKey::new();
            self.initializing = Some(key);
            Poll::Ready(CombinedPendingTableAction::Init(key))
        } else if let Poll::Ready(update) = self.short.poll_update(cx) {
            Poll::Ready(CombinedPendingTableAction::UpdateShort(
                update.key,
                update.index,
                update.value,
            ))
        } else if let Poll::Ready(update) = self.extended.poll_update(cx) {
            Poll::Ready(CombinedPendingTableAction::UpdateExtended(
                update.key,
                update.index,
                update.value,
            ))
        } else {
            Poll::Pending
        }
    }

    pub fn report_update_result(&mut self, key: UniqueKey, success: bool) {
        self.short.report_update_result(key, success);
        self.extended.report_update_result(key, success);
    }
}
