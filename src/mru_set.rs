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
use std::collections::VecDeque;
use std::hash::Hash;

#[derive(Debug)]
pub struct MRUSetEntry<T> {
    value: T,
    newer: Option<T>,    // Points to first newer entry after this.
    older: Option<T>,    // Points to first older entry after this.
    slot: Option<usize>, // Slot used by this entry.
}

#[derive(Debug)]
pub struct MRUSet<T> where T: Eq + Hash {
    map: HashMap<T, MRUSetEntry<T>>, // Entries with data
    newest: Option<T>,               // Points to the newest entry
    oldest_slotted: Option<T>,       // Points to the oldest entry with a slot.
    free_slots: VecDeque<usize>,     // List of free slots.
}

#[derive(PartialEq, Debug)]
pub enum MRUSetAction<T> {
    None,
    ClearSlot(usize),
    SetSlot(usize, T),
}

impl<T> MRUSet<T>
where
    T: Hash + Eq + Clone,
{
    pub fn new(num_slots: usize) -> MRUSet<T> {
        assert_ne!(num_slots, 0);
        MRUSet {
            map: HashMap::new(),
            newest: None,
            oldest_slotted: None,
            free_slots: (0..num_slots).collect(),
        }
    }

    pub fn contains(&self, item: &T) -> bool {
        self.map.contains_key(item)
    }

    fn unlink(&mut self, item: &T) -> bool {
        // Reset older & newer on entry, and temporarily store their old values.
        let (existed, newer, older) = if let Some(entry) = self.map.get_mut(item) {
            (true, entry.newer.take(), entry.older.take())
        } else {
            (false, None, None)
        };
        // Update newer and older entry
        if let Some(newer) = newer.clone() {
            if let Some(newer_entry) = self.map.get_mut(&newer) {
                newer_entry.older = older.clone();
            }
        }
        if let Some(older) = older.clone() {
            if let Some(older_entry) = self.map.get_mut(&older) {
                older_entry.newer = newer.clone();
            }
        }
        // Update newest & oldest_slotted entries
        if self.newest == Some(item.clone()) {
            self.newest = older;
        }
        if self.oldest_slotted == Some(item.clone()) {
            self.oldest_slotted = newer;
        }
        existed
    }

    fn link_as_newest(&mut self, item: &T) -> MRUSetAction<T> {
        let second_newest = self.newest.replace(item.clone());
        let (slot, action) = if let Some(current_slot) = self.has_slot(item) {
            (Some(current_slot), MRUSetAction::None)
        } else {
            if let Some(new_slot) = self.take_oldest_slot() {
                (
                    Some(new_slot),
                    MRUSetAction::SetSlot(new_slot, item.clone()),
                )
            } else {
                eprintln!("Unable to assign slot!");
                (None, MRUSetAction::None)
            }
        };

        if let Some(entry) = self.map.get_mut(item) {
            assert!(entry.newer.is_none());
            assert!(entry.older.is_none());
            entry.slot = slot;
            entry.older = second_newest.clone();
        } else {
            self.map.insert(
                item.clone(),
                MRUSetEntry {
                    value: item.clone(),
                    newer: None,
                    older: second_newest.clone(),
                    slot,
                },
            );
        }
        if let Some(second_newest_entry) = second_newest.and_then(|second_newest| self.map.get_mut(&second_newest)) {
            assert!(second_newest_entry.newer.is_none());
            second_newest_entry.newer.replace(item.clone());
        }
        if let None = self.oldest_slotted {
            self.oldest_slotted = Some(item.clone())
        }

        action
    }

    fn take_oldest_slot(&mut self) -> Option<usize> {
        if let Some(slot) = self.free_slots.pop_front() {
            Some(slot)
        } else {
            // Given that we have more than one slot, and there are no free ones,
            // there must be an entry in self.oldest_slotted, and that entry must exist.
            let oldest = self.oldest_slotted.take()?;
            let oldest_entry = self.map.get_mut(&oldest)?;
            self.oldest_slotted = oldest_entry.newer.clone();
            oldest_entry.slot.take()
        }
    }

    pub fn has_slot(&self, item: &T) -> Option<usize> {
        self.map.get(item).and_then(|entry| entry.slot)
    }

    pub fn insert(&mut self, item: &T) -> MRUSetAction<T> {
        // Are we already the newest entry ?
        // If so, there's no need to do anything.
        if self.newest == Some(item.clone()) {
            MRUSetAction::None
        } else {
            self.unlink(item);
            self.link_as_newest(item)
        }
    }

    /**
     * Tries to assign a (newly recovered) slot to an item.
     * upon returning Some(...), this slot is then taken by that item.
     * upon returning None, this slot will have been added to free_slots
     *
     */
    fn reassign_slot(&mut self, slot: usize) -> Option<T> {
        if let Some(next_item_in_line) = self.oldest_slotted.clone().and_then(|item| self.map.get(&item)).and_then(|entry| entry.older.clone()) {
            let entry = self.map.get_mut(&next_item_in_line).unwrap();
            assert!(entry.slot.is_none());
            entry.slot.replace(slot);
            self.oldest_slotted.replace(next_item_in_line.clone());
            Some(next_item_in_line)
        } else {
            self.free_slots.push_back(slot);
            None
        }
    }

    pub fn remove(&mut self, item: &T) -> MRUSetAction<T> {
        self.unlink(item);
        let slot = self.map.remove(item).and_then(|entry| entry.slot);
        if let Some(slot) = slot {
            match self.reassign_slot(slot) {
                Some(new_item) => MRUSetAction::SetSlot(slot, new_item),
                None => MRUSetAction::ClearSlot(slot),
            }
        } else {
            // Removed entry didn't have a slot, so no action needed
            MRUSetAction::None
        }
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
