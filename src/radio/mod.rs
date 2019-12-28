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
use std::convert::TryInto;
use std::marker::Unpin;
use std::ops::{Deref, DerefMut};

#[derive(Debug)]
pub enum RadioRequest {
	SetParam16(Option<Token>, RawRadioParam, u16),
	SetParam64(Option<Token>, RawRadioParam, u64),
}

#[derive(Debug)]
pub enum RadioResponse {
	SetParam16(Option<Token>, RawRadioParam, Result<u16, RadioError>),
	SetParam64(Option<Token>, RawRadioParam, Result<u64, RadioError>),
	OnPacket(Vec<u8>),
}

#[derive(Debug)]
pub struct RadioPacket {
    data: Vec<u8>,
}

#[derive(Debug)]
pub enum RadioError {
    RawError(Vec<u8>),
    UnexpectedResponse,
}

#[derive(Debug)]
pub enum RadioIncoming {
    Response(Token, Result<RadioResponse, RadioError>),
    Packet(RadioPacket),
}

type RadioResponseParser = fn(Vec<u8>) -> Result<RadioResponse, RadioError>;

fn parse_max_power_response(data: Vec<u8>) -> Result<RadioResponse, RadioError> {
    if data.len() == 2 {
        Ok(RadioResponse::MaxTxPower(u16::from_be_bytes(
            data.deref().try_into().unwrap(),
        )))
    } else {
        println!("Unexpected: {:?}", data);
        Err(RadioError::UnexpectedResponse)
    }
}

impl RadioRequest {
    fn response_parser(&self) -> RadioResponseParser {
        match self {
            RadioRequest::GetMaxTxPower => parse_max_power_response,
        }
    }
    fn to_raw(self, request_id: u16) -> RawRadioMessage {
        match self {
            RadioRequest::GetMaxTxPower => RawRadioMessage {
                command_id: RawRadioCommand::GetValue,
                request_id,
                data: (RawRadioParam::TxPowerMax as u16)
                    .to_be_bytes()
                    .as_ref()
                    .into(),
            },
        }
    }
}

async fn radio_request_task<
    W: AsyncWrite + Unpin,
    S: Stream<Item = (Token, RadioRequest)> + Unpin,
>(
    port: W,
    mut requests: S,
    tokenmap: &Mutex<HashMap<u16, (Token, RadioResponseParser)>>,
) {
    let mut port = RawRadioSink::new(port);
    let mut next_request_id: u16 = 0;
    loop {
        println!("Checking for requests");
        if let Some((token, request)) = requests.next().await {
            let request_id = {
                let mut tokenmap = tokenmap.lock().await;
                while tokenmap.contains_key(&next_request_id) {
                    next_request_id += 1;
                }
                let request_id = next_request_id;
                next_request_id += 1;
                tokenmap
                    .deref_mut()
                    .insert(request_id, (token, request.response_parser()));
                request_id
            };
            println!("Sending request with request id {}", request_id);
            let request = request.to_raw(request_id);
            println!("Request: {:?}", request);
            if let Err(e) = port.send(request).await {
                println!("Unable to send :/");
            }
        } else {
            println!("Requests dried up, stopping radio service");
            break;
        }
    }
}

async fn radio_response_task<R: AsyncRead + Unpin, S: Sink<RadioIncoming> + Unpin>(
    port: R,
    mut responses: S,
    tokenmap: &Mutex<HashMap<u16, (Token, RadioResponseParser)>>,
) {
    let mut port = RawRadioStream::new(port);
    loop {
        println!("Reading radio message");
        let RawRadioMessage {
            command_id,
            request_id,
            data,
        } = port.next().await.unwrap();
        println!("Radio message received");
        match command_id {
            RawRadioCommand::Ok => {
                println!("Received OK");
                if let Some((token, parser)) = tokenmap.lock().await.remove(&request_id) {
                    println!("Found token");
                    responses
                        .send(RadioIncoming::Response(token, parser(data)))
                        .await
                        .unwrap_or(());
                } else {
                    println!("Unable to find token for request_id {}", request_id);
                }
            }
            RawRadioCommand::Err => {
                println!("Received Err");
                if let Some((token, _)) = tokenmap.lock().await.remove(&request_id) {
                    responses
                        .send(RadioIncoming::Response(
                            token,
                            Err(RadioError::RawError(data)),
                        ))
                        .await
                        .unwrap_or(());
                }
            }
            RawRadioCommand::OnPacket => {},
						_ => {
							println!("Unexpected packed from radio: {:?}", command_id);
						},
        }
    }
}

async fn radio_service<
    W: AsyncWrite + Unpin,
    R: AsyncRead + Unpin,
    RQ: Stream<Item = (Token, RadioRequest)> + Unpin,
    RS: Sink<RadioIncoming> + Unpin,
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
) -> (
    impl Sink<(Token, RadioRequest)>,
    impl Stream<Item = RadioIncoming>,
) {
    let (response_in, response_out) = mpsc::channel(0);
    let (request_in, request_out) = mpsc::channel(0);
    let task = radio_service(write, read, request_out, response_in);
    executor.spawn(task).unwrap();
    (request_in, response_out)
}
