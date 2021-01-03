use crate::ieee802154::frame::{AddressingMode, FullAddress};
pub use crate::ieee802154::services::error::Error;
use crate::unique_key::UniqueKey;

#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy)]
pub struct MsduHandle(UniqueKey);

impl MsduHandle {
    pub fn new() -> Self {
        Self(UniqueKey::new())
    }
}

#[derive(Debug)]
pub struct DataRequest {
    pub source_addressing_mode: AddressingMode,
    pub destination: Option<FullAddress>,
    pub msdu: Vec<u8>,
    pub msdu_handle: MsduHandle,
    pub ack_tx: bool,
    pub indirect_tx: bool,
}
#[derive(Debug)]
pub struct DataConfirm {
    pub msdu_handle: MsduHandle,
    pub ack_payload: Result<Vec<u8>, Error>,
}
#[derive(Debug)]
pub struct DataIndication {
    pub source: Option<FullAddress>,
    pub destination: Option<FullAddress>,
    pub msdu: Vec<u8>,
    pub mpdu_link_quality: u8,
    pub dsn: Option<u8>,
    pub rssi: u8,
}
#[derive(Debug)]
pub struct PurgeRequest {
    pub msdu_handle: MsduHandle,
}
#[derive(Debug)]
pub struct PurgeConfirm {
    pub msdu_handle: MsduHandle,
    pub status: Result<(), Error>,
}

#[derive(Debug)]
pub enum Request {
    Data(DataRequest),
    Purge(PurgeRequest),
}

#[derive(Debug)]
pub enum Confirm {
    Data(DataConfirm),
    Purge(PurgeConfirm),
}

#[derive(Debug)]
pub enum Indication {
    Data(DataIndication),
}

#[derive(Debug)]
pub enum Response {}

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
