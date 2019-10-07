use crate::radio_bridge::raw_service;
pub use crate::radio_bridge::raw_service::IncomingPacket;
use crate::radio_bridge::serial_protocol;
use bytes::{buf::FromBuf, buf::IntoBuf, Buf, BufMut, Bytes, BytesMut};
use futures::channel::mpsc;
use futures::future;
use futures::task::Spawn;
use futures::{Future, FutureExt, Sink, Stream, TryFutureExt};
use std::io;

pub struct RadioBridgeService {
    inner: raw_service::RadioBridgeService,
}

enum RadioParam {
    PowerMode = 0,
    Channel,
    PanId,
    ShortAddress,
    RxMode,
    TxMode,
    TxPower,
    CcaThreshold,
    Rssi,
    LastRssi,
    LastLinkQuality,
    LongAddress,
    LastPacketTimestamp,
    ChannelMin,
    ChannelMax,
    TxPowerMin,
    TxPowerMax,
}

#[derive(Debug)]
pub enum Error {
    RawError(raw_service::Error),
    UnexpectedResponseSize,
    ErrorCode(usize),
    Unsupported,
}

impl From<raw_service::Error> for Error {
    fn from(err: raw_service::Error) -> Self {
        Error::RawError(err)
    }
}

trait FromToRadioValue: Sized {
    fn to_radio_value(&self) -> Result<u16, Error>;
    fn from_radio_value(value: u16) -> Result<Self, Error>;
}

impl FromToRadioValue for u16 {
    fn to_radio_value(&self) -> Result<u16, Error> {
        Ok(*self)
    }
    fn from_radio_value(value: u16) -> Result<Self, Error> {
        Ok(value)
    }
}

impl FromToRadioValue for bool {
    fn to_radio_value(&self) -> Result<u16, Error> {
        match self {
            true => Ok(1),
            false => Ok(0),
        }
    }

    fn from_radio_value(value: u16) -> Result<Self, Error> {
        match value {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Error::Unsupported),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct RadioRxMode {
    pub address_filter: bool,
    pub autoack: bool,
    pub poll_mode: bool,
}

impl FromToRadioValue for RadioRxMode {
    fn to_radio_value(&self) -> Result<u16, Error> {
        let retval = Ok((self.address_filter as u16) << 0
            | (self.autoack as u16) << 1
            | (self.poll_mode as u16) << 2);
        retval
    }

    fn from_radio_value(param: u16) -> Result<RadioRxMode, Error> {
        if param >> 3 != 0 {
            Err(Error::Unsupported)
        } else {
            Ok(RadioRxMode {
                address_filter: ((param >> 0) & 0x1) != 0,
                autoack: ((param >> 1) & 0x1) != 0,
                poll_mode: ((param >> 2) & 0x1) != 0,
            })
        }
    }
}

#[test]
fn test_radio_rx_mode_tofrom() {
    let input: u16 = 0;
    let parsed = RadioRxMode::from_radio_value(input).unwrap();
    assert_eq!(
        RadioRxMode {
            address_filter: false,
            autoack: false,
            poll_mode: false
        },
        parsed
    );
    assert_eq!(parsed.to_radio_value().unwrap(), input);

    let input: u16 = 1;
    let parsed = RadioRxMode::from_radio_value(input).unwrap();
    assert_eq!(
        RadioRxMode {
            address_filter: true,
            autoack: false,
            poll_mode: false
        },
        parsed
    );
    assert_eq!(parsed.to_radio_value().unwrap(), input);

    let input: u16 = 2;
    let parsed = RadioRxMode::from_radio_value(input).unwrap();
    assert_eq!(
        RadioRxMode {
            address_filter: false,
            autoack: true,
            poll_mode: false
        },
        parsed
    );
    assert_eq!(parsed.to_radio_value().unwrap(), input);

    let input: u16 = 4;
    let parsed = RadioRxMode::from_radio_value(input).unwrap();
    assert_eq!(
        RadioRxMode {
            address_filter: false,
            autoack: false,
            poll_mode: true
        },
        parsed
    );
    assert_eq!(parsed.to_radio_value().unwrap(), input);

    let input: u16 = 7;
    let parsed = RadioRxMode::from_radio_value(input).unwrap();
    assert_eq!(
        RadioRxMode {
            address_filter: true,
            autoack: true,
            poll_mode: true
        },
        parsed
    );
    assert_eq!(parsed.to_radio_value().unwrap(), input);
}

macro_rules! default_param_get_set{
    ($param:expr, $t:ty, $get:ident, $set:ident) => {
        pub fn $get(&self) -> impl Future<Output=Result<$t, Error>>{
            self.get_value($param)
        }
        pub fn $set(&self, value: $t) -> impl Future<Output=Result<(),Error>> {
            self.set_value($param, &value)
        }
    }
}

impl RadioBridgeService {
    pub fn new(
        serial_output: Box<Sink<serial_protocol::Command, Error = io::Error> + Unpin + Send>,
        serial_input: Box<
            Stream<Item = Result<serial_protocol::Command, io::Error>> + Unpin + Send,
        >,
        handle: &mut Spawn,
    ) -> (RadioBridgeService, mpsc::UnboundedReceiver<IncomingPacket>) {
        let (raw_service, packet_stream) =
            raw_service::RadioBridgeService::new(serial_output, serial_input, handle);
        (RadioBridgeService { inner: raw_service }, packet_stream)
    }

    fn get_object(
        &self,
        radio_param: RadioParam,
        expected_size: usize,
    ) -> impl Future<Output = Result<Bytes, Error>> {
        let mut request_data = BytesMut::new();
        request_data.put_u16_be(radio_param as u16);
        request_data.put_u16_be(expected_size as u16);
        self.inner
            .call(raw_service::Request {
                command_id: raw_service::RequestCommand::RadioGetObject,
                data: request_data.freeze(),
            })
            .map(move |data| {
                let mut data = data?.into_buf();
                if data.remaining() < 2 {
                    Err(Error::UnexpectedResponseSize)
                } else {
                    let retval = data.get_u16_be();
                    if retval != 0 {
                        Err(Error::ErrorCode(retval as usize))
                    } else if data.remaining() < expected_size {
                        Err(Error::UnexpectedResponseSize)
                    } else {
                        Ok(Bytes::from_buf(data))
                    }
                }
            })
    }

    pub fn get_long_address(&self) -> impl Future<Output = Result<u64, Error>> {
        self.get_object(RadioParam::LongAddress, std::mem::size_of::<u64>())
            .map_ok(|data| data.into_buf().get_u64_be())
    }

    fn get_value<T>(&self, radio_param: RadioParam) -> impl Future<Output = Result<T, Error>>
    where
        T: FromToRadioValue + Send + 'static,
    {
        let mut request_data = BytesMut::new();
        request_data.put_u16_be(radio_param as u16);
        self.inner
            .call(raw_service::Request {
                command_id: raw_service::RequestCommand::RadioGetValue,
                data: request_data.freeze(),
            })
            .map(move |data| {
                let mut data = data?.into_buf();
                if data.remaining() < 2 {
                    Err(Error::UnexpectedResponseSize)
                } else {
                    let retval = data.get_u16_be();
                    if retval != 0 {
                        Err(Error::ErrorCode(retval as usize))
                    } else if data.remaining() < 2 {
                        Err(Error::UnexpectedResponseSize)
                    } else {
                        T::from_radio_value(data.get_u16_be())
                    }
                }
            })
    }

    fn set_value<T>(
        &self,
        radio_param: RadioParam,
        value: &T,
    ) -> impl Future<Output = Result<(), Error>>
    where
        T: FromToRadioValue,
        T: 'static,
    {
        match value.to_radio_value() {
            Ok(request_value) => {
                let mut request_data = BytesMut::new();
                request_data.put_u16_be(radio_param as u16);
                request_data.put_u16_be(request_value);
                Box::new(
                    self.inner
                        .call(raw_service::Request {
                            command_id: raw_service::RequestCommand::RadioSetValue,
                            data: request_data.freeze(),
                        })
                        .map_err(|e| Error::RawError(e))
                        .and_then(|data| {
                            if data.len() < 2 {
                                future::err(Error::UnexpectedResponseSize)
                            } else {
                                let mut data = data.into_buf();
                                let retval = data.get_u16_be();
                                if retval == 0 {
                                    future::ok(())
                                } else {
                                    future::err(Error::ErrorCode(retval as usize))
                                }
                            }
                        }),
                )
            }
            Err(x) => {
                Box::new(future::err(x)) as Box<Future<Output = Result<(), Error>> + Send + Unpin>
            }
        }
    }

    default_param_get_set!(RadioParam::PowerMode, bool, get_power_mode, set_power_mode);
    default_param_get_set!(RadioParam::Channel, u16, get_channel, set_channel);
    default_param_get_set!(RadioParam::PanId, u16, get_pan_id, set_pan_id);
    default_param_get_set!(
        RadioParam::ShortAddress,
        u16,
        get_short_address,
        set_short_address
    );
    default_param_get_set!(RadioParam::RxMode, RadioRxMode, get_rx_mode, set_rx_mode);

    default_param_get_set!(RadioParam::TxPower, u16, get_tx_power, set_tx_power);
    default_param_get_set!(
        RadioParam::ChannelMin,
        u16,
        get_channel_min,
        set_channel_min
    );
    default_param_get_set!(
        RadioParam::ChannelMax,
        u16,
        get_channel_max,
        set_channel_max
    );
    default_param_get_set!(
        RadioParam::TxPowerMin,
        u16,
        get_tx_power_min,
        set_tx_power_min
    );
    default_param_get_set!(
        RadioParam::TxPowerMax,
        u16,
        get_tx_power_max,
        set_tx_power_max
    );

    pub fn send(&self, data: Bytes) -> impl Future<Output = Result<(), Error>> {
        self.inner
            .call(raw_service::Request {
                command_id: raw_service::RequestCommand::RadioSend,
                data: data,
            })
            .map_err(|e| Error::RawError(e))
            .and_then(|data| {
                if data.len() < 2 {
                    future::err(Error::UnexpectedResponseSize)
                } else {
                    let mut data = data.into_buf();
                    let retval = data.get_u16_be();
                    if retval == 0 {
                        future::ok(())
                    } else {
                        future::err(Error::ErrorCode(retval as usize))
                    }
                }
            })
    }

    pub fn on(&self) -> impl Future<Output = Result<(), Error>> {
        Box::new(
            self.inner
                .call(raw_service::Request {
                    command_id: raw_service::RequestCommand::RadioOn,
                    data: Bytes::new(),
                })
                .map_err(|e| Error::RawError(e))
                .and_then(|data| {
                    if data.len() < 2 {
                        future::err(Error::UnexpectedResponseSize)
                    } else {
                        let mut data = data.into_buf();
                        let retval = data.get_u16_be();
                        if retval == 1 {
                            future::ok(())
                        } else {
                            future::err(Error::ErrorCode(retval as usize))
                        }
                    }
                }),
        )
    }

    pub fn init_pending_data_table(&self) -> impl Future<Output = Result<(), Error>> {
        Box::new(
            self.inner
                .call(raw_service::Request {
                    command_id: raw_service::RequestCommand::RadioInitPendingTable,
                    data: Bytes::new(),
                })
                .map_err(|e| Error::RawError(e))
                .and_then(|data| {
                    if data.len() < 2 {
                        future::err(Error::UnexpectedResponseSize)
                    } else {
                        let mut data = data.into_buf();
                        let retval = data.get_u16_be();
                        if retval == 0 {
                            future::ok(())
                        } else {
                            future::err(Error::ErrorCode(retval as usize))
                        }
                    }
                }),
        )
    }

    pub fn set_pending_data_ext(
        &self,
        index: usize,
        address: Option<u64>,
    ) -> impl Future<Output = Result<(), Error>> {
        let mut request_data = BytesMut::new();
        request_data.put_u8(0x80 | (index as u8));
        if let Some(address) = address {
            request_data.put_u64_le(address);
        }
        self.inner
            .call(raw_service::Request {
                command_id: raw_service::RequestCommand::RadioSetPending,
                data: request_data.freeze(),
            })
            .map_err(|e| Error::RawError(e))
            .and_then(|data| {
                if data.len() < 2 {
                    future::err(Error::UnexpectedResponseSize)
                } else {
                    let mut data = data.into_buf();
                    let retval = data.get_u16_be();
                    if retval == 0 {
                        future::ok(())
                    } else {
                        future::err(Error::ErrorCode(retval as usize))
                    }
                }
            })
    }

    pub fn set_pending_data_short(
        &self,
        index: usize,
        address: Option<(u16, u16)>,
    ) -> impl Future<Output = Result<(), Error>> {
        let mut request_data = BytesMut::new();
        request_data.put_u8(0x7F & (index as u8));
        if let Some((panid, address)) = address {
            request_data.put_u16_le(panid);
            request_data.put_u16_le(address);
        }
        self.inner
            .call(raw_service::Request {
                command_id: raw_service::RequestCommand::RadioSetPending,
                data: request_data.freeze(),
            })
            .map_err(|e| Error::RawError(e))
            .and_then(|data| {
                if data.len() < 2 {
                    future::err(Error::UnexpectedResponseSize)
                } else {
                    let mut data = data.into_buf();
                    let retval = data.get_u16_be();
                    if retval == 0 {
                        future::ok(())
                    } else {
                        future::err(Error::ErrorCode(retval as usize))
                    }
                }
            })
    }
}
