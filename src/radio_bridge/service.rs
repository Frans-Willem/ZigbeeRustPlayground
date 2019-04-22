use crate::radio_bridge::raw_service;
pub use crate::radio_bridge::raw_service::IncomingPacket;
use bytes::{buf::FromBuf, buf::IntoBuf, Buf, BufMut, Bytes, BytesMut};
use futures::future::result;
use futures::sync::mpsc;
use futures::Future;
use tokio_core::reactor::Handle;
use tokio_serial::Serial;
use tokio_service::Service;

pub type ServiceFuture<T> = Future<Item = T, Error = Error>;
pub type BoxServiceFuture<T> = Box<ServiceFuture<T>>;

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

pub struct RadioRxMode {
    pub address_filter: bool,
    pub autoack: bool,
    pub poll_mode: bool,
}

impl FromToRadioValue for RadioRxMode {
    fn to_radio_value(&self) -> Result<u16, Error> {
        Ok((self.address_filter as u16) >> 0
            | (self.autoack as u16) >> 1
            | (self.poll_mode as u16) >> 2)
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

macro_rules! default_param_get_set{
    ($param:expr, $t:ty, $get:ident, $set:ident) => {
        pub fn $get(&self) -> BoxServiceFuture<$t> {
            self.get_value($param)
        }
        pub fn $set(&self, value: $t) -> BoxServiceFuture<()> {
            self.set_value($param, &value)
        }
    }
}

impl RadioBridgeService {
    pub fn new(
        port: Serial,
        handle: Handle,
    ) -> (RadioBridgeService, mpsc::UnboundedReceiver<IncomingPacket>) {
        let (raw_service, packet_stream) = raw_service::RadioBridgeService::new(port, handle);
        (RadioBridgeService { inner: raw_service }, packet_stream)
    }

    fn get_object(&self, radio_param: RadioParam, expected_size: usize) -> BoxServiceFuture<Bytes> {
        let mut request_data = BytesMut::new();
        request_data.put_u16_be(radio_param as u16);
        request_data.put_u16_be(expected_size as u16);
        Box::new(
            self.inner
                .call(raw_service::Request {
                    command_id: raw_service::RequestCommand::RadioGetObject,
                    data: request_data.freeze(),
                })
                .map_err(|e| Error::RawError(e))
                .and_then(move |data| {
                    if data.len() < 2 {
                        Err(Error::UnexpectedResponseSize)
                    } else {
                        let mut data = data.into_buf();
                        let retval = data.get_u16_be();
                        if retval == 0 {
                            if data.remaining() < expected_size {
                                Err(Error::UnexpectedResponseSize)
                            } else {
                                Ok(Bytes::from_buf(data))
                            }
                        } else {
                            Err(Error::ErrorCode(retval as usize))
                        }
                    }
                }),
        )
    }

    pub fn get_long_address(&self) -> BoxServiceFuture<u64> {
        Box::new(
            self.get_object(RadioParam::LongAddress, std::mem::size_of::<u64>())
                .map(|data| data.into_buf().get_u64_be()),
        )
    }

    fn get_value<T>(&self, radio_param: RadioParam) -> BoxServiceFuture<T>
    where
        T: FromToRadioValue,
        T: 'static,
    {
        let mut request_data = BytesMut::new();
        request_data.put_u16_be(radio_param as u16);
        let retval = self
            .inner
            .call(raw_service::Request {
                command_id: raw_service::RequestCommand::RadioGetValue,
                data: request_data.freeze(),
            })
            .map_err(|e| Error::RawError(e))
            .and_then(|data| {
                if data.len() < 2 {
                    Err(Error::UnexpectedResponseSize)
                } else {
                    let mut data = data.into_buf();
                    let retval = data.get_u16_be();
                    if retval == 0 {
                        if data.remaining() < 2 {
                            Err(Error::UnexpectedResponseSize)
                        } else {
                            Ok(data.get_u16_be())
                        }
                    } else {
                        Err(Error::ErrorCode(retval as usize))
                    }
                }
            });
        let retval = retval.and_then(move |x| T::from_radio_value(x));
        Box::new(retval)
    }

    fn set_value<T>(&self, radio_param: RadioParam, value: &T) -> BoxServiceFuture<()>
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
                                Err(Error::UnexpectedResponseSize)
                            } else {
                                let mut data = data.into_buf();
                                let retval = data.get_u16_be();
                                if retval == 0 {
                                    Ok(())
                                } else {
                                    Err(Error::ErrorCode(retval as usize))
                                }
                            }
                        }),
                )
            }
            Err(x) => Box::new(result(Err(x))),
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
    default_param_get_set!(RadioParam::ChannelMin, u16, get_channel_min, set_channel_min);
    default_param_get_set!(RadioParam::ChannelMax, u16, get_channel_max, set_channel_max);
    default_param_get_set!(RadioParam::TxPowerMin, u16, get_tx_power_min, set_tx_power_min);
    default_param_get_set!(RadioParam::TxPowerMax, u16, get_tx_power_max, set_tx_power_max);

    pub fn send(&self, data: Bytes) -> BoxServiceFuture<()> {
        Box::new(
            self.inner
                .call(raw_service::Request {
                    command_id: raw_service::RequestCommand::RadioSend,
                    data: data,
                })
                .map_err(|e| Error::RawError(e))
                .and_then(|data| {
                    if data.len() < 2 {
                        Err(Error::UnexpectedResponseSize)
                    } else {
                        let mut data = data.into_buf();
                        let retval = data.get_u16_be();
                        if retval == 0 {
                            Ok(())
                        } else {
                            Err(Error::ErrorCode(retval as usize))
                        }
                    }
                }),
        )
    }

    pub fn on(&self) -> BoxServiceFuture<()> {
        Box::new(
            self.inner
                .call(raw_service::Request {
                    command_id: raw_service::RequestCommand::RadioOn,
                    data: Bytes::new(),
                })
                .map_err(|e| Error::RawError(e))
                .and_then(|data| {
                    if data.len() < 2 {
                        Err(Error::UnexpectedResponseSize)
                    } else {
                        let mut data = data.into_buf();
                        let retval = data.get_u16_be();
                        if retval == 1 {
                            Ok(())
                        } else {
                            Err(Error::ErrorCode(retval as usize))
                        }
                    }
                }),
        )
    }
}
