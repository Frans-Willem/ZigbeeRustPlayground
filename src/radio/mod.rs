pub mod raw;
use crate::radio::raw::{
    RawRadioCommand, RawRadioMessage, RawRadioParam, RawRadioSink, RawRadioStream,
};
use crate::tokenmap::Token;
use async_std::sync::Mutex;
use futures::channel::mpsc;
use futures::io::{AsyncRead, AsyncWrite};
use futures::sink::{Sink, SinkExt};
use futures::stream::{Stream, StreamExt};
use futures::task::{Spawn, SpawnExt};
use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};
use std::marker::Unpin;
use std::ops::Deref;

pub type RadioParam = RawRadioParam;

#[derive(Debug, Clone, Copy)]
pub enum RadioParamType {
    U16,
    U32,
    U64,
}

#[derive(Debug)]
pub enum RadioParamValue {
    U16(u16),
    U32(u32),
    U64(u64),
}

impl TryFrom<(RadioParamType, &[u8])> for RadioParamValue {
    type Error = RadioError;

    fn try_from(input: (RadioParamType, &[u8])) -> Result<RadioParamValue, RadioError> {
        let (param_type, data) = input;
        println!("RadioParamValue::try_from: {:?} {:?}", param_type, data);
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

#[derive(Debug)]
pub enum RadioRequest {
    SetParam(Option<Token>, RawRadioParam, RadioParamValue),
    GetParam(Option<Token>, RawRadioParam, RadioParamType),
}

#[derive(Debug)]
pub enum RadioResponse {
    SetParam(
        Option<Token>,
        RadioParam,
        Result<RadioParamValue, RadioError>,
    ),
    GetParam(
        Option<Token>,
        RadioParam,
        Result<RadioParamValue, RadioError>,
    ),
    OnPacket(Vec<u8>),
}

#[derive(Debug)]
pub struct RadioPacket {
    data: Vec<u8>,
}

#[derive(Debug)]
pub enum RadioError {
    RawError(Vec<u8>),
    RetvalError(u16, Vec<u8>),
    UnexpectedResponse,
    UnexpectedResponseSize,
}

type RadioResponseParser = Box<dyn FnOnce(Result<&[u8], RadioError>) -> RadioResponse + Send>;

impl RadioRequest {
    fn to_raw(self) -> (raw::RawRadioCommand, Vec<u8>, RadioResponseParser) {
        match self {
            RadioRequest::GetParam(token, param, param_type) => (
                match param_type {
                    RadioParamType::U16 => RawRadioCommand::GetValue,
                    _ => RawRadioCommand::GetObject,
                },
                {
                    let mut data = (param as u16).to_be_bytes().as_ref().to_vec();
                    match param_type {
                        RadioParamType::U16 => (),
                        RadioParamType::U32 => {
                            data.extend_from_slice((4 as u16).to_be_bytes().as_ref())
                        }
                        RadioParamType::U64 => {
                            data.extend_from_slice((8 as u16).to_be_bytes().as_ref())
                        }
                    };
                    data
                },
                Box::new(move |response| {
                    RadioResponse::GetParam(
                        token,
                        param,
                        match response {
                            Ok(data) => RadioParamValue::try_from((param_type, data)),
                            Err(err) => Err(err),
                        },
                    )
                }),
            ),
            _ => todo!(),
        }
    }
}

async fn radio_request_task<W: AsyncWrite + Unpin, S: Stream<Item = RadioRequest> + Unpin>(
    port: W,
    mut requests: S,
    responsemap: &Mutex<HashMap<u16, RadioResponseParser>>,
) {
    let mut port = RawRadioSink::new(port);
    let mut next_request_id: u16 = 4;
    while let Some(request) = requests.next().await {
        // Generate a request ID.
        let (command_id, data, response_parser) = request.to_raw();
        let request_id = {
            let mut responsemap = responsemap.lock().await;
            while responsemap.contains_key(&next_request_id) {
                next_request_id += 1;
            }
            let request_id = next_request_id;
            responsemap.insert(request_id, response_parser);
            next_request_id += 1;
            request_id
        };
        println!("Sending request with request id {}", request_id);
        let request = raw::RawRadioMessage {
            command_id,
            request_id,
            data,
        };
        println!("Request: {:?}", request);
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
        println!("Reading radio message");
        let RawRadioMessage {
            command_id,
            request_id,
            data,
        } = port.next().await.unwrap();
        println!(
            "Radio message received {:?} {:?} {:?}",
            command_id, request_id, data
        );
        match command_id {
            RawRadioCommand::Ok => {
                if let Some(parser) = responsemap.lock().await.remove(&request_id) {
                    responses
                        .send(parser(if data.len() < 2 {
                            Err(RadioError::UnexpectedResponseSize)
                        } else {
                            let retval = u16::from_be_bytes(data[0..2].try_into().unwrap());
                            if retval == 0 {
                                Ok(&data[2..])
                            } else {
                                Err(RadioError::RetvalError(retval, data[2..].to_vec()))
                            }
                        }))
                        .await
                        .unwrap_or(());
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
            RawRadioCommand::OnPacket => {}
            _ => {
                println!("Unexpected packed from radio: {:?}", command_id);
            }
        }
    }
}

async fn radio_service<
    W: AsyncWrite + Unpin,
    R: AsyncRead + Unpin,
    RQ: Stream<Item = RadioRequest> + Unpin,
    RS: Sink<RadioResponse> + Unpin,
>(
    write: W,
    read: R,
    requests: RQ,
    responses: RS,
) {
    let map = Mutex::new(HashMap::new());
    let a = radio_request_task(write, requests, &map);
    let b = radio_response_task(read, responses, &map);
    futures::future::join(a, b).await;
}

pub fn start_radio<
    S: Spawn,
    W: AsyncWrite + Unpin + Send + 'static,
    R: AsyncRead + Unpin + Send + 'static,
>(
    executor: S,
    read: R,
    write: W,
) -> (impl Sink<RadioRequest>, impl Stream<Item = RadioResponse>) {
    let (response_in, response_out) = mpsc::channel(0);
    let (request_in, request_out) = mpsc::channel(0);
    let task = radio_service(write, read, request_out, response_in);
    executor.spawn(task).unwrap();
    (request_in, response_out)
}
