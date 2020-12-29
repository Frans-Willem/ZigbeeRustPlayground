use crate::ieee802154::{ExtendedAddress, ShortAddress};
use crate::pack::{ExtEnum, Pack, PackError, PackTagged, PackTarget, UnpackError};

// IEEE Std 802.15.4 - 2015: 7.5.1
#[derive(Debug, Clone, PartialEq, Eq, PackTagged, Pack)]
#[tag_type(u8)]
pub enum Command {
    #[tag(0x01)]
    AssociationRequest(CapabilityInformation),
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityInformation {
    pub device_type: DeviceType,
    pub power_source: PowerSource,
    pub receiver_on_when_idle: bool,
    pub fast_association: bool,
    pub security_capable: bool,
    pub allocate_address: bool,
}

impl Pack for CapabilityInformation {
    fn unpack(data: &[u8]) -> Result<(Self, &[u8]), UnpackError> {
        let (capability_information, data) = u8::unpack(data)?;
        let _reserved = capability_information & 1;
        let (device_type, data) = DeviceType::unpack_data((capability_information >> 1) & 1, data)?;
        let (power_source, data) =
            PowerSource::unpack_data((capability_information >> 2) & 1, data)?;
        let receiver_on_when_idle = (capability_information >> 3) & 1 != 0;
        let fast_association = (capability_information >> 4) & 1 != 0;
        let _reserved2 = (capability_information >> 5) & 1;
        let security_capable = (capability_information >> 6) & 1 != 0;
        let allocate_address = (capability_information >> 7) & 1 != 0;
        Ok((
            CapabilityInformation {
                device_type,
                power_source,
                receiver_on_when_idle,
                fast_association,
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssociationResponse {
    pub fast_association: bool,
    pub status: Result<ShortAddress, AssociationError>,
}

impl Pack for AssociationResponse {
    fn unpack(data: &[u8]) -> Result<(Self, &[u8]), UnpackError> {
        let (short_address, data) = ShortAddress::unpack(data)?;
        let (status, data) = u8::unpack(data)?;
        let fast_association = (status & 0x80) != 0;
        let status = status & 0x7F;
        let (status, data) = match status {
            0 => (Ok(short_address), data),
            x => {
                // TODO: Check for 0xFFFF ?
                let (error, data) = AssociationError::unpack_data(x, data)?;
                (Err(error), data)
            }
        };
        Ok((
            AssociationResponse {
                fast_association,
                status,
            },
            data,
        ))
    }

    fn pack<T: PackTarget>(&self, target: T) -> Result<T, PackError<T::Error>> {
        let invalid_addr = ShortAddress::invalid();
        let (address, mut status) = match &self.status {
            Ok(addr) => (addr, 0),
            Err(x) => (&invalid_addr, x.get_tag()),
        };
        if self.fast_association {
            status |= 0x80;
        }
        let target = address.pack(target)?;
        status.pack(target)
    }
}

// IEEE Std 802.15.4 - 2015: 7.5.3 - Table 7-50
#[derive(Debug, Clone, PartialEq, Eq, PackTagged, ExtEnum)]
#[tag_type(u8)]
pub enum AssociationError {
    //Successful = 0,
    PANAtCapacity = 1,
    PANAccessDenied = 2,
    HoppingSequenceOffset = 3,
}
