use crate::ieee802154::mac::commands;
use crate::ieee802154::mac::data;
use crate::ieee802154::mac::mlme;
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

pub struct MacConfig {
    pub channel: u16,
    pub short_address: ShortAddress,
    pub pan_id: PANID,
}

struct MacData {
    config: MacConfig,
    my_address: ExtendedAddress,
    radio: Box<dyn Sink<RadioRequest, Error = mpsc::SendError> + Unpin + Send>,
    mlme_confirms: Box<dyn Sink<mlme::Confirm, Error = mpsc::SendError> + Unpin + Send>,
    mlme_indications: Box<dyn Sink<mlme::Indication, Error = mpsc::SendError> + Unpin + Send>,
}

#[derive(Debug)]
enum MacInput {
    Radio(RadioResponse),
    Request(mlme::Request),
}

impl MacData {
    async fn new(
        config: MacConfig,
        radio_requests: Box<dyn Sink<RadioRequest, Error = mpsc::SendError> + Unpin + Send>,
        radio_responses: &mut (dyn Stream<Item = RadioResponse> + Unpin + Send),
        mlme_confirms: Box<dyn Sink<mlme::Confirm, Error = mpsc::SendError> + Unpin + Send>,
        mlme_indications: Box<dyn Sink<mlme::Indication, Error = mpsc::SendError> + Unpin + Send>,
    ) -> MacData {
        let mut radio_requests = radio_requests;
        println!("Initializing MAC");
        let max_power = radio_get_param_u16(
            radio_requests.as_mut(),
            radio_responses,
            RadioParam::TxPowerMax,
        )
        .await
        .unwrap();
        println!("Setting power: {:?}", max_power);
        radio_set_param(
            radio_requests.as_mut(),
            radio_responses,
            RadioParam::TxPower,
            max_power,
        )
        .await
        .unwrap();
        println!("Setting channel: {:?}", config.channel);
        radio_set_param(
            radio_requests.as_mut(),
            radio_responses,
            RadioParam::Channel,
            config.channel,
        )
        .await
        .unwrap();
        println!("Setting short address: {:?}", config.short_address);
        radio_set_param(
            radio_requests.as_mut(),
            radio_responses,
            RadioParam::ShortAddress,
            config.short_address.0,
        )
        .await
        .unwrap();
        println!("Setting PAN ID: {:?}", config.pan_id);
        radio_set_param(
            radio_requests.as_mut(),
            radio_responses,
            RadioParam::PanId,
            config.pan_id.0,
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
        println!("Getting own address");
        let my_address = ExtendedAddress(
            radio_get_param_u64(
                radio_requests.as_mut(),
                radio_responses,
                RadioParam::LongAddress,
            )
            .await
            .unwrap(),
        );
        println!("Turning radio on");
        radio_set_power(radio_requests.as_mut(), radio_responses, true)
            .await
            .unwrap();
        println!("Initialization of MAC complete");
        MacData {
            config,
            my_address,
            radio: radio_requests,
            mlme_confirms,
            mlme_indications,
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
                input => println!("Mac: Unhandled input: {:?}", input),
            }
        }
        println!("Mac: One or more inputs dried up, stopping service")
    }

    async fn process_radio_response(&mut self, response: RadioResponse) {
        match response {
            RadioResponse::OnPacket(p) => self.process_packet(p).await,
            r => println!("Unhandled radio response: {:?}", r),
        }
    }
    async fn process_packet(&mut self, packet: RadioPacket) {
        println!("Packet! {:?}", packet);
        let (frame, rest) = data::Frame::<data::VecPayload>::unpack(&packet.data).unwrap();
        println!("Frame: {:?}", frame);
        match &frame.frame_type {
            data::FrameType::Command(commands::Command::BeaconRequest()) => {
                let request = mlme::BeaconRequestIndication {
                    beacon_type: mlme::BeaconType::Beacon, // NOTE: Cheating, we should check the frame more carefully.
                    src_addr: frame.source.clone(),
                    dst_pan_id: frame
                        .destination
                        .map_or(PANID::broadcast(), |full_address| full_address.pan_id),
                };
                self.mlme_indications
                    .send(mlme::Indication::BeaconRequest(request))
                    .await
                    .unwrap();
            }
            _ => {}
        }
    }
}

pub async fn start(
    config: MacConfig,
    radio_requests: Box<dyn Sink<RadioRequest, Error = mpsc::SendError> + Unpin + Send>,
    radio_responses: Box<dyn Stream<Item = RadioResponse> + Unpin + Send>,
    mlme_requests: Box<dyn Stream<Item = mlme::Request> + Unpin + Send>,
    mlme_confirms: Box<dyn Sink<mlme::Confirm, Error = mpsc::SendError> + Unpin + Send>,
    mlme_indications: Box<dyn Sink<mlme::Indication, Error = mpsc::SendError> + Unpin + Send>,
) {
    let mut radio_responses = radio_responses;
    let data = MacData::new(
        config,
        radio_requests,
        radio_responses.as_mut(),
        mlme_confirms,
        mlme_indications,
    )
    .await;
    data.process(radio_responses, mlme_requests).await
}
