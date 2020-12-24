use crate::ieee802154::mac::mlme;
use crate::ieee802154::{ExtendedAddress, ShortAddress, PANID};
use rand::random;
use std::convert::TryInto;

/**
 * Implements a PIB as described in 8.4 of 802.15.4-2015 standard
 * Only properties relevant to this implementation are implemented.
 */
#[derive(Debug, Clone, Copy)]
pub enum PIBProperty {
    MacExtendedAddress,
    MacAssociatedPanCoord,
    MacAssociationPermit,
    MacBeaconPayload,
    MacBsn,
    MacDsn,
    MacPanId,
    MacShortAddress,
    MacBeaconAutoRespond,
    PhyCurrentChannel,
    PhyMaxTxPower,
    PhyTxPower,
}

#[derive(Debug, Clone)]
pub enum PIBValue {
    Bool(bool),
    U8(u8),
    U16(u16),
    Blob(Vec<u8>),
    ShortAddress(ShortAddress),
    ExtendedAddress(ExtendedAddress),
    PANID(PANID),
    Pair(Box<PIBValue>, Box<PIBValue>),
    Empty,
}

impl From<bool> for PIBValue {
    fn from(value: bool) -> PIBValue {
        PIBValue::Bool(value)
    }
}
impl From<u8> for PIBValue {
    fn from(value: u8) -> PIBValue {
        PIBValue::U8(value)
    }
}
impl From<u16> for PIBValue {
    fn from(value: u16) -> PIBValue {
        PIBValue::U16(value)
    }
}
impl From<Vec<u8>> for PIBValue {
    fn from(value: Vec<u8>) -> PIBValue {
        PIBValue::Blob(value)
    }
}
impl From<ShortAddress> for PIBValue {
    fn from(value: ShortAddress) -> PIBValue {
        PIBValue::ShortAddress(value)
    }
}
impl From<ExtendedAddress> for PIBValue {
    fn from(value: ExtendedAddress) -> PIBValue {
        PIBValue::ExtendedAddress(value)
    }
}
impl From<PANID> for PIBValue {
    fn from(value: PANID) -> PIBValue {
        PIBValue::PANID(value)
    }
}
impl<A, B> From<(A, B)> for PIBValue
where
    PIBValue: From<A> + From<B>,
{
    fn from(value: (A, B)) -> PIBValue {
        PIBValue::Pair(Box::new(value.0.into()), Box::new(value.1.into()))
    }
}

impl<T> From<Option<T>> for PIBValue
where
    PIBValue: From<T>,
{
    fn from(value: Option<T>) -> PIBValue {
        match value {
            Option::Some(x) => x.into(),
            Option::None => PIBValue::Empty,
        }
    }
}

impl TryInto<bool> for PIBValue {
    type Error = ();
    fn try_into(self) -> Result<bool, Self::Error> {
        if let PIBValue::Bool(x) = self {
            Ok(x)
        } else {
            Err(())
        }
    }
}
impl TryInto<u8> for PIBValue {
    type Error = ();
    fn try_into(self) -> Result<u8, Self::Error> {
        if let PIBValue::U8(x) = self {
            Ok(x)
        } else {
            Err(())
        }
    }
}
impl TryInto<u16> for PIBValue {
    type Error = ();
    fn try_into(self) -> Result<u16, Self::Error> {
        if let PIBValue::U16(x) = self {
            Ok(x)
        } else {
            Err(())
        }
    }
}
impl TryInto<Vec<u8>> for PIBValue {
    type Error = ();
    fn try_into(self) -> Result<Vec<u8>, Self::Error> {
        if let PIBValue::Blob(x) = self {
            Ok(x)
        } else {
            Err(())
        }
    }
}
impl TryInto<ShortAddress> for PIBValue {
    type Error = ();
    fn try_into(self) -> Result<ShortAddress, Self::Error> {
        if let PIBValue::ShortAddress(x) = self {
            Ok(x)
        } else {
            Err(())
        }
    }
}
impl TryInto<ExtendedAddress> for PIBValue {
    type Error = ();
    fn try_into(self) -> Result<ExtendedAddress, Self::Error> {
        if let PIBValue::ExtendedAddress(x) = self {
            Ok(x)
        } else {
            Err(())
        }
    }
}
impl TryInto<PANID> for PIBValue {
    type Error = ();
    fn try_into(self) -> Result<PANID, Self::Error> {
        if let PIBValue::PANID(x) = self {
            Ok(x)
        } else {
            Err(())
        }
    }
}
impl<A, B> TryInto<(A, B)> for PIBValue
where
    PIBValue: TryInto<A, Error = ()> + TryInto<B, Error = ()>,
{
    type Error = ();
    fn try_into(self) -> Result<(A, B), Self::Error> {
        if let PIBValue::Pair(a, b) = self {
            let a: PIBValue = *a;
            let b: PIBValue = *b;
            Ok((a.try_into()?, b.try_into()?))
        } else {
            Err(())
        }
    }
}

/*
impl<T> TryInto<Option<T>> for PIBValue where PIBValue: TryInto<T, Error=()> {
    type Error = ();
    fn try_into(self) -> Result<Option<T>, Self::Error> {
        if let PIBValue::Empty = self {
            Ok(None)
        } else {
            Ok(Some(self.try_into()?))
        }
    }
}
*/

pub struct PIB {
    pub mac_extended_address: ExtendedAddress,
    // Combination of mac_associated_pan_coord, macCoordExtendedAddress, macCoordShortAddress
    pub mac_associated_pan_coord: Option<(ExtendedAddress, ShortAddress)>,
    pub mac_association_permit: bool,
    pub mac_beacon_payload: Vec<u8>,
    pub mac_bsn: u8,
    pub mac_dsn: u8,
    pub mac_pan_id: PANID,
    pub mac_short_address: ShortAddress,
    pub mac_beacon_auto_respond: bool,
    pub phy_current_channel: u16,
    pub phy_max_tx_power: u16,
    pub phy_tx_power: u16,
}

impl PIBValue {
    fn set_to<T>(self, target: &mut T) -> Result<(), ()>
    where
        PIBValue: TryInto<T, Error = ()>,
    {
        *target = self.try_into()?;
        Ok(())
    }
}

impl PIB {
    pub fn new(
        extended_address: ExtendedAddress,
        phy_current_channel: u16,
        phy_max_tx_power: u16,
    ) -> PIB {
        PIB {
            mac_extended_address: extended_address,
            mac_associated_pan_coord: None,
            mac_association_permit: false,
            mac_beacon_payload: Vec::new(),
            mac_bsn: random(),
            mac_dsn: random(),
            mac_pan_id: PANID(0xFFFF),
            mac_short_address: ShortAddress(0xFFFF),
            mac_beacon_auto_respond: false,
            phy_current_channel,
            phy_max_tx_power,
            phy_tx_power: phy_max_tx_power,
        }
    }

    pub fn reset(&mut self) {
        *self = PIB::new(
            self.mac_extended_address,
            self.phy_current_channel,
            self.phy_tx_power,
        );
    }

    pub fn get(&self, param: PIBProperty) -> Result<PIBValue, ()> {
        match param {
            PIBProperty::MacExtendedAddress => Ok(self.mac_extended_address.into()),
            PIBProperty::MacAssociatedPanCoord => Ok(self.mac_associated_pan_coord.into()),
            PIBProperty::MacAssociationPermit => Ok(self.mac_association_permit.into()),
            PIBProperty::MacBeaconPayload => Ok(self.mac_beacon_payload.clone().into()),
            PIBProperty::MacBsn => Ok(self.mac_bsn.into()),
            PIBProperty::MacDsn => Ok(self.mac_dsn.into()),
            PIBProperty::MacPanId => Ok(self.mac_pan_id.into()),
            PIBProperty::MacShortAddress => Ok(self.mac_short_address.into()),
            PIBProperty::MacBeaconAutoRespond => Ok(self.mac_beacon_auto_respond.into()),
            PIBProperty::PhyCurrentChannel => Ok(self.phy_current_channel.into()),
            PIBProperty::PhyMaxTxPower => Ok(self.phy_max_tx_power.into()),
            PIBProperty::PhyTxPower => Ok(self.phy_tx_power.into()),
            _ => Err(()),
        }
    }

    pub fn set(&mut self, param: PIBProperty, value: PIBValue) -> Result<(), mlme::Error> {
        match param {
            PIBProperty::MacExtendedAddress => Err(mlme::Error::ReadOnly),
            PIBProperty::MacAssociationPermit => {
                self.mac_association_permit =
                    value.try_into().or(Err(mlme::Error::InvalidParameter))?;
                Ok(())
            }
            PIBProperty::MacBeaconPayload => {
                self.mac_beacon_payload =
                    value.try_into().or(Err(mlme::Error::InvalidParameter))?;
                Ok(())
            }
            PIBProperty::MacBsn => {
                self.mac_bsn = value.try_into().or(Err(mlme::Error::InvalidParameter))?;
                Ok(())
            }
            PIBProperty::MacDsn => {
                self.mac_dsn = value.try_into().or(Err(mlme::Error::InvalidParameter))?;
                Ok(())
            }
            PIBProperty::MacPanId => {
                self.mac_pan_id = value.try_into().or(Err(mlme::Error::InvalidParameter))?;
                Ok(())
            }
            PIBProperty::MacShortAddress => {
                self.mac_short_address = value.try_into().or(Err(mlme::Error::InvalidParameter))?;
                Ok(())
            }
            PIBProperty::MacBeaconAutoRespond => {
                self.mac_beacon_auto_respond =
                    value.try_into().or(Err(mlme::Error::InvalidParameter))?;
                Ok(())
            }
            PIBProperty::PhyCurrentChannel => {
                self.phy_current_channel =
                    value.try_into().or(Err(mlme::Error::InvalidParameter))?;
                Ok(())
            }
            PIBProperty::PhyMaxTxPower => {
                self.phy_max_tx_power = value.try_into().or(Err(mlme::Error::InvalidParameter))?;
                Ok(())
            }
            PIBProperty::PhyTxPower => {
                self.phy_tx_power = value.try_into().or(Err(mlme::Error::InvalidParameter))?;
                Ok(())
            }
            _ => Err(mlme::Error::UnsupportedAttribute),
        }
    }

    pub fn next_beacon_sequence_nr(&mut self) -> u8 {
        let ret = self.mac_bsn;
        self.mac_bsn = self.mac_bsn.wrapping_add(1);
        ret
    }
}
