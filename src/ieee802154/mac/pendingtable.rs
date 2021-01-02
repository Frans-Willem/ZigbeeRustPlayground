use crate::unique_key::UniqueKey;
use futures::stream::{FusedStream, Stream};
use futures::task::{Context, Poll, Waker};
use std::collections::HashSet;
use std::hash::Hash;
use std::pin::Pin;

struct PendingTableEntry<T> {
    dirty: bool,
    value: Option<T>,
}

impl<T> Default for PendingTableEntry<T> {
    fn default() -> Self {
        PendingTableEntry {
            dirty: true,
            value: None,
        }
    }
}
impl<T> PendingTableEntry<T> {
    fn new(value: T) -> Self {
        PendingTableEntry {
            dirty: true,
            value: Some(value),
        }
    }
}

pub struct PendingTable<T: Clone + Hash + PartialEq + Eq + Unpin> {
    values: HashSet<T>,
    table: Vec<PendingTableEntry<T>>,
    order: Vec<usize>, // First index in this table is first to be overwritten
    updating: Option<(UniqueKey, usize)>,
    waker: Option<Waker>,
}

impl<T: Clone + Hash + PartialEq + Eq + Unpin> PendingTable<T> {
    pub fn new(size: usize) -> Self {
        let mut table = Vec::with_capacity(size);
        table.resize_with(size, Default::default);
        PendingTable {
            values: HashSet::new(),
            table,
            order: (0..size).collect(),
            updating: None,
            waker: None,
        }
    }

    pub fn assume_empty(&mut self) {
        for entry in self.table.iter_mut() {
            entry.dirty = entry.value.is_some();
        }
        self.wake();
    }

    fn wake(&mut self) {
        if let Some(waker) = self.waker.take() {
            waker.wake()
        }
    }

    /**
     * Gets index in table for value, or None if not present in table.
     */
    fn get_index(&self, value: &T) -> Option<usize> {
        for index in 0..self.table.len() {
            if let Some(current_value) = &self.table[index].value {
                if current_value == value {
                    return Some(index);
                }
            }
        }
        None
    }

    /**
     * Promotes index to the last entry to be overwritten
     */
    fn promote_index(&mut self, index: usize) {
        self.order.retain(|x| x != &index);
        self.order.push(index);
    }

    /**
     * Demotes index to the first entry to be overwritten
     */
    fn demote_index(&mut self, index: usize) {
        self.order.retain(|x| x != &index);
        self.order.insert(0, index);
    }

    /**
     * Promotes value, if present in set,
     * ensures it is in the table, and promotes it to be the last to be overwritten.
     */
    pub fn promote(&mut self, value: &T) -> bool {
        if self.values.contains(value) {
            let index = if let Some(current_index) = self.get_index(&value) {
                current_index
            } else {
                let next_index = *self.order.first().unwrap();
                self.table[next_index] = PendingTableEntry::new(value.clone());
                if self.updating.is_none() {
                    self.wake();
                }
                next_index
            };
            self.promote_index(index);
            true
        } else {
            false
        }
    }

    pub fn set(&mut self, value: &T, inserted: bool) {
        if inserted {
            self.insert(value.clone());
        } else {
            self.remove(value);
        }
    }

    /**
     * Inserts new item, and promotes it.
     */
    pub fn insert(&mut self, value: T) -> bool {
        let inserted = self.values.insert(value.clone());
        self.promote(&value);
        inserted
    }

    pub fn contains(&self, value: &T) -> bool {
        self.values.contains(value)
    }

    pub fn remove(&mut self, value: &T) -> bool {
        if self.values.remove(value) {
            if let Some(index) = self.get_index(value) {
                self.table[index] = Default::default();
                if self.updating.is_none() {
                    self.wake();
                }
                self.demote_index(index);
            }
            true
        } else {
            false
        }
    }

    /*
    pub fn pop_update(&mut self) -> Option<(usize, Option<T>)> {
        if self.updating.is_some() {
            return None
        }
    }
    */

    pub fn report_update_result(&mut self, token: UniqueKey, result: bool) {
        if let Some((current_token, index)) = self.updating {
            if token == current_token {
                self.updating = None;
                if !result {
                    self.table[index].dirty = true;
                }
                self.wake();
            }
        }
    }
    pub fn poll_update(&mut self, cx: &mut Context<'_>) -> Poll<PendingTableUpdate<T>> {
        if self.updating.is_none() {
            for index in 0..self.table.len() {
                if self.table[index].dirty {
                    self.table[index].dirty = false;
                    let value = self.table[index].value.clone();
                    let key = UniqueKey::new();
                    self.updating = Some((key, index));
                    return Poll::Ready(PendingTableUpdate { key, index, value });
                }
            }
        }
        self.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

pub struct PendingTableUpdate<T> {
    pub key: UniqueKey,
    pub index: usize,
    pub value: Option<T>,
}

impl<T: Clone + Hash + PartialEq + Eq + Unpin> Stream for PendingTable<T> {
    type Item = PendingTableUpdate<T>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::into_inner(self).poll_update(cx) {
            Poll::Ready(x) => Poll::Ready(Some(x)),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<T: Clone + Hash + PartialEq + Eq + Unpin> FusedStream for PendingTable<T> {
    fn is_terminated(&self) -> bool {
        false
    }
}
