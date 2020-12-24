use crate::ieee802154::ShortAddress;
use crate::pack::{ExtEnum, Pack, PackError, PackTagged, PackTarget, UnpackError};

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
#[derive(Debug, Clone, PartialEq, Eq, ExtEnum, Pack, PackTagged)]
#[tag_type(u8)]
pub enum DeviceType {
    RFD = 0,
    FFD = 1,
}

// IEEE Std 802.15.4 - 2015: 7.5.2
#[derive(Debug, Clone, PartialEq, Eq, ExtEnum, Pack, PackTagged)]
#[tag_type(u8)]
pub enum PowerSource {
    Battery = 0,
    AC = 1,
}

// IEEE Std 802.15.4 - 2015: 7.5.2
#[derive(Debug, Clone, PartialEq, Eq, ExtEnum, Pack, PackTagged)]
#[tag_type(u8)]
pub enum AssociationType {
    Normal = 0,
    Fast = 1,
}

// IEEE Std 802.15.4 - 2015: 7.5.2
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssociationRequest {
    device_type: DeviceType,
    power_source: PowerSource,
    receiver_on_when_idle: bool,
    association_type: AssociationType,
    security_capable: bool,
    allocate_address: bool,
}

impl Pack for AssociationRequest {
    fn unpack(data: &[u8]) -> Result<(Self, &[u8]), UnpackError> {
        let (capability_information, data) = u8::unpack(data)?;
        let reserved = capability_information & 1;
        let (device_type, data) = DeviceType::unpack_data((capability_information >> 1) & 1, data)?;
        let (power_source, data) =
            PowerSource::unpack_data((capability_information >> 2) & 1, data)?;
        let receiver_on_when_idle = (capability_information >> 3) & 1 != 0;
        let (association_type, data) =
            AssociationType::unpack_data((capability_information >> 4) & 1, data)?;
        let reserved2 = (capability_information >> 5) & 1;
        let security_capable = (capability_information >> 6) & 1 != 0;
        let allocate_address = (capability_information >> 7) & 1 != 0;
        Ok((
            AssociationRequest {
                device_type,
                power_source,
                receiver_on_when_idle,
                association_type,
                security_capable,
                allocate_address,
            },
            data,
        ))
    }

    fn pack<T: PackTarget>(&self, target: T) -> Result<T, PackError<T::Error>> {
        Ok(target)
    }
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
