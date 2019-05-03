/**
 * Implements a set that keeps track of the N most-recently inserted/reinserted nodes for you.
 * The CC2531 chip has 24 address slots to keep track of which nodes still have data pending,
 * in an effort to extend the number of supported devices, this set was created.
 * On the PC side we have the full set of devices that we have data for,
 * on the MCU side we keep the 24 last-seen devices that have pending data.
 * As nodes often request data several times when waking up, it's OK to have a false negative
 * response the first time, as long as the second time the on-MCU cache is updated.
 */
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::hash::Hash;

#[derive(Debug)]
pub struct MRUSet<T>
where
    T: Eq + Hash + Clone,
{
    entries: HashSet<T>,               // All entries in this set
    assigned_slots: HashMap<T, usize>, // Entries with slots assigned.
    link_newer: HashMap<T, T>,         // Links from entry to an entry one newer
    link_older: HashMap<T, T>,         // Links from an entry to one older
    newest: Option<T>,
    oldest_slotted: Option<T>,
    free_slots: VecDeque<usize>, // List of free slots.
}

#[derive(PartialEq, Debug)]
pub enum MRUSetAction<T> {
    None,
    ClearSlot(usize),
    SetSlot(usize, T),
}

trait InsertOrRemove<K, V> {
    fn insert_or_remove(&mut self, key: &K, value: Option<V>) -> Option<V>;
}

impl<K, V> InsertOrRemove<K, V> for HashMap<K, V>
where
    K: Eq + Hash + Clone,
{
    fn insert_or_remove(&mut self, key: &K, value: Option<V>) -> Option<V> {
        match value {
            Some(value) => self.insert(key.clone(), value),
            None => self.remove(key),
        }
    }
}

impl<T> MRUSet<T>
where
    T: Hash + Eq + Clone,
{
    pub fn new(num_slots: usize) -> MRUSet<T> {
        assert_ne!(num_slots, 0);
        MRUSet {
            entries: HashSet::new(),
            link_newer: HashMap::new(),
            link_older: HashMap::new(),
            assigned_slots: HashMap::new(),
            newest: None,
            oldest_slotted: None,
            free_slots: (0..num_slots).collect(),
        }
    }

    pub fn contains(&self, item: &T) -> bool {
        self.entries.contains(item)
    }

    fn unlink(&mut self, item: &T) {
        // Remove links
        let newer = self.link_newer.remove(item);
        let older = self.link_older.remove(item);

        // Update newer and older items to no longer point to us.
        if let Some(newer) = newer.as_ref() {
            assert!(self.link_older.insert_or_remove(newer, older.clone()) == Some(item.clone()));
        }
        if let Some(older) = older.as_ref() {
            assert!(self.link_newer.insert_or_remove(older, newer.clone()) == Some(item.clone()));
        }

        // Update newest & oldest slotted
        if self.newest.as_ref() == Some(item) {
            self.newest = older;
        }

        if self.oldest_slotted.as_ref() == Some(item) {
            self.oldest_slotted = newer;
        }
    }

    fn link_as_newest(&mut self, item: &T) {
        let second_newest = self.newest.replace(item.clone());

        if let Some(second_newest) = second_newest.as_ref() {
            assert!(self
                .link_newer
                .insert(second_newest.clone(), item.clone())
                .is_none());
        }
        assert!(self
            .link_older
            .insert_or_remove(item, second_newest)
            .is_none());
        // Update oldest slotted
        if let None = self.oldest_slotted {
            self.oldest_slotted = Some(item.clone())
        }
    }

    fn take_oldest_slot(&mut self) -> Option<usize> {
        if let Some(slot) = self.free_slots.pop_front() {
            Some(slot)
        } else {
            // Given that we have more than one slot, and there are no free ones,
            // there must be an entry in self.oldest_slotted, and that entry must exist.
            let oldest_slotted = self.oldest_slotted.take()?;
            self.oldest_slotted = self.link_newer.get(&oldest_slotted).cloned();
            self.assigned_slots.remove(&oldest_slotted)
        }
    }

    pub fn has_slot(&self, item: &T) -> Option<usize> {
        self.assigned_slots.get(item).cloned()
    }

    pub fn insert(&mut self, item: &T) -> MRUSetAction<T> {
        // Are we already the newest entry ?
        // If so, there's no need to do anything.
        if self.newest.as_ref() == Some(item) {
            MRUSetAction::None
        } else {
            self.entries.insert(item.clone());
            self.unlink(item);
            let (retval, slot) = if let Some(assigned_slot) = self.assigned_slots.remove(item) {
                (MRUSetAction::None, assigned_slot)
            } else {
                let slot = self.take_oldest_slot().unwrap();
                (MRUSetAction::SetSlot(slot, item.clone()), slot)
            };
            self.assigned_slots.insert(item.clone(), slot);
            self.link_as_newest(item);
            retval
        }
    }

    /**
     * Tries to assign a (newly recovered) slot to an item.
     * upon returning Some(...), this slot is then taken by that item.
     * upon returning None, this slot will have been added to free_slots
     *
     */
    fn reassign_slot(&mut self, slot: usize) -> Option<T> {
        if let Some(next_in_line) = self
            .oldest_slotted
            .as_ref()
            .and_then(|item| self.link_older.get(item))
        {
            let next_in_line = next_in_line.clone();
            self.assigned_slots.insert(next_in_line.clone(), slot);
            self.oldest_slotted.replace(next_in_line.clone());
            Some(next_in_line)
        } else {
            self.free_slots.push_back(slot);
            None
        }
    }

    pub fn remove(&mut self, item: &T) -> MRUSetAction<T> {
        self.unlink(item);
        self.entries.remove(item);
        if let Some(slot) = self.assigned_slots.remove(item) {
            match self.reassign_slot(slot) {
                Some(new_item) => MRUSetAction::SetSlot(slot, new_item),
                None => MRUSetAction::ClearSlot(slot),
            }
        } else {
            MRUSetAction::None
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

#[test]
fn test_mru_set() {
    let mut set: MRUSet<char> = MRUSet::new(3);
    assert_eq!(3, set.free_slots.len());
    // Insert, reinsert, remove
    assert_eq!(MRUSetAction::SetSlot(0, 'A'), set.insert(&'A'));
    assert_eq!(MRUSetAction::None, set.insert(&'A'));
    assert_eq!(MRUSetAction::ClearSlot(0), set.remove(&'A'));
    // Insert B, C, D, taking 2, 3, 0
    assert_eq!(MRUSetAction::SetSlot(1, 'B'), set.insert(&'B'));
    assert_eq!(MRUSetAction::SetSlot(2, 'C'), set.insert(&'C'));
    assert_eq!(MRUSetAction::SetSlot(0, 'D'), set.insert(&'D'));
    // Promote B, so C should now be oldest
    assert_eq!(MRUSetAction::None, set.insert(&'B'));
    // New entry should get C's slot
    assert_eq!(MRUSetAction::SetSlot(2, 'E'), set.insert(&'E'));
    // C should get it's slot back
    assert_eq!(MRUSetAction::SetSlot(2, 'C'), set.remove(&'E'));
    // And lose it again
    assert_eq!(MRUSetAction::SetSlot(2, 'E'), set.insert(&'E'));
}
