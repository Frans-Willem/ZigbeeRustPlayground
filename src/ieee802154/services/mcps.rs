use crate::ieee802154::frame::{AddressingMode, FullAddress};
use crate::unique_key::UniqueKey;

#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy)]
pub struct MsduHandle(UniqueKey);

impl MsduHandle {
    pub fn new() -> Self {
        Self(UniqueKey::new())
    }
}

#[derive(Debug)]
pub enum McpsError {
    // TODO: Shared between MLME and MPCS?
    // iee802154::Error instead ?
    InvalidHandle,
}

#[derive(Debug)]
pub struct DataRequest {
    source_addressing_mode: AddressingMode,
    destination: Option<FullAddress>,
    msdu: Vec<u8>,
    msdu_handle: MsduHandle,
    ack_tx: bool,
    indirect_tx: bool,
}
#[derive(Debug)]
pub struct DataConfirm {
    msdu_handle: MsduHandle,
    ack_payload: Result<Vec<u8>, McpsError>,
}
#[derive(Debug)]
pub struct DataIndication {
    source: Option<FullAddress>,
    destination: Option<FullAddress>,
    msdu: Vec<u8>,
    mpdu_link_quality: u8,
    dsn: Option<u8>,
    rssi: u8,
}
#[derive(Debug)]
pub struct PurgeRequest {
    msdu_handle: MsduHandle,
}
#[derive(Debug)]
pub struct PurgeConfirm {
    msdu_handle: MsduHandle,
    status: Result<(), McpsError>,
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
