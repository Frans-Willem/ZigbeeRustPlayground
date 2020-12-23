use crate::ieee802154::mac::commands;
use crate::ieee802154::mac::data;
use crate::ieee802154::mac::mlme;
use crate::ieee802154::mac::pib;
use crate::ieee802154::{ExtendedAddress, ShortAddress, PANID};
use crate::pack::Pack;
use crate::radio::{
    RadioError, RadioPacket, RadioParam, RadioParamType, RadioParamValue, RadioRequest,
    RadioResponse, RadioRxMode,
};
use crate::unique_key::UniqueKey;
use futures::channel::mpsc;
use futures::future::{Future, FutureExt};
use futures::select;
use futures::sink::{Sink, SinkExt};
use futures::stream::{Stream, StreamExt};
use std::convert::TryInto;
use std::marker::Unpin;
use std::collections::HashMap;
use std::collections::HashSet;

/**
 * Quickly gets a parameter from the radio,
 * ignoring all other responses received before the get-response.
 */
async fn radio_get_param(
    radio_requests: &mut (dyn Sink<RadioRequest, Error = mpsc::SendError> + Unpin + Send),
    radio_responses: &mut (dyn Stream<Item = RadioResponse> + Unpin + Send),
    param: RadioParam,
    param_type: RadioParamType,
) -> Result<RadioParamValue, RadioError> {
    let token = UniqueKey::new();
    radio_requests
        .send(RadioRequest::GetParam(token, param, param_type))
        .await
        .unwrap_or(());
    loop {
        if let Some(RadioResponse::GetParam(response_token, _, result)) =
            radio_responses.next().await
        {
            if token == response_token {
                return result;
            }
        }
    }
}

async fn radio_get_param_u64(
    radio_requests: &mut (dyn Sink<RadioRequest, Error = mpsc::SendError> + Unpin + Send),
    radio_responses: &mut (dyn Stream<Item = RadioResponse> + Unpin + Send),
    param: RadioParam,
) -> Result<u64, RadioError> {
    let untyped_result =
        radio_get_param(radio_requests, radio_responses, param, RadioParamType::U64).await?;
    untyped_result.try_into()
}

async fn radio_get_param_u32(
    radio_requests: &mut (dyn Sink<RadioRequest, Error = mpsc::SendError> + Unpin + Send),
    radio_responses: &mut (dyn Stream<Item = RadioResponse> + Unpin + Send),
    param: RadioParam,
) -> Result<u32, RadioError> {
    let untyped_result =
        radio_get_param(radio_requests, radio_responses, param, RadioParamType::U32).await?;
    untyped_result.try_into()
}

async fn radio_get_param_u16(
    radio_requests: &mut (dyn Sink<RadioRequest, Error = mpsc::SendError> + Unpin + Send),
    radio_responses: &mut (dyn Stream<Item = RadioResponse> + Unpin + Send),
    param: RadioParam,
) -> Result<u16, RadioError> {
    let untyped_result =
        radio_get_param(radio_requests, radio_responses, param, RadioParamType::U16).await?;
    untyped_result.try_into()
}

async fn radio_set_param<T: Into<RadioParamValue>>(
    requests: &mut (dyn Sink<RadioRequest, Error = mpsc::SendError> + Unpin + Send),
    responses: &mut (dyn Stream<Item = RadioResponse> + Unpin + Send),
    param: RadioParam,
    value: T,
) -> Result<(), RadioError> {
    let token = UniqueKey::new();
    requests
        .send(RadioRequest::SetParam(token, param, value.into()))
        .await
        .unwrap();
    loop {
        if let Some(RadioResponse::SetParam(response_token, _, result)) = responses.next().await {
            if token == response_token {
                result?;
                return Ok(());
            }
        }
    }
}

async fn radio_init_pending_data_table(
    requests: &mut (dyn Sink<RadioRequest, Error = mpsc::SendError> + Unpin + Send),
    responses: &mut (dyn Stream<Item = RadioResponse> + Unpin + Send),
) -> Result<(), RadioError> {
    let token = UniqueKey::new();
    requests
        .send(RadioRequest::InitPendingDataTable(token))
        .await
        .unwrap();
    loop {
        if let Some(RadioResponse::InitPendingDataTable(response_token, result)) =
            responses.next().await
        {
            if token == response_token {
                return result;
            }
        }
    }
}

async fn radio_set_power(
    requests: &mut (dyn Sink<RadioRequest, Error = mpsc::SendError> + Unpin + Send),
    responses: &mut (dyn Stream<Item = RadioResponse> + Unpin + Send),
    power: bool,
) -> Result<(), RadioError> {
    let token = UniqueKey::new();
    requests
        .send(RadioRequest::SetPower(token, power))
        .await
        .unwrap();
    loop {
        if let Some(RadioResponse::SetPower(response_token, _, result)) = responses.next().await {
            if token == response_token {
                return result;
            }
        }
    }
}

struct MacData {
    pib: pib::PIB,
    radio: Box<dyn Sink<RadioRequest, Error = mpsc::SendError> + Unpin + Send>,
    mlme_confirms: Box<dyn Sink<mlme::Confirm, Error = mpsc::SendError> + Unpin + Send>,
    mlme_indications: Box<dyn Sink<mlme::Indication, Error = mpsc::SendError> + Unpin + Send>,
    radio_param_cache: HashMap<RadioParam, RadioParamValue>,
    radio_param_updating: HashSet<RadioParam>,
}

#[derive(Debug)]
enum MacInput {
    Radio(RadioResponse),
    Request(mlme::Request),
}

impl MacData {
    async fn new(
        radio_requests: Box<dyn Sink<RadioRequest, Error = mpsc::SendError> + Unpin + Send>,
        radio_responses: &mut (dyn Stream<Item = RadioResponse> + Unpin + Send),
        mlme_confirms: Box<dyn Sink<mlme::Confirm, Error = mpsc::SendError> + Unpin + Send>,
        mlme_indications: Box<dyn Sink<mlme::Indication, Error = mpsc::SendError> + Unpin + Send>,
    ) -> MacData {
        let mut radio_requests = radio_requests;
        println!("Initializing MAC");
        println!("Getting properties");
        let extended_address = ExtendedAddress(
            radio_get_param_u64(
                radio_requests.as_mut(),
                radio_responses,
                RadioParam::LongAddress,
            )
            .await
            .unwrap(),
        );
        let max_tx_power = radio_get_param_u16(
            radio_requests.as_mut(),
            radio_responses,
            RadioParam::TxPowerMax,
        )
        .await
        .unwrap();
        let current_channel = radio_get_param_u16(
            radio_requests.as_mut(),
            radio_responses,
            RadioParam::Channel,
        )
        .await
        .unwrap();
        println!("Setting RX Mode");
        radio_set_param(
            radio_requests.as_mut(),
            radio_responses,
            RadioParam::RxMode,
            RadioRxMode {
                address_filter: true,
                autoack: true,
                poll_mode: false,
            },
        )
        .await
        .unwrap();
        println!("Initializing pending data table");
        radio_init_pending_data_table(radio_requests.as_mut(), radio_responses)
            .await
            .unwrap();
        println!("Turning radio on");
        radio_set_power(radio_requests.as_mut(), radio_responses, true)
            .await
            .unwrap();
        println!("Initialization of MAC complete");
        MacData {
            pib: pib::PIB::new(extended_address, current_channel, max_tx_power),
            radio: radio_requests,
            mlme_confirms,
            mlme_indications,
            radio_param_cache: HashMap::new(),
            radio_param_updating: HashSet::new(),
        }
    }

    async fn process(
        mut self,
        mut radio_responses: Box<dyn Stream<Item = RadioResponse> + Unpin + Send>,
        requests: Box<dyn Stream<Item = mlme::Request> + Unpin + Send>,
    ) {
        let mut radio_responses = radio_responses.fuse();
        let mut requests = requests.fuse();
        while let Some(input) = select! {
            x = radio_responses.next() => x.map(MacInput::Radio),
            x = requests.next() => x.map(MacInput::Request),
        } {
            match input {
                MacInput::Radio(x) => self.process_radio_response(x).await,
                MacInput::Request(x) => self.process_mlme_request(x).await,
                input => println!("Mac: Unhandled input: {:?}", input),
            }
        }
        println!("Mac: One or more inputs dried up, stopping service")
    }

    async fn update_radio_params(&mut self) {
        let mut wanted : HashMap<RadioParam, RadioParamValue> = HashMap::new();
        wanted.insert(RadioParam::Channel, self.pib.phy_current_channel.into());
        wanted.insert(RadioParam::PanId, self.pib.mac_pan_id.0.into());
        wanted.insert(RadioParam::ShortAddress, self.pib.mac_short_address.0.into());
        wanted.insert(RadioParam::RxMode, RadioRxMode {
            address_filter: true,
            autoack: true,
            poll_mode: false,
        }.into());
        wanted.insert(RadioParam::TxPower, self.pib.phy_tx_power.into());
        //wanted.insert(RadioParam::LongAddress, self.pib.mac_extended_address.0.into());

        for (attribute, value) in wanted.drain() {
            if !self.radio_param_updating.contains(&attribute) {
                if self.radio_param_cache.get(&attribute) != Some(&value) {
                    self.radio_param_updating.insert(attribute);
                    self.radio.send(RadioRequest::SetParam(UniqueKey::new(), attribute, value)).await.unwrap();
                }
            }
        }
    }

    async fn process_radio_response(&mut self, response: RadioResponse) {
        match response {
            RadioResponse::OnPacket(p) => self.process_packet(p).await,
            RadioResponse::SetParam(_, param, result) => {
                if let Ok(value) = result {
                    self.radio_param_cache.insert(param, value);
                } else {
                    self.radio_param_cache.remove(&param);
                }
                self.radio_param_updating.remove(&param);
                self.update_radio_params().await;
            },
            r => println!("Unhandled radio response: {:?}", r),
        }
    }
    async fn process_packet(&mut self, packet: RadioPacket) {
        println!("Packet! {:?}", packet);
        let (frame, rest) = data::Frame::<data::VecPayload>::unpack(&packet.data).unwrap();
        println!("Frame: {:?} + {:?}", frame, rest);
        match &frame.frame_type {
            data::FrameType::Command(commands::Command::BeaconRequest()) => {
                let request = mlme::Indication::BeaconRequest {
                    beacon_type: mlme::BeaconType::Beacon, // NOTE: Cheating, we should check the frame more carefully.
                    src_addr: frame.source.clone(),
                    dst_pan_id: frame
                        .destination
                        .map_or(PANID::broadcast(), |full_address| full_address.pan_id),
                };
                self.mlme_indications.send(request).await.unwrap();
            }
            _ => {}
        }
    }

    async fn process_mlme_request(&mut self, request: mlme::Request) {
        match request {
            mlme::Request::Beacon(request) => self.process_mlme_request_beacon(request).await,
            mlme::Request::Reset(request) => self.process_mlme_request_reset(request).await,
            mlme::Request::Get(request) => self.process_mlme_request_get(request).await,
            mlme::Request::Set(request) => self.process_mlme_request_set(request).await,
            request => println!("Unhandled MLME request: {:?}", request),
        }
    }

    async fn process_mlme_request_beacon(&mut self, request: mlme::BeaconRequest) {
        if request.superframe_order != 15
            || request.channel != self.pib.phy_current_channel
            || request.channel_page != 0
        {
            self.mlme_confirms
                .send(mlme::Confirm::Beacon(Err(mlme::Error::InvalidParameter)))
                .await
                .unwrap();
            return;
        }
        let beacon = data::Beacon {
            beacon_order: 15,
            superframe_order: request.superframe_order,
            final_cap_slot: 15,
            battery_life_extension: false,
            pan_coordinator: true, // Not sure how to set this parameter correctly :/
            association_permit: self.pib.mac_association_permit,
        };
        let frame = data::Frame::<data::VecPayload> {
            frame_pending: false,
            acknowledge_request: false,
            sequence_number: Some(self.pib.next_beacon_sequence_nr()),
            destination: None,
            source: Some(data::FullAddress {
                pan_id: self.pib.mac_pan_id,
                address: if self.pib.mac_short_address != ShortAddress(0xFFFE) {
                    data::Address::Short(self.pib.mac_short_address)
                } else {
                    data::Address::Extended(self.pib.mac_extended_address)
                },
            }),
            frame_type: data::FrameType::<data::VecPayload>::Beacon(beacon),
        };
        self.mlme_confirms
            .send(mlme::Confirm::Beacon(Ok(())))
            .await
            .unwrap();
    }

    async fn process_mlme_request_reset(&mut self, request: mlme::ResetRequest) {
        if request.set_default_pib {
            self.pib.reset()
        }
        self.update_radio_params().await;
        self.mlme_confirms
            .send(mlme::Confirm::Reset(Ok(())))
            .await
            .unwrap();
    }

    async fn process_mlme_request_start() {}

    async fn process_mlme_request_get(&mut self, request: mlme::GetRequest) {
        let result = self
            .pib
            .get(request.attribute)
            .or(Err(mlme::Error::UnsupportedAttribute));
        self.mlme_confirms
            .send(mlme::Confirm::Get(request.attribute, result))
            .await
            .unwrap();
    }
    async fn process_mlme_request_set(&mut self, request: mlme::SetRequest) {
        let result = self.pib.set(request.attribute, request.value);
        self.update_radio_params().await;
        self.mlme_confirms
            .send(mlme::Confirm::Set(request.attribute, result))
            .await
            .unwrap();
    }
}

pub async fn start(
    radio_requests: Box<dyn Sink<RadioRequest, Error = mpsc::SendError> + Unpin + Send>,
    radio_responses: Box<dyn Stream<Item = RadioResponse> + Unpin + Send>,
    mlme_requests: Box<dyn Stream<Item = mlme::Request> + Unpin + Send>,
    mlme_confirms: Box<dyn Sink<mlme::Confirm, Error = mpsc::SendError> + Unpin + Send>,
    mlme_indications: Box<dyn Sink<mlme::Indication, Error = mpsc::SendError> + Unpin + Send>,
) {
    let mut radio_responses = radio_responses;
    let data = MacData::new(
        radio_requests,
        radio_responses.as_mut(),
        mlme_confirms,
        mlme_indications,
    )
    .await;
    data.process(radio_responses, mlme_requests).await
}
