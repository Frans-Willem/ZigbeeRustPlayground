use crate::ieee802154::mac::data::FullAddress;
use crate::ieee802154::mac::pib::{PIBProperty, PIBValue};

use crate::ieee802154::PANID;

#[derive(Debug)]
pub enum BeaconType {
    Beacon,
    EnhancedBeacon,
}

#[derive(Debug)]
pub enum Error {
    ChannelAccessFailure,
    FrameTooLong,
    ReadOnly,
    UnsupportedAttribute,
    InvalidIndex,
    InvalidParameter,
}

#[derive(Debug)]
pub struct ResetRequest {
    pub set_default_pib: bool,
}
#[derive(Debug)]
pub struct StartRequest {
    pub pan_id: PANID,
    pub channel_number: u32,
    pub channel_page: u32,
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
}
