use crate::ieee802154::ExtendedAddress;
use crate::ieee802154::ShortAddress;
use crate::ieee802154::PANID;

#[derive(Debug)]
pub enum BeaconType {
    Beacon,
    EnhancedBeacon,
}

#[derive(Debug)]
pub enum AnyAddr {
    None,
    Short(ShortAddress),
    Extended(ExtendedAddress),
}

#[derive(Debug)]
pub enum Status {
    Sucess,
    ChannelAccessFailure,
    FrameTooLong,
    InvalidParameter,
}

#[derive(Debug)]
pub struct BeaconRequestIndication {
    beacon_type: BeaconType,
    src_addr: AnyAddr,
    dst_pan_id: PANID,
    // Not supported yet: header_ie_list, payload_ie_list
}

#[derive(Debug)]
pub struct BeaconRequest {
    beacon_type: BeaconType,
    channel: i32,
    channel_page: i32,
    superframe_order: i8,
    // header_ie_list
    // payload_ie_list
    // header_ie_id_list
    // payload_ie_id_list
    // beacon_security_level
    // beacon_key_id_mode
    // beacon_key_source
    // beacon_key_index
    dst_addr: AnyAddr,
    // bsn_suppression
}

#[derive(Debug)]
pub struct BeaconConfirm(pub Status);

#[derive(Debug)]
pub enum Request {
    Beacon(BeaconRequest),
}

#[derive(Debug)]
pub enum Confirm {
    Beacon(BeaconConfirm),
}

#[derive(Debug)]
pub enum Indication {
    BeaconRequesT(BeaconRequest),
}
