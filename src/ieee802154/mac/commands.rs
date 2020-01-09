use crate::ieee802154::ShortAddress;
use crate::pack::{ExtEnum, Pack, PackTagged};

// IEEE Std 802.15.4 - 2015: 7.5.1
#[derive(Debug, Clone, PartialEq, Eq, PackTagged, Pack)]
#[tag_type(u8)]
pub enum Command {
    #[tag(0x01)]
    AssociationRequest(AssociationRequest),
    #[tag(0x02)]
    AssociationResponse(AssociationResponse),
    #[tag(0x04)]
    DataRequest(),
    #[tag(0x07)]
    BeaconRequest(),
}

// IEEE Std 802.15.4 - 2015: 7.5.2
#[derive(Debug, Clone, PartialEq, Eq, ExtEnum, Pack)]
#[tag_type(u8)]
pub enum DeviceType {
    RFD = 0,
    FFD = 1,
}

// IEEE Std 802.15.4 - 2015: 7.5.2
#[derive(Debug, Clone, PartialEq, Eq, ExtEnum, Pack)]
#[tag_type(u8)]
pub enum PowerSource {
    Battery = 0,
    AC = 1,
}

// IEEE Std 802.15.4 - 2015: 7.5.2
#[derive(Debug, Clone, PartialEq, Eq, Pack)]
pub struct AssociationRequest {
    device_type: DeviceType,
    power_source: PowerSource,
    receiver_on_when_idle: bool,
    security_capable: bool,
    allocate_address: bool,
    fast_association_requested: bool,
}

// IEEE Std 802.15.4 - 2015: 7.5.3
#[derive(Debug, Clone, PartialEq, Eq, Pack)]
pub struct AssociationResponse {
    short_address: ShortAddress,
    status: AssociationStatus,
}

// IEEE Std 802.15.4 - 2015: 7.5.3 - Table 7-50
#[derive(Debug, Clone, PartialEq, Eq, Pack, ExtEnum)]
#[tag_type(u8)]
pub enum AssociationStatus {
    Successful = 0,
    PANAtCapacity = 1,
    PANAccessDenied = 2,
    HoppingSequenceOffset = 3,
    FastAssociationSuccess = 0x80,
}
