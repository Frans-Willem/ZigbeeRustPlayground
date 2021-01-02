use crate::ieee802154::frame;
use crate::ieee802154::mac::queue::{MacQueue, MacQueueAction, MacQueueEntry};
use crate::ieee802154::pib;
use crate::ieee802154::services::mlme;
use crate::ieee802154::{ExtendedAddress, ShortAddress, PANID};
use crate::pack::Pack;
use crate::pack::VecPackTarget;
use crate::radio::{
    RadioError, RadioPacket, RadioParam, RadioParamType, RadioParamValue, RadioRequest,
    RadioResponse, RadioRxMode,
};
use crate::unique_key::UniqueKey;
use futures::channel::mpsc;
use futures::future::FutureExt;
use futures::select;
use futures::sink::{Sink, SinkExt};
use futures::stream::{Stream, StreamExt};

use crate::delay_queue::DelayQueue;
use bimap::BiMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::convert::TryInto;
use std::marker::Unpin;
use std::time::Duration;

// TODO:
// - Ack handling
// - Ack timeout handling
// - Data timeout handling

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
    mlme_output: Box<dyn Sink<mlme::Output, Error = mpsc::SendError> + Unpin + Send>,
    radio_param_cache: HashMap<RadioParam, RadioParamValue>,
    radio_param_updating: HashSet<RadioParam>,
    packet_queue: VecDeque<Vec<u8>>,
    packet_in_progress: Option<UniqueKey>,
    queue: MacQueue,
    ack_timeouts: DelayQueue<UniqueKey>,
    ack_map: BiMap<u8, UniqueKey>,
}

#[derive(Debug)]
enum MacInput {
    Radio(RadioResponse),
    Request(mlme::Request),
    Response(mlme::Response),
    Queue(MacQueueAction),
    AckTimeout(UniqueKey),
}

impl From<mlme::Input> for MacInput {
    fn from(value: mlme::Input) -> MacInput {
        match value {
            mlme::Input::Request(x) => MacInput::Request(x),
            mlme::Input::Response(x) => MacInput::Response(x),
        }
    }
}

impl From<RadioResponse> for MacInput {
    fn from(value: RadioResponse) -> MacInput {
        MacInput::Radio(value)
    }
}

impl From<MacQueueAction> for MacInput {
    fn from(value: MacQueueAction) -> MacInput {
        MacInput::Queue(value)
    }
}

impl MacData {
    async fn new(
        radio_requests: Box<dyn Sink<RadioRequest, Error = mpsc::SendError> + Unpin + Send>,
        radio_responses: &mut (dyn Stream<Item = RadioResponse> + Unpin + Send),
        mlme_output: Box<dyn Sink<mlme::Output, Error = mpsc::SendError> + Unpin + Send>,
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
            mlme_output,
            radio_param_cache: HashMap::new(),
            radio_param_updating: HashSet::new(),
            packet_queue: VecDeque::new(),
            packet_in_progress: None,
            queue: MacQueue::new(),
            ack_timeouts: DelayQueue::new(),
            ack_map: BiMap::new(),
        }
    }

    fn get_full_short_address(&self) -> frame::FullAddress {
        frame::FullAddress {
            pan_id: self.pib.mac_pan_id,
            address: if self.pib.mac_short_address != ShortAddress::none_assigned() {
                frame::Address::Short(self.pib.mac_short_address)
            } else {
                frame::Address::Extended(self.pib.mac_extended_address)
            },
        }
    }
    fn get_full_extended_address(&self) -> frame::FullAddress {
        frame::FullAddress {
            pan_id: self.pib.mac_pan_id,
            address: frame::Address::Extended(self.pib.mac_extended_address),
        }
    }

    async fn queue_packet(&mut self, data: Vec<u8>) {
        self.packet_queue.push_back(data);
        self.flush_packet().await;
    }

    async fn queue_frame(&mut self, frame: frame::Frame) {
        let packet: Vec<u8> = frame.pack(VecPackTarget::new()).unwrap().into();
        self.queue_packet(packet).await
    }

    async fn send_indication(&mut self, indication: mlme::Indication) {
        self.mlme_output
            .send(mlme::Output::Indication(indication))
            .await
            .unwrap();
    }

    async fn send_confirm(&mut self, confirm: mlme::Confirm) {
        self.mlme_output
            .send(mlme::Output::Confirm(confirm))
            .await
            .unwrap();
    }

    async fn flush_packet(&mut self) {
        if let (None, Some(front)) = (self.packet_in_progress, self.packet_queue.front()) {
            let token = UniqueKey::new();
            self.packet_in_progress = Some(token);
            self.radio
                .send(RadioRequest::SendPacket(token, front.clone()))
                .await
                .unwrap();
        }
    }

    pub fn next_beacon_sequence_nr(&mut self) -> u8 {
        let ret = self.pib.mac_bsn;
        self.pib.mac_bsn = self.pib.mac_bsn.wrapping_add(1);
        ret
    }
    fn next_data_sequence_nr(&mut self) -> u8 {
        let ret = self.pib.mac_dsn;
        self.pib.mac_dsn = self.pib.mac_dsn.wrapping_add(1);
        ret
    }

    async fn queue_entry(&mut self, entry: MacQueueEntry) {
        let source = match entry.source_mode {
            frame::AddressingMode::None => None,
            frame::AddressingMode::Reserved => None,
            frame::AddressingMode::Short => Some(self.get_full_short_address()),
            frame::AddressingMode::Extended => Some(self.get_full_extended_address()),
        };
        let seq_nr = self.next_data_sequence_nr();
        let frame = frame::Frame {
            frame_pending: self.queue.is_pending_indirect(&entry.destination),
            acknowledge_request: entry.acknowledge_request,
            sequence_number: Some(seq_nr),
            destination: entry.destination,
            source,
            frame_type: entry.content,
        };
        if frame.acknowledge_request {
            self.ack_map.insert(seq_nr, entry.key);
            self.ack_timeouts
                .insert(entry.key, Duration::from_millis(50));
        }
        self.queue_frame(frame).await;
    }

    async fn process(
        mut self,
        radio_responses: Box<dyn Stream<Item = RadioResponse> + Unpin + Send>,
        mlme_input: Box<dyn Stream<Item = mlme::Input> + Unpin + Send>,
    ) {
        let mut radio_responses = radio_responses.fuse();
        let mut mlme_input = mlme_input.fuse();
        while let Some(input) = select! {
            x = radio_responses.next() => x.map(Into::into),
            x = mlme_input.next() => x.map(Into::into),
            x = self.queue.next() => x.map(Into::into),
            x = self.ack_timeouts.next() => x.map(MacInput::AckTimeout),
        } {
            match input {
                MacInput::Radio(x) => self.process_radio_response(x).await,
                MacInput::Request(x) => self.process_mlme_request(x).await,
                MacInput::Response(x) => self.process_mlme_response(x).await,
                MacInput::Queue(x) => self.process_queue(x).await,
                MacInput::AckTimeout(x) => self.process_ack_timeout(x).await,
                input => println!("Mac: Unhandled input: {:?}", input),
            }
        }
        println!("Mac: One or more inputs dried up, stopping service")
    }

    async fn update_radio_params(&mut self) {
        let wanted: Vec<(RadioParam, RadioParamValue)> = vec![
            (RadioParam::Channel, self.pib.phy_current_channel.into()),
            (RadioParam::PanId, self.pib.mac_pan_id.0.into()),
            (
                RadioParam::ShortAddress,
                self.pib.mac_short_address.0.into(),
            ),
            (
                RadioParam::RxMode,
                RadioRxMode {
                    address_filter: true,
                    autoack: true,
                    poll_mode: false,
                }
                .into(),
            ),
            (RadioParam::TxPower, self.pib.phy_tx_power.into()),
        ];

        for (attribute, value) in wanted {
            if !self.radio_param_updating.contains(&attribute)
                && self.radio_param_cache.get(&attribute) != Some(&value)
            {
                self.radio_param_updating.insert(attribute);
                self.radio
                    .send(RadioRequest::SetParam(UniqueKey::new(), attribute, value))
                    .await
                    .unwrap();
            }
        }
    }

    async fn process_radio_response(&mut self, response: RadioResponse) {
        match response {
            RadioResponse::OnPacket(p) => self.process_packet(p).await,
            RadioResponse::SetParam(token, param, result) => {
                self.process_radio_response_setparam(token, param, result)
                    .await
            }
            RadioResponse::SendPacket(token, result) => {
                self.process_radio_response_sendpacket(token, result).await
            }
            RadioResponse::SetPendingShort(token, result) => {
                self.queue
                    .report_set_pending_short_result(token, result.is_ok());
            }
            RadioResponse::SetPendingExtended(token, result) => {
                self.queue
                    .report_set_pending_extended_result(token, result.is_ok());
            }
            r => println!("Unhandled radio response: {:?}", r),
        }
    }

    async fn process_radio_response_setparam(
        &mut self,
        _token: UniqueKey,
        param: RadioParam,
        result: Result<RadioParamValue, RadioError>,
    ) {
        if let Ok(value) = result {
            self.radio_param_cache.insert(param, value);
        } else {
            self.radio_param_cache.remove(&param);
        }
        self.radio_param_updating.remove(&param);
        self.update_radio_params().await;
    }

    async fn process_radio_response_sendpacket(
        &mut self,
        token: UniqueKey,
        result: Result<(), RadioError>,
    ) {
        if Some(token) == self.packet_in_progress {
            self.packet_in_progress = None;
            if let Err(err) = result {
                println!("MAC: Error while sending packet: {:?}", err);
            } else {
                self.packet_queue.pop_front();
            }
            self.flush_packet().await;
        }
    }

    async fn process_packet(&mut self, packet: RadioPacket) {
        let (frame, rest) = frame::Frame::unpack(&packet.data).unwrap();
        match &frame.frame_type {
            frame::FrameType::Command(frame::Command::BeaconRequest()) => {
                self.process_packet_beaconrequest(&frame).await
            }
            frame::FrameType::Command(frame::Command::AssociationRequest(req)) => {
                self.process_packet_associationrequest(&frame, req).await
            }
            frame::FrameType::Command(frame::Command::DataRequest()) => {
                self.process_packet_datarequest(&frame).await
            }
            frame::FrameType::Ack(payload) => self.process_packet_ack(&frame, payload).await,
            _ => println!("Unhandled: {:?} + {:?}", frame, rest),
        }
    }

    async fn process_packet_beaconrequest(&mut self, frame: &frame::Frame) {
        let beacon_type = mlme::BeaconType::Beacon; // NOTE: Cheating, we should check the frame more carefully.
        if self.pib.mac_beacon_auto_respond {
            let request = mlme::BeaconRequest {
                beacon_type,
                channel: self.pib.phy_current_channel,
                channel_page: 0,
                superframe_order: 15,
                dst_addr: frame.source,
            };
            self.process_mlme_request_beacon(request).await;
        } else {
            self.send_indication(mlme::Indication::BeaconRequest {
                beacon_type: mlme::BeaconType::Beacon,
                src_addr: frame.source,
                dst_pan_id: frame
                    .destination
                    .map_or(PANID::broadcast(), |full_address| full_address.pan_id),
            })
            .await;
        }
    }

    async fn process_packet_associationrequest(
        &mut self,
        frame: &frame::Frame,
        capability_information: &frame::CapabilityInformation,
    ) {
        if !self.pib.mac_association_permit {
            println!("Ignoring: Association not allowed");
            return;
        }
        if frame.destination != Some(self.get_full_short_address()) {
            println!("Ignoring: Association request not meant for me");
            return;
        }
        if let Some(frame::FullAddress {
            pan_id: source_pan_id,
            address: frame::Address::Extended(device_address),
        }) = frame.source
        {
            if source_pan_id != PANID::broadcast() {
                println!("Warning: Source PAN ID was not correctly set to broadcast");
            }
            let device_address = device_address;
            let capability_information = capability_information.clone();
            self.send_indication(mlme::Indication::Associate {
                device_address,
                capability_information,
            })
            .await;
        } else {
            println!("Invalid source address in AssociationRequest");
        }
    }

    async fn process_packet_datarequest(&mut self, frame: &frame::Frame) {
        println!("Data request: {:?}", frame.source);
        if let Some(to_send) = self.queue.pop_datarequest(&frame.source) {
            self.queue_entry(to_send).await;
        }
    }

    async fn process_mlme_request(&mut self, request: mlme::Request) {
        match request {
            mlme::Request::Beacon(request) => self.process_mlme_request_beacon(request).await,
            mlme::Request::Reset(request) => self.process_mlme_request_reset(request).await,
            mlme::Request::Get(request) => self.process_mlme_request_get(request).await,
            mlme::Request::Set(request) => self.process_mlme_request_set(request).await,
            mlme::Request::Start(request) => self.process_mlme_request_start(request).await,
            request => println!("Unhandled MLME request: {:?}", request),
        }
    }

    async fn process_mlme_request_beacon(&mut self, request: mlme::BeaconRequest) {
        if request.superframe_order != 15
            || request.channel != self.pib.phy_current_channel
            || request.channel_page != 0
        {
            self.send_confirm(mlme::Confirm::Beacon(Err(mlme::Error::InvalidParameter)))
                .await;
            return;
        }
        let beacon = frame::Beacon {
            beacon_order: 15,
            superframe_order: request.superframe_order,
            final_cap_slot: 15,
            battery_life_extension: false,
            pan_coordinator: self.pib.mac_associated_pan_coord
                == Some((self.pib.mac_extended_address, self.pib.mac_short_address)),
            association_permit: self.pib.mac_association_permit,
            payload: frame::Payload(self.pib.mac_beacon_payload.clone()),
        };
        let frame = frame::Frame {
            frame_pending: false,
            acknowledge_request: false,
            sequence_number: Some(self.next_beacon_sequence_nr()),
            destination: None,
            source: Some(self.get_full_short_address()),
            frame_type: frame::FrameType::Beacon(beacon),
        };
        self.queue_frame(frame).await;

        self.send_confirm(mlme::Confirm::Beacon(Ok(()))).await;
    }

    async fn process_mlme_request_reset(&mut self, request: mlme::ResetRequest) {
        if request.set_default_pib {
            self.pib.reset()
        }
        self.update_radio_params().await;
        self.send_confirm(mlme::Confirm::Reset(Ok(()))).await;
    }

    async fn process_mlme_request_start(&mut self, request: mlme::StartRequest) {
        if self.pib.mac_short_address == ShortAddress(0xFFFF) {
            self.send_confirm(mlme::Confirm::Start(Err(mlme::Error::NoShortAddress)))
                .await;
            return;
        }
        if request.channel_page != 0
            || request.start_time != 0
            || request.beacon_order != 15
            || request.superframe_order != 15
            || !request.pan_coordinator
            || request.battery_life_extension
        {
            self.send_confirm(mlme::Confirm::Start(Err(mlme::Error::InvalidParameter)))
                .await;
            return;
        }
        self.pib.phy_current_channel = request.channel_number;
        self.pib.mac_pan_id = request.pan_id;
        if request.pan_coordinator {
            self.pib.mac_associated_pan_coord =
                Some((self.pib.mac_extended_address, self.pib.mac_short_address));
        }
        self.update_radio_params().await;
        self.send_confirm(mlme::Confirm::Start(Ok(()))).await;
    }

    async fn process_mlme_request_get(&mut self, request: mlme::GetRequest) {
        let result = self
            .pib
            .get(request.attribute)
            .or(Err(mlme::Error::UnsupportedAttribute));
        self.send_confirm(mlme::Confirm::Get(request.attribute, result))
            .await;
    }
    async fn process_mlme_request_set(&mut self, request: mlme::SetRequest) {
        let result = self.pib.set(request.attribute, request.value);
        self.update_radio_params().await;
        self.send_confirm(mlme::Confirm::Set(request.attribute, result))
            .await;
    }

    async fn process_mlme_response(&mut self, response: mlme::Response) {
        match response {
            mlme::Response::Associate {
                device_address,
                fast_association,
                status,
            } => {
                self.process_mlme_response_associate(device_address, fast_association, status)
                    .await;
            }
            x => println!("Unhandled MLME response: {:?}", x),
        }
    }

    async fn process_mlme_response_associate(
        &mut self,
        device_address: ExtendedAddress,
        fast_association: bool,
        status: Result<Option<ShortAddress>, frame::AssociationError>,
    ) {
        let status = status.map(|addr| addr.unwrap_or(ShortAddress::none_assigned()));
        let command = frame::Command::AssociationResponse(frame::AssociationResponse {
            fast_association,
            status,
        });
        let entry = MacQueueEntry {
            key: UniqueKey::new(),
            destination: Some(frame::FullAddress {
                pan_id: self.pib.mac_pan_id,
                address: frame::Address::Extended(device_address),
            }),
            source_mode: frame::AddressingMode::Extended,
            acknowledge_request: true,
            indirect: !fast_association,
            content: frame::FrameType::Command(command),
        };
        println!("Inserted: {:?}", entry);
        self.queue.insert(entry);
    }

    async fn process_queue(&mut self, action: MacQueueAction) {
        match action {
            MacQueueAction::SetPendingShort(token, index, value) => {
                let request =
                    RadioRequest::SetPendingShort(token, index, value.map(|(x, y)| (x.0, y.0)));
                self.radio.send(request).await.unwrap();
            }
            MacQueueAction::SetPendingExtended(token, index, value) => {
                let request = RadioRequest::SetPendingExtended(token, index, value.map(|x| x.0));
                self.radio.send(request).await.unwrap();
            }
            x => println!("Unhandled Mac queue action: {:?}", x),
        }
    }

    async fn process_ack_timeout(&mut self, key: UniqueKey) {
        self.ack_map.remove_by_right(&key);
        self.queue.acknowledge_timeout(key);
    }

    async fn process_packet_ack(&mut self, frame: &frame::Frame, payload: &frame::Payload) {
        if let Some(seq_nr) = frame.sequence_number {
            if let Some((seq_nr, key)) = self.ack_map.remove_by_left(&seq_nr) {
                self.ack_timeouts.cancel(&key);
                self.queue.acknowledge(key);
            }
        }
    }
}

pub async fn start(
    radio_requests: Box<dyn Sink<RadioRequest, Error = mpsc::SendError> + Unpin + Send>,
    radio_responses: Box<dyn Stream<Item = RadioResponse> + Unpin + Send>,
    mlme_input: Box<dyn Stream<Item = mlme::Input> + Unpin + Send>,
    mlme_output: Box<dyn Sink<mlme::Output, Error = mpsc::SendError> + Unpin + Send>,
) {
    let mut radio_responses = radio_responses;
    let data = MacData::new(radio_requests, radio_responses.as_mut(), mlme_output).await;
    data.process(radio_responses, mlme_input).await
}
