use crate::ieee802154::mac::frame::AddressSpecification;
use crate::ieee802154::{ExtendedAddress, ShortAddress, PANID};
use crate::mru_set::{MRUSet, MRUSetAction};

pub enum MRUAddressSetAction {
    None,
    SetShortSlot(usize, Option<(PANID, ShortAddress)>),
    SetExtendedSlot(usize, Option<ExtendedAddress>),
}

impl From<MRUSetAction<(PANID, ShortAddress)>> for MRUAddressSetAction {
    fn from(action: MRUSetAction<(PANID, ShortAddress)>) -> Self {
        match action {
            MRUSetAction::None => MRUAddressSetAction::None,
            MRUSetAction::SetSlot(slot, address) => {
                MRUAddressSetAction::SetShortSlot(slot, Some(address))
            }
            MRUSetAction::ClearSlot(slot) => MRUAddressSetAction::SetShortSlot(slot, None),
        }
    }
}

impl From<MRUSetAction<ExtendedAddress>> for MRUAddressSetAction {
    fn from(action: MRUSetAction<ExtendedAddress>) -> Self {
        match action {
            MRUSetAction::None => MRUAddressSetAction::None,
            MRUSetAction::SetSlot(slot, address) => {
                MRUAddressSetAction::SetExtendedSlot(slot, Some(address))
            }
            MRUSetAction::ClearSlot(slot) => MRUAddressSetAction::SetExtendedSlot(slot, None),
        }
    }
}

pub struct MRUAddressSet {
    short: MRUSet<(PANID, ShortAddress)>,
    extended: MRUSet<ExtendedAddress>,
}

impl MRUAddressSet {
    pub fn new(num_short_slots: usize, num_extended_slots: usize) -> MRUAddressSet {
        MRUAddressSet {
            short: MRUSet::new(num_short_slots),
            extended: MRUSet::new(num_extended_slots),
        }
    }

    pub fn contains(&self, address: &AddressSpecification) -> bool {
        match address {
            AddressSpecification::None => false,
            AddressSpecification::Short(panid, address) => {
                self.short.contains(&(panid.clone(), address.clone()))
            }
            AddressSpecification::Extended(_, address) => self.extended.contains(address),
        }
    }

    pub fn insert(&mut self, address: &AddressSpecification) -> MRUAddressSetAction {
        match address {
            AddressSpecification::None => MRUAddressSetAction::None,
            AddressSpecification::Short(panid, address) => {
                self.short.insert(&(panid.clone(), address.clone())).into()
            }
            AddressSpecification::Extended(_, address) => self.extended.insert(address).into(),
        }
    }
 
    pub fn remove(&mut self, address: &AddressSpecification) -> MRUAddressSetAction {
        match address {
            AddressSpecification::None => MRUAddressSetAction::None,
            AddressSpecification::Short(panid, address) => {
                self.short.remove(&(panid.clone(), address.clone())).into()
            }
            AddressSpecification::Extended(_, address) => self.extended.remove(address).into(),
        }
    }

    pub fn len(&self) -> usize {
        self.short.len() + self.extended.len()
    }
}
