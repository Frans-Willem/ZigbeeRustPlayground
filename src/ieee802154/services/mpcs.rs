use crate::ieee802154::{ExtendedAddress, ShortAddress, PANID};
use crate::ieee802154::frame::{AddressingMode, FullAddress};
use crate::unique_key::UniqueKey;

#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy)]
pub struct MsduHandle(UniqueKey);

impl MsduHandle {
    pub fn new() -> Self {
        Self(UniqueKey::new())
    }
}

pub enum MpcsError {
    // TODO: Shared between MLME and MPCS?
    // iee802154::Error instead ?
    InvalidHandle,
}

pub struct DataRequest {
    source_addressing_mode: AddressingMode,
    destination: Option<FullAddress>,
    msdu: Vec<u8>,
    msdu_handle: MsduHandle,
    ack_tx: bool,
    indirect_tx: bool,
}
pub struct DataConfirm {
    msdu_handle: MsduHandle,
    ack_payload: Result<Vec<u8>, MpcsError>,
}
pub struct DataIndication {
    source: Option<FullAddress>,
    destination: Option<FullAddress>,
    msdu: Vec<u8>,
    mpdu_link_quality: u8,
    dsn: Option<u8>,
    rssi: u8,
}
pub struct PurgeRequest {
    msdu_handle: MsduHandle,
}
pub struct PurgeConfirm {
    msdu_handle: MsduHandle,
    status: Result<(), MpcsError>,
}

pub enum Request {
    Data(DataRequest),
    Purge(PurgeRequest),
}

pub enum Confirm {
    Data(DataConfirm),
    Purge(PurgeConfirm),
}

pub enum Indication {
    Data(DataIndication),
}

pub enum Response {}

pub enum Input {
    Request(Request),
    Response(Response),
}

pub enum Output {
    Confirm(Confirm),
    Indication(Indication),
}
