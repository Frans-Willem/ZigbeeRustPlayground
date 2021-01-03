use crate::ieee802154::frame;

use crate::ieee802154::pib;
use crate::ieee802154::services::mlme;
use crate::ieee802154::{ExtendedAddress};
use crate::pack::Pack;
use crate::pack::VecPackTarget;
use crate::radio::{
    RadioError, RadioPacket, RadioParam, RadioParamType, RadioParamValue, RadioRequest,
    RadioResponse, RadioRxMode,
};
use crate::unique_key::UniqueKey;
use futures::channel::mpsc;
use futures::future::{Future, FutureExt};

use futures::sink::{Sink, SinkExt};
use futures::stream::{BoxStream, StreamExt};






use std::convert::TryInto;



use crate::ieee802154::mac::data::{DataService, DataServiceAction};
use crate::ieee802154::mac::management::{ManagementService, ManagementServiceAction};

use std::pin::Pin;
use std::task::{Context, Poll};

type BoxSink<'a, Item, Error> = Pin<Box<dyn Sink<Item, Error = Error> + 'a + Send>>;

struct SyncRadio {
    requests: BoxSink<'static, RadioRequest, mpsc::SendError>,
    responses: BoxStream<'static, RadioResponse>,
}

impl SyncRadio {
    fn new(
        requests: BoxSink<'static, RadioRequest, mpsc::SendError>,
        responses: BoxStream<'static, RadioResponse>,
    ) -> Self {
        Self {
            requests,
            responses,
        }
    }

    fn destroy(
        self,
    ) -> (
        BoxSink<'static, RadioRequest, mpsc::SendError>,
        BoxStream<'static, RadioResponse>,
    ) {
        (self.requests, self.responses)
    }

    async fn get_param(
        &mut self,
        param: RadioParam,
        param_type: RadioParamType,
    ) -> Result<RadioParamValue, RadioError> {
        let token = UniqueKey::new();
        self.requests
            .send(RadioRequest::GetParam(token, param, param_type))
            .await
            .unwrap_or(());
        loop {
            if let Some(RadioResponse::GetParam(response_token, _, result)) =
                self.responses.next().await
            {
                if token == response_token {
                    return result;
                }
            }
        }
    }

    async fn get_param_u64(&mut self, param: RadioParam) -> Result<u64, RadioError> {
        let untyped_result = self.get_param(param, RadioParamType::U64).await?;
        untyped_result.try_into()
    }
    async fn get_param_u32(&mut self, param: RadioParam) -> Result<u32, RadioError> {
        let untyped_result = self.get_param(param, RadioParamType::U32).await?;
        untyped_result.try_into()
    }
    async fn get_param_u16(&mut self, param: RadioParam) -> Result<u16, RadioError> {
        let untyped_result = self.get_param(param, RadioParamType::U16).await?;
        untyped_result.try_into()
    }
    async fn set_param<T: Into<RadioParamValue>>(
        &mut self,
        param: RadioParam,
        value: T,
    ) -> Result<(), RadioError> {
        let token = UniqueKey::new();
        self.requests
            .send(RadioRequest::SetParam(token, param, value.into()))
            .await
            .unwrap();
        loop {
            if let Some(RadioResponse::SetParam(response_token, _, result)) =
                self.responses.next().await
            {
                if token == response_token {
                    result?;
                    return Ok(());
                }
            }
        }
    }
    async fn set_power(&mut self, power: bool) -> Result<(), RadioError> {
        let token = UniqueKey::new();
        self.requests
            .send(RadioRequest::SetPower(token, power))
            .await
            .unwrap();
        loop {
            if let Some(RadioResponse::SetPower(response_token, _, result)) =
                self.responses.next().await
            {
                if token == response_token {
                    return result;
                }
            }
        }
    }
}

struct MacData {
    pib: pib::PIB,
    radio_requests: BoxSink<'static, RadioRequest, mpsc::SendError>,
    radio_responses: BoxStream<'static, RadioResponse>,
    mlme_output: BoxSink<'static, mlme::Output, mpsc::SendError>,
    mlme_input: BoxStream<'static, mlme::Input>,
    management: ManagementService,
    data: DataService,
}

struct MacDataPoller<'a>(&'a mut MacData);

impl<'a> Future for MacDataPoller<'a> {
    type Output = MacInput;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::into_inner(self).0.poll_next_input(cx)
    }
}

#[derive(Debug)]
enum MacInput {
    Radio(RadioResponse),
    MlmeRequest(mlme::Request),
    MlmeResponse(mlme::Response),
    Management(ManagementServiceAction),
    Data(DataServiceAction),
}

impl MacData {
    async fn new(
        radio_requests: BoxSink<'static, RadioRequest, mpsc::SendError>,
        radio_responses: BoxStream<'static, RadioResponse>,
        mlme_input: BoxStream<'static, mlme::Input>,
        mlme_output: BoxSink<'static, mlme::Output, mpsc::SendError>,
    ) -> MacData {
        println!("Initializing MAC");
        println!("Getting properties");
        let mut radio = SyncRadio::new(radio_requests, radio_responses);
        let extended_address =
            ExtendedAddress(radio.get_param_u64(RadioParam::LongAddress).await.unwrap());
        let max_tx_power = radio.get_param_u16(RadioParam::TxPowerMax).await.unwrap();
        let current_channel = radio.get_param_u16(RadioParam::Channel).await.unwrap();
        println!("Setting RX Mode");
        radio
            .set_param(
                RadioParam::RxMode,
                RadioRxMode {
                    address_filter: true,
                    autoack: true,
                    poll_mode: false,
                },
            )
            .await
            .unwrap();
        println!("Turning radio on");
        radio.set_power(true).await.unwrap();
        println!("Initialization of MAC complete");
        let pib = pib::PIB::new(extended_address, current_channel, max_tx_power);
        let management = ManagementService::new(&pib);
        let data = DataService::new();
        let (radio_requests, radio_responses) = radio.destroy();
        MacData {
            pib,
            radio_requests,
            radio_responses,
            mlme_output,
            mlme_input,
            management,
            data,
        }
    }

    fn poll_next_input(&mut self, cx: &mut Context<'_>) -> Poll<MacInput> {
        if let Poll::Ready(x) = self.management.poll_action(cx) {
            Poll::Ready(MacInput::Management(x))
        } else if let Poll::Ready(x) = self.data.poll_action(&mut self.pib, cx) {
            Poll::Ready(MacInput::Data(x))
        } else if let Poll::Ready(x) = self.radio_responses.poll_next_unpin(cx) {
            Poll::Ready(MacInput::Radio(x.unwrap()))
        } else if let Poll::Ready(x) = self.mlme_input.poll_next_unpin(cx) {
            Poll::Ready(match x.unwrap() {
                mlme::Input::Request(x) => MacInput::MlmeRequest(x),
                mlme::Input::Response(x) => MacInput::MlmeResponse(x),
            })
        } else {
            Poll::Pending
        }
    }

    async fn process(mut self) {
        loop {
            match MacDataPoller(&mut self).await {
                MacInput::Management(x) => self.process_management(x).await,
                MacInput::Data(x) => self.process_data(x).await,
                MacInput::MlmeRequest(x) => self.process_mlme_request(x).await,
                MacInput::MlmeResponse(x) => self.process_mlme_response(x).await,
                MacInput::Radio(x) => self.process_radio_response(x).await,
                input => println!("MAC: Unhandled input: {:?}", input),
            }
        }
    }

    async fn send_frame(&mut self, key: UniqueKey, frame: frame::Frame) {
        let data = frame.pack(VecPackTarget::new()).unwrap().into();
        self.radio_requests
            .send(RadioRequest::SendPacket(key, data))
            .await
            .unwrap();
    }

    async fn process_management(&mut self, action: ManagementServiceAction) {
        println!("MGMT: {:?}", action);
        match action {
            ManagementServiceAction::SetParam(k, p, v) => self
                .radio_requests
                .send(RadioRequest::SetParam(k, p, v))
                .await
                .unwrap(),
            ManagementServiceAction::SendFrame(f) => self.send_frame(UniqueKey::new(), f).await,
            action => println!("Unhandled management action: {:?}", action),
        }
    }

    async fn process_data(&mut self, action: DataServiceAction) {
        println!("DATA: {:?}", action);
        match action {
            DataServiceAction::InitPendingTable(key) => self
                .radio_requests
                .send(RadioRequest::InitPendingDataTable(key))
                .await
                .unwrap(),
            DataServiceAction::SetPendingShort(key, index, value) => self
                .radio_requests
                .send(RadioRequest::SetPendingShort(
                    key,
                    index,
                    value.map(|(pan_id, address)| (pan_id.0, address.0)),
                ))
                .await
                .unwrap(),
            DataServiceAction::SetPendingExtended(key, index, value) => self
                .radio_requests
                .send(RadioRequest::SetPendingExtended(
                    key,
                    index,
                    value.map(|x| x.0),
                ))
                .await
                .unwrap(),
            DataServiceAction::SendFrame(key, frame) => self.send_frame(key, frame).await,
            action => println!("Unhandled data action: {:?}", action),
        }
    }

    async fn process_mlme_request(&mut self, request: mlme::Request) {
        if let Some(confirm) =
            self.management
                .process_mlme_request(&mut self.pib, &mut self.data, request)
        {
            self.mlme_output
                .send(mlme::Output::Confirm(confirm))
                .await
                .unwrap();
        }
    }

    async fn process_mlme_response(&mut self, response: mlme::Response) {
        self.management
            .process_mlme_response(&mut self.data, &self.pib, response);
    }

    async fn process_radio_response(&mut self, response: RadioResponse) {
        match response {
            RadioResponse::InitPendingDataTable(x, r) => {
                self.data.process_init_pending_table_result(x, r.is_ok());
            }
            RadioResponse::SetPendingShort(k, r) => {
                self.data.process_set_pending_result(k, r.is_ok());
            }
            RadioResponse::SetPendingExtended(k, r) => {
                self.data.process_set_pending_result(k, r.is_ok());
            }
            RadioResponse::SetParam(k, _, r) => {
                self.management.process_set_param_result(k, r.is_ok());
            }
            RadioResponse::OnPacket(packet) => self.process_radio_packet(packet).await,
            RadioResponse::SendPacket(k, r) => self.process_radio_send_result(k, r),
            r => println!("Unhandled radio response: {:?}", r),
        }
    }

    async fn process_radio_packet(&mut self, packet: RadioPacket) {
        let (frame, _rest) = frame::Frame::unpack(&packet.data).unwrap();
        println!("FRAME: {:?}", frame);
        if let Some(indication) = self.management.process_frame(&mut self.pib, &frame) {
            println!("INDICATION: {:?}", indication);
            self.mlme_output
                .send(mlme::Output::Indication(indication))
                .await
                .unwrap();
        }
        self.data.process_frame(&self.pib, &frame);
    }

    fn process_radio_send_result(&mut self, key: UniqueKey, result: Result<(), RadioError>) {
        self.data.process_send_result(key, result.is_ok())
    }
}

pub async fn start(
    radio_requests: BoxSink<'static, RadioRequest, mpsc::SendError>,
    radio_responses: BoxStream<'static, RadioResponse>,
    mlme_input: BoxStream<'static, mlme::Input>,
    mlme_output: BoxSink<'static, mlme::Output, mpsc::SendError>,
) {
    let radio_responses = radio_responses;
    let data = MacData::new(radio_requests, radio_responses, mlme_input, mlme_output).await;
    data.process().await
}
