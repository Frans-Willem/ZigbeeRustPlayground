use crate::ieee802154::{ExtendedAddress, ShortAddress, PANID};
use crate::parse_serialize::{
    Deserialize, DeserializeResult, DeserializeTagged, Serialize, SerializeResult, SerializeTagged,
};
use bitfield::bitfield;

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
#[enum_tag_type(u8)]
pub enum Command {
    #[enum_tag(1)]
    AssociationRequest(CommandAssociationRequest),
    #[enum_tag(2)]
    AssociationResponse(CommandAssociationResponse),
    #[enum_tag(4)]
    DataRequest, /* 0x04 */
    #[enum_tag(7)]
    BeaconRequest, /* 0x07 */
}

#[derive(Debug, PartialEq, Copy, Clone, Eq, Hash, SerializeTagged, DeserializeTagged)]
#[enum_tag_type(u8)]
pub enum DeviceType {
    RFD = 0, // Reduced function device
    FFD = 1, // Full functioning device
}

#[derive(Debug, PartialEq, Copy, Clone, Eq, Hash, SerializeTagged, DeserializeTagged)]
#[enum_tag_type(u8)]
pub enum PowerSource {
    Battery = 0, // Not AC powered
    Powered = 1, // AC powered
}

bitfield! {
    #[derive(Serialize, Deserialize)]
    struct AssociationRequestCapabilityInfo(u8);
    impl Debug;
    pub alternate_pan_coordinator, set_alternate_pan_coordinator: 0, 0;
    pub device_type, set_device_type: 1, 1;
    pub power_source, set_power_source: 2, 2;
    pub receive_on_when_idle, set_receive_on_when_idle: 3, 3;
    pub association_type, set_association_type: 4, 4;
    pub reserved, set_reserved: 5, 5;
    pub security_capability, set_security_capability: 6, 6;
    pub allocate_address, set_allocate_address: 7, 7;
}

#[derive(Debug, PartialEq, Copy, Clone, Eq, Hash)]
pub struct CommandAssociationRequest {
    pub alternate_pan_coordinator: bool,
    pub device_type: DeviceType,
    pub power_source: PowerSource,
    pub receive_on_when_idle: bool,
    pub security_capability: bool,
    pub allocate_address: bool,
}

impl Deserialize for CommandAssociationRequest {
    fn deserialize(input: &[u8]) -> DeserializeResult<Self> {
        let (input, bf) = AssociationRequestCapabilityInfo::deserialize(input)?;
        let (input, device_type) = DeviceType::deserialize_data(bf.device_type(), input)?;
        let (input, power_source) = PowerSource::deserialize_data(bf.power_source(), input)?;
        Ok((
            input,
            CommandAssociationRequest {
                alternate_pan_coordinator: bf.alternate_pan_coordinator() > 0,
                device_type,
                power_source,
                receive_on_when_idle: bf.receive_on_when_idle() > 0,
                security_capability: bf.security_capability() > 0,
                allocate_address: bf.allocate_address() > 0,
            },
        ))
    }
}
impl Serialize for CommandAssociationRequest {
    fn serialize_to(&self, target: &mut Vec<u8>) -> SerializeResult<()> {
        let mut cif = AssociationRequestCapabilityInfo(0);
        cif.set_alternate_pan_coordinator(self.alternate_pan_coordinator as u8);
        cif.set_device_type(self.device_type.serialize_tag()?);
        cif.set_power_source(self.power_source.serialize_tag()?);
        cif.set_receive_on_when_idle(self.receive_on_when_idle as u8);
        cif.set_association_type(0); // TODO
        cif.set_reserved(0);
        cif.set_security_capability(self.security_capability as u8);
        cif.set_allocate_address(self.allocate_address as u8);
        cif.serialize_to(target)?;
        self.device_type.serialize_data_to(target)?;
        self.power_source.serialize_data_to(target)?;
        Ok(())
    }
}

#[derive(Debug, PartialEq, Copy, Clone, Eq, Hash, Serialize, Deserialize)]
#[enum_tag_type(u8)]
pub enum AssociationResponseStatus {
    AssociationSuccessful = 0,
    PANAtCapacity = 1,
    PANAccessDenied = 2,
    HoppingSequenceOffsetDuplication = 3,
    FastAssociationSuccessful = 0x80,
}
#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct CommandAssociationResponse {
    pub short_address: ShortAddress,
    pub status: AssociationResponseStatus,
}
