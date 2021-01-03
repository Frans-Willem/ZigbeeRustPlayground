#[derive(Debug)]
pub enum Error {
    ChannelAccessFailure,
    FrameTooLong,
    ReadOnly,
    UnsupportedAttribute,
    InvalidIndex,
    InvalidParameter,
    InvalidHandle,
    NoShortAddress,
    TransactionExpired,
    NoAck,
}
