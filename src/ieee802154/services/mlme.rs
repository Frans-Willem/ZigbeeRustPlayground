use crate::ieee802154::frame::FullAddress;
use crate::ieee802154::frame::{AssociationError, CapabilityInformation};
use crate::ieee802154::pib::{PIBProperty, PIBValue};
pub use crate::ieee802154::services::error::Error;
use crate::ieee802154::{ExtendedAddress, ShortAddress, PANID};

#[derive(Debug)]
#[allow(dead_code)]
pub enum BeaconType {
    Beacon,
    EnhancedBeacon,
}

#[derive(Debug)]
pub struct ResetRequest {
    pub set_default_pib: bool,
}
#[derive(Debug)]
pub struct StartRequest {
    pub pan_id: PANID,
    pub channel_number: u16,
    pub channel_page: u16,
    pub start_time: u32,
    pub beacon_order: u8,
    pub superframe_order: u8,
    pub pan_coordinator: bool,
    pub battery_life_extension: bool,
    // Not supported currently:
    // - CoordRealign*
    // - BeaconSecurity*
    // - BeaconKey *
    // - HeaderIe* PayloadIe*
}
#[derive(Debug)]
pub struct BeaconRequest {
    pub beacon_type: BeaconType,
    pub channel: u16,
    pub channel_page: u16,
    pub superframe_order: usize,
    // header_ie_list
    // payload_ie_list
    // header_ie_id_list
    // payload_ie_id_list
    // beacon_security_level
    // beacon_key_id_mode
    // beacon_key_source
    // beacon_key_index
    pub dst_addr: Option<FullAddress>,
    // bsn_suppression
}
#[derive(Debug)]
pub struct GetRequest {
    pub attribute: PIBProperty,
}
#[derive(Debug)]
pub struct SetRequest {
    pub attribute: PIBProperty,
    pub value: PIBValue,
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum Request {
    Reset(ResetRequest),
    Start(StartRequest),
    Beacon(BeaconRequest),
    Get(GetRequest),
    Set(SetRequest),
}

#[derive(Debug)]
pub enum Confirm {
    Reset(Result<(), Error>),
    Start(Result<(), Error>),
    Beacon(Result<(), Error>),
    Get(PIBProperty, Result<PIBValue, Error>),
    Set(PIBProperty, Result<(), Error>),
}

#[derive(Debug)]
pub enum Indication {
    BeaconRequest {
        beacon_type: BeaconType,
        src_addr: Option<FullAddress>,
        dst_pan_id: PANID,
    },
    Associate {
        device_address: ExtendedAddress,
        capability_information: CapabilityInformation, // 7.5.2
    },
}

#[derive(Debug)]
pub enum Response {
    Associate {
        device_address: ExtendedAddress,
        fast_association: bool,
        status: Result<Option<ShortAddress>, AssociationError>,
    },
}

#[derive(Debug)]
pub enum Input {
    Request(Request),
    Response(Response),
}

#[derive(Debug)]
pub enum Output {
    Confirm(Confirm),
    Indication(Indication),
}
