use crate::ieee802154::mac::mlme;
use crate::radio::{
    RadioError, RadioParam, RadioParamType, RadioParamValue, RadioRequest, RadioResponse,
};
use crate::unique_key::UniqueKey;
use futures::channel::mpsc;
use futures::sink::{Sink, SinkExt};
use futures::stream::{Stream, StreamExt};
use futures::task::{Spawn, SpawnExt};
use std::convert::TryInto;
use std::marker::Unpin;

struct MacData {
    radio: Box<dyn Sink<RadioRequest, Error = mpsc::SendError> + Unpin + Send>,
    mlme_confirms: Box<dyn Sink<mlme::Confirm, Error = mpsc::SendError> + Unpin + Send>,
    mlme_indications: Box<dyn Sink<mlme::Indication, Error = mpsc::SendError> + Unpin + Send>,
}

#[derive(Debug)]
enum MacInput {
    Radio(RadioResponse),
    Request(mlme::Request),
}

/**
 * Quickly gets a parameter from the radio,
 * ignoring all other responses received before the get-response.
 */
async fn radio_get_param(
    radio_requests: &mut (dyn Sink<RadioRequest, Error = mpsc::SendError> + Unpin),
    radio_responses: &mut (dyn Stream<Item = RadioResponse> + Unpin),
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
    radio_requests: &mut (dyn Sink<RadioRequest, Error = mpsc::SendError> + Unpin),
    radio_responses: &mut (dyn Stream<Item = RadioResponse> + Unpin),
    param: RadioParam,
) -> Result<u64, RadioError> {
    let untyped_result =
        radio_get_param(radio_requests, radio_responses, param, RadioParamType::U64).await?;
    untyped_result.try_into()
}

impl MacData {
    async fn process(
        mut self,
        radio_responses: Box<dyn Stream<Item = RadioResponse> + Unpin + Send>,
        requests: Box<dyn Stream<Item = mlme::Request> + Unpin + Send>,
    ) {
        let mut input_stream = futures::stream::select(
            radio_responses.map(MacInput::Radio),
            requests.map(MacInput::Request),
        );
        while let Some(input) = input_stream.next().await {
            match input {
                i => println!("Mac: Unhandled input: {:?}", i),
            }
        }
        println!("Mac: Mac inputs dried up, stopping service")
    }

    async fn initialize(
        &mut self,
        radio_responses: &mut (dyn Stream<Item = RadioResponse> + Unpin + Send),
    ) {
        let address: u64 =
            radio_get_param_u64(&mut self.radio, radio_responses, RadioParam::LongAddress)
                .await
                .unwrap();
    }
}

pub fn start_mac(
    executor: &dyn Spawn,
    radio_requests: Box<dyn Sink<RadioRequest, Error = mpsc::SendError> + Unpin + Send>,
    radio_responses: Box<dyn Stream<Item = RadioResponse> + Unpin + Send>,
    mlme_requests: Box<dyn Stream<Item = mlme::Request> + Unpin + Send>,
    mlme_confirms: Box<dyn Sink<mlme::Confirm, Error = mpsc::SendError> + Unpin + Send>,
    mlme_indications: Box<dyn Sink<mlme::Indication, Error = mpsc::SendError> + Unpin + Send>,
) {
    let data = MacData {
        radio: radio_requests,
        mlme_confirms,
        mlme_indications,
    };
    let task = data.process(radio_responses, mlme_requests);
    executor.spawn(task).unwrap();
}
