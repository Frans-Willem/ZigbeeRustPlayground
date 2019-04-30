use std::cmp::Eq;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::hash::Hash;

struct PendingHandlerEntry<S> {
    pub age: usize,
    slot: S,
}

pub struct PendingHandler<S, I> {
    free_slots: VecDeque<S>,
    used_slots: HashMap<I, PendingHandlerEntry<S>>,
}

// S = Slot type
// I = Item type
impl<S, I> PendingHandler<S, I>
where
    I: Eq + Hash + Clone,
    S: Copy,
{
    pub fn new(slots: Vec<S>) -> PendingHandler<S, I> {
        PendingHandler {
            free_slots: slots.into(),
            used_slots: HashMap::new(),
        }
    }

    fn get_new_slot(&mut self) -> S {
        if let Some(slot) = self.free_slots.pop_front() {
            slot
        } else {
            let mut highest: Option<(I, usize)> = None;
            for (key, value) in self.used_slots.iter() {
                highest = match highest {
                    None => Some((key.clone(), value.age)),
                    Some((highest_key, highest_age)) => {
                        if value.age > highest_age {
                            Some((key.clone(), value.age))
                        } else {
                            Some((highest_key, highest_age))
                        }
                    }
                }
            }
            let highest = highest.unwrap();
            self.used_slots.remove(&highest.0).unwrap().slot
        }
    }

    pub fn promote(&mut self, item: I) -> Option<S> {
        match self.used_slots.get_mut(&item) {
            Some(current) => {
                if current.age != 0 {
                    for (key, val) in self.used_slots.iter_mut() {
                        if item == *key {
                            val.age = 0;
                        } else {
                            val.age = val.age + 1;
                        }
                    }
                }
                None
            }
            None => {
                let new_slot = self.get_new_slot();
                for (_, val) in self.used_slots.iter_mut() {
                    val.age = val.age + 1;
                }
                self.used_slots.insert(
                    item,
                    PendingHandlerEntry {
                        age: 0,
                        slot: new_slot,
                    },
                );
                Some(new_slot)
            }
        }
    }

    pub fn clear(&mut self, item: I) -> Option<S> {
        if let Some(PendingHandlerEntry { age: _, slot }) = self.used_slots.remove(&item) {
            self.free_slots.push_back(slot);
            Some(slot)
        } else {
            None
        }
    }
}

#[test]
fn test_pending_handler() {
    let mut handler = PendingHandler::new(vec![1, 2, 3]);
    // Insert one, remove it, should return the same slot.
    assert_eq!(Some(1), handler.promote("A"));
    assert_eq!(Some(1), handler.clear("A"));
    // Add B, C, D, taking 2, 3, 1
    assert_eq!(Some(2), handler.promote("B"));
    assert_eq!(Some(3), handler.promote("C"));
    assert_eq!(Some(1), handler.promote("D"));
    // Promote B, so C is now oldest
    assert_eq!(None, handler.promote("B"));
    // New entry should get C's slot
    assert_eq!(Some(3), handler.promote("E"));
    assert_eq!(None, handler.promote("D"));
    assert_eq!(None, handler.promote("B"));
    assert_eq!(None, handler.promote("E"));
    // If we reinsert C, it should now get D's slot
    assert_eq!(Some(1), handler.promote("C"));
}
