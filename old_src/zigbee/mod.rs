pub mod aps;
pub mod nwk;
pub mod security;
use crate::parse_serialize::Serialize;

#[derive(Eq, PartialEq, Copy, Clone, Debug, Serialize)]
pub struct ClusterId(pub u16);

#[derive(Eq, PartialEq, Copy, Clone, Debug, Serialize)]
pub struct ProfileId(pub u16);

#[derive(Eq, PartialEq, Copy, Clone, Debug, Serialize)]
pub struct EndpointId(pub u8);

#[derive(Eq, PartialEq, Copy, Clone, Debug, Serialize)]
pub struct GroupId(pub u16);
