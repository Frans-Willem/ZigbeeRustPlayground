pub mod aps;
pub mod nwk;
pub mod security;

pub struct ClusterId(pub u16);
default_serialization_newtype!(ClusterId, u16);

pub struct ProfileId(pub u16);
default_serialization_newtype!(ProfileId, u16);

pub struct EndpointId(pub u8);
default_serialization_newtype!(EndpointId, u8);

pub struct GroupId(pub u16);
default_serialization_newtype!(GroupId, u16);
