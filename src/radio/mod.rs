pub mod raw;
use crate::radio::raw::{
    RawRadioCommand, RawRadioMessage, RawRadioParam, RawRadioSink, RawRadioStream,
};
use crate::unique_key::UniqueKey;
use async_std::sync::Mutex;
use futures::channel::mpsc;
use futures::future::FutureExt;
use futures::io::{AsyncRead, AsyncWrite};
use futures::sink::{Sink, SinkExt};
use futures::stream::{Stream, StreamExt};
use futures::task::{Spawn, SpawnExt};
use rand::prelude::*;
use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};
use std::marker::Unpin;

pub type RadioParam = RawRadioParam;

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum RadioParamType {
    U16,
    U32,
    U64,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum RadioParamValue {
    U16(u16),
    U32(u32),
    U64(u64),
}

impl TryFrom<(RadioParamType, &[u8])> for RadioParamValue {
    type Error = RadioError;

    fn try_from(input: (RadioParamType, &[u8])) -> Result<RadioParamValue, RadioError> {
        let (param_type, data) = input;
        match param_type {
            RadioParamType::U16 => match data.try_into() {
                Ok(data) => Ok(RadioParamValue::U16(u16::from_be_bytes(data))),
                Err(_) => Err(RadioError::UnexpectedResponseSize),
            },
            RadioParamType::U32 => match data.try_into() {
                Ok(data) => Ok(RadioParamValue::U32(u32::from_be_bytes(data))),
                Err(_) => Err(RadioError::UnexpectedResponseSize),
            },
            RadioParamType::U64 => match data.try_into() {
                Ok(data) => Ok(RadioParamValue::U64(u64::from_be_bytes(data))),
                Err(_) => Err(RadioError::UnexpectedResponseSize),
            },
        }
    }
}

impl TryInto<u16> for RadioParamValue {
    type Error = RadioError;
    fn try_into(self) -> Result<u16, Self::Error> {
        match self {
            RadioParamValue::U16(x) => Ok(x),
            _ => Err(RadioError::UnexpectedResponseSize),
        }
    }
}

impl TryInto<u32> for RadioParamValue {
    type Error = RadioError;
    fn try_into(self) -> Result<u32, Self::Error> {
        match self {
            RadioParamValue::U32(x) => Ok(x),
            _ => Err(RadioError::UnexpectedResponseSize),
        }
    }
}

impl TryInto<u64> for RadioParamValue {
    type Error = RadioError;
    fn try_into(self) -> Result<u64, Self::Error> {
        match self {
            RadioParamValue::U64(x) => Ok(x),
            _ => Err(RadioError::UnexpectedResponseSize),
        }
    }
}

impl From<u16> for RadioParamValue {
    fn from(value: u16) -> RadioParamValue {
        RadioParamValue::U16(value)
    }
}
impl From<u32> for RadioParamValue {
    fn from(value: u32) -> RadioParamValue {
        RadioParamValue::U32(value)
    }
}
impl From<u64> for RadioParamValue {
    fn from(value: u64) -> RadioParamValue {
        RadioParamValue::U64(value)
    }
}

#[derive(Debug, PartialEq)]
pub struct RadioRxMode {
    pub address_filter: bool,
    pub autoack: bool,
    pub poll_mode: bool,
}

impl From<RadioRxMode> for RadioParamValue {
    fn from(value: RadioRxMode) -> RadioParamValue {
        ((value.address_filter as u16)
            | (value.autoack as u16) << 1
            | (value.poll_mode as u16) << 2)
            .into()
    }
}

impl TryInto<RadioRxMode> for RadioParamValue {
    type Error = RadioError;
    fn try_into(self) -> Result<RadioRxMode, Self::Error> {
        let value: u16 = self.try_into()?;
        if value << 3 != 0 {
            Err(RadioError::UnexpectedResponse)
        } else {
            Ok(RadioRxMode {
                address_filter: (value & 1) != 0,
                autoack: ((value >> 1) & 1) != 0,
                poll_mode: ((value >> 2) & 1) != 0,
            })
        }
    }
}

#[derive(Debug)]
pub enum RadioRequest {
    SetParam(UniqueKey, RawRadioParam, RadioParamValue),
    GetParam(UniqueKey, RawRadioParam, RadioParamType),
    InitPendingDataTable(UniqueKey),
    SetPower(UniqueKey, bool),
    SendPacket(UniqueKey, Vec<u8>),
    SetPendingShort(UniqueKey, usize, Option<(u16, u16)>),
    SetPendingExtended(UniqueKey, usize, Option<u64>),
}

#[derive(Debug)]
pub enum RadioResponse {
    SetParam(UniqueKey, RadioParam, Result<RadioParamValue, RadioError>),
    GetParam(UniqueKey, RadioParam, Result<RadioParamValue, RadioError>),
    InitPendingDataTable(UniqueKey, Result<(), RadioError>),
    SetPower(UniqueKey, bool, Result<(), RadioError>),
    SendPacket(UniqueKey, Result<(), RadioError>),
    OnPacket(RadioPacket),
    SetPendingShort(UniqueKey, Result<(), RadioError>),
    SetPendingExtended(UniqueKey, Result<(), RadioError>),
}

#[derive(Debug)]
pub struct RadioPacket {
    pub data: Vec<u8>,
    pub rssi: u8,
    pub link_quality: u8,
}

#[derive(Debug)]
pub enum RadioError {
    RawError(Vec<u8>),
    UnexpectedRetvalError(u16, Vec<u8>),
    UnexpectedResponse,
    UnexpectedResponseSize,
}

impl From<std::array::TryFromSliceError> for RadioError {
    fn from(_: std::array::TryFromSliceError) -> RadioError {
        RadioError::UnexpectedResponseSize
    }
}

type RadioResponseParser = Box<dyn FnOnce(Result<&[u8], RadioError>) -> RadioResponse + Send>;

fn check_retval(expected_retval: u16, data: &[u8]) -> Result<&[u8], RadioError> {
    const U16_SIZE: usize = std::mem::size_of::<u16>();
    if data.len() < U16_SIZE {
        return Err(RadioError::UnexpectedResponseSize);
    }
    let retval = u16::from_be_bytes(data[0..U16_SIZE].try_into().unwrap());
    if retval != expected_retval {
        return Err(RadioError::UnexpectedRetvalError(
            retval,
            data[U16_SIZE..].to_vec(),
        ));
    }
    Ok(&data[U16_SIZE..])
}

fn unpack_result_get_param(
    param_type: RadioParamType,
    response: Result<&[u8], RadioError>,
) -> Result<RadioParamValue, RadioError> {
    let data = response?;
    let data = check_retval(0, data)?;
    RadioParamValue::try_from((param_type, data))
}

fn unpack_result_set_param(
    set_value: RadioParamValue,
    response: Result<&[u8], RadioError>,
) -> Result<RadioParamValue, RadioError> {
    let data = response?;
    let data = check_retval(0, data)?;
    if !data.is_empty() {
        Err(RadioError::UnexpectedResponse)
    } else {
        Ok(set_value)
    }
}

fn unpack_result_only_retval(
    expected_retval: u16,
    response: Result<&[u8], RadioError>,
) -> Result<(), RadioError> {
    let data = response?;
    let data = check_retval(expected_retval, data)?;
    if !data.is_empty() {
        Err(RadioError::UnexpectedResponse)
    } else {
        Ok(())
    }
}

impl RadioRequest {
    fn into_raw(self) -> (raw::RawRadioCommand, Vec<u8>, RadioResponseParser) {
        match self {
            RadioRequest::SetParam(token, param, param_value) => (
                match param_value {
                    RadioParamValue::U16(_) => RawRadioCommand::SetValue,
                    _ => RawRadioCommand::SetObject,
                },
                {
                    let mut data = (param as u16).to_be_bytes().to_vec();
                    match param_value {
                        RadioParamValue::U16(v) => data.extend_from_slice(v.to_be_bytes().as_ref()),
                        RadioParamValue::U32(v) => data.extend_from_slice(v.to_be_bytes().as_ref()),
                        RadioParamValue::U64(v) => data.extend_from_slice(v.to_be_bytes().as_ref()),
                    }
                    data
                },
                Box::new(move |response| {
                    RadioResponse::SetParam(
                        token,
                        param,
                        unpack_result_set_param(param_value, response),
                    )
                }),
            ),
            RadioRequest::GetParam(token, param, param_type) => (
                match param_type {
                    RadioParamType::U16 => RawRadioCommand::GetValue,
                    _ => RawRadioCommand::GetObject,
                },
                {
                    let mut data = (param as u16).to_be_bytes().to_vec();
                    match param_type {
                        RadioParamType::U16 => (),
                        RadioParamType::U32 => {
                            data.extend_from_slice((4_u16).to_be_bytes().as_ref())
                        }
                        RadioParamType::U64 => {
                            data.extend_from_slice((8_u16).to_be_bytes().as_ref())
                        }
                    };
                    data
                },
                Box::new(move |response| {
                    RadioResponse::GetParam(
                        token,
                        param,
                        unpack_result_get_param(param_type, response),
                    )
                }),
            ),
            RadioRequest::InitPendingDataTable(token) => (
                RawRadioCommand::InitPendingTable,
                Vec::new(),
                Box::new(move |response| {
                    RadioResponse::InitPendingDataTable(
                        token,
                        unpack_result_only_retval(0, response),
                    )
                }),
            ),
            RadioRequest::SetPower(token, power) => (
                if power {
                    RawRadioCommand::On
                } else {
                    RawRadioCommand::Off
                },
                Vec::new(),
                Box::new(move |response| {
                    RadioResponse::SetPower(token, power, unpack_result_only_retval(1, response))
                }),
            ),
            RadioRequest::SendPacket(token, packet) => (
                RawRadioCommand::Send,
                packet,
                Box::new(move |response| {
                    RadioResponse::SendPacket(token, unpack_result_only_retval(0, response))
                }),
            ),
            RadioRequest::SetPendingShort(token, index, address) => (
                RawRadioCommand::SetPending,
                {
                    let mut data = vec![(index as u8) & 0x7F];
                    if let Some((pan_id, short_address)) = address {
                        data.extend_from_slice(pan_id.to_le_bytes().as_ref());
                        data.extend_from_slice(short_address.to_le_bytes().as_ref());
                    }
                    data
                },
                Box::new(move |response| {
                    RadioResponse::SetPendingShort(token, unpack_result_only_retval(0, response))
                }),
            ),
            RadioRequest::SetPendingExtended(token, index, address) => (
                RawRadioCommand::SetPending,
                {
                    let mut data = vec![(index as u8) | 0x80];
                    if let Some(address) = address {
                        data.extend_from_slice(address.to_le_bytes().as_ref());
                    }
                    data
                },
                Box::new(move |response| {
                    RadioResponse::SetPendingExtended(token, unpack_result_only_retval(0, response))
                }),
            ),
        }
    }
}

async fn radio_request_task<W: AsyncWrite + Unpin, S: Stream<Item = RadioRequest> + Unpin>(
    port: W,
    mut requests: S,
    responsemap: &Mutex<HashMap<u16, RadioResponseParser>>,
) {
    let mut port = RawRadioSink::new(port);
    while let Some(request) = requests.next().await {
        // Generate a request ID.
        let (command_id, data, response_parser) = request.into_raw();
        let request_id = {
            let mut responsemap = responsemap.lock().await;
            let request_id = loop {
                let potential_id = rand::thread_rng().gen();
                if !responsemap.contains_key(&potential_id) {
                    break potential_id;
                }
            };
            responsemap.insert(request_id, response_parser);
            request_id
        };
        let request = raw::RawRadioMessage {
            command_id,
            request_id,
            data,
        };
        if let Err(e) = port.send(request).await {
            println!("Unable to send: {:?}", e);
        }
    }
    println!("Radio: Requests dried up, stopping service");
}

async fn radio_response_task<R: AsyncRead + Unpin, S: Sink<RadioResponse> + Unpin>(
    port: R,
    mut responses: S,
    responsemap: &Mutex<HashMap<u16, RadioResponseParser>>,
) {
    let mut port = RawRadioStream::new(port);
    loop {
        let RawRadioMessage {
            command_id,
            request_id,
            data,
        } = port.next().await.unwrap();
        match command_id {
            RawRadioCommand::Ok => {
                if let Some(parser) = responsemap.lock().await.remove(&request_id) {
                    responses.send(parser(Ok(&data))).await.unwrap_or(());
                } else {
                    println!(
                        "Unable to find response parser for request_id {}",
                        request_id
                    );
                }
            }
            RawRadioCommand::Err => {
                println!("Received Err");
                if let Some(parser) = responsemap.lock().await.remove(&request_id) {
                    responses
                        .send(parser(Err(RadioError::RawError(data))))
                        .await
                        .unwrap_or(());
                }
            }
            RawRadioCommand::OnPacket => {
                if data.len() < 2 {
                    println!("RADIO: Packet received without postfix");
                } else {
                    let mut data = data;
                    let link_quality = data.pop().unwrap();
                    let rssi = data.pop().unwrap();
                    let packet = RadioPacket {
                        data,
                        link_quality,
                        rssi,
                    };
                    responses
                        .send(RadioResponse::OnPacket(packet))
                        .await
                        .unwrap_or(());
                }
            }
            _ => {
                println!("Unexpected packed from radio: {:?}", command_id);
            }
        }
    }
}

async fn radio_service<
    W: AsyncWrite + Unpin + Send,
    R: AsyncRead + Unpin + Send,
    RQ: Stream<Item = RadioRequest> + Unpin + Send,
    RS: Sink<RadioResponse> + Unpin + Send,
>(
    write: W,
    read: R,
    requests: RQ,
    responses: RS,
) {
    let map = Mutex::new(HashMap::new());
    let a = radio_request_task(write, requests, &map);
    let b = radio_response_task(read, responses, &map);
    futures::future::select(a.boxed(), b.boxed()).await;
    //futures::future::join(a, b).await;
    println!("[RADIO] Either request or response task quit, radio aborting");
}

pub fn start_radio<
    S: Spawn,
    W: AsyncWrite + Unpin + Send + 'static,
    R: AsyncRead + Unpin + Send + 'static,
>(
    executor: S,
    read: R,
    write: W,
) -> (
    impl Sink<RadioRequest, Error = mpsc::SendError>,
    impl Stream<Item = RadioResponse>,
) {
    let (response_in, response_out) = mpsc::channel(0);
    let (request_in, request_out) = mpsc::channel(0);
    let task = radio_service(write, read, request_out, response_in);
    executor.spawn(task).unwrap();
    (request_in, response_out)
}
