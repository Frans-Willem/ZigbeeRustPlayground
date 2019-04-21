use crate::radio_bridge::raw_service;
pub use crate::radio_bridge::raw_service::IncomingPacket;
use bytes::{buf::FromBuf, buf::IntoBuf, Buf, BufMut, Bytes, BytesMut};
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

    fn get_value(&self, radio_param: RadioParam) -> BoxServiceFuture<u16> {
        let mut request_data = BytesMut::new();
        request_data.put_u16_be(radio_param as u16);
        Box::new(
            self.inner
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
                }),
        )
    }

    fn set_value(&self, radio_param: RadioParam, value: u16) -> BoxServiceFuture<()> {
        let mut request_data = BytesMut::new();
        request_data.put_u16_be(radio_param as u16);
        request_data.put_u16_be(value);
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

    pub fn get_channel_min(&self) -> BoxServiceFuture<u16> {
        self.get_value(RadioParam::ChannelMin)
    }

    pub fn get_channel_max(&self) -> BoxServiceFuture<u16> {
        self.get_value(RadioParam::ChannelMax)
    }

    pub fn get_channel(&self) -> BoxServiceFuture<u16> {
        self.get_value(RadioParam::Channel)
    }

    pub fn set_channel(&self, channel: u16) -> BoxServiceFuture<()> {
        self.set_value(RadioParam::Channel, channel)
    }

    pub fn set_rx_mode(&self, rx_mode: u16) -> BoxServiceFuture<()> {
        self.set_value(RadioParam::RxMode, rx_mode)
    }

    pub fn get_txpower_max(&self) -> BoxServiceFuture<u16> {
        self.get_value(RadioParam::TxPowerMax)
    }

    pub fn get_txpower_min(&self) -> BoxServiceFuture<u16> {
        self.get_value(RadioParam::TxPowerMin)
    }

    pub fn set_txpower(&self, tx_power: u16) -> BoxServiceFuture<()> {
        self.set_value(RadioParam::TxPower, tx_power)
    }

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
