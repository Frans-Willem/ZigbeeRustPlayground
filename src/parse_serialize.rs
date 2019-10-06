use bytes::{Buf, BufMut, Bytes};

#[derive(Debug)]
pub enum Error {
    InsufficientData,
    UnexpectedData,
    Unimplemented(&'static str),
}
pub type Result<T> = std::result::Result<T, Error>;

impl From<enum_tryfrom::InvalidEnumValue> for Error {
    fn from(_: enum_tryfrom::InvalidEnumValue) -> Self {
        Error::Unimplemented("Invalid enum value")
    }
}

pub trait ParseFromBuf: Sized {
    fn parse_from_buf(buf: &mut Buf) -> Result<Self>;
}

pub trait SerializeToBuf {
    fn expected_size(&self) -> usize {
        return 0;
    }
    fn serialize_to_buf(&self, buf: &mut BufMut) -> Result<()>;
}

pub trait SerializeToBufEx {
    fn serialize_as_vec(&self) -> Result<Vec<u8>>;
}

impl<T> SerializeToBufEx for T
where
    T: SerializeToBuf,
{
    fn serialize_as_vec(&self) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        self.serialize_to_buf(&mut buf)?;
        Ok(buf)
    }
}

pub trait ParseFromBufEx: Sized {
    fn parse_from_vec(data: &Vec<u8>) -> Result<Self>;
}

impl<T> ParseFromBufEx for T
where
    T: ParseFromBuf,
{
    fn parse_from_vec(data: &Vec<u8>) -> Result<T> {
        let mut cursor = std::io::Cursor::new(data);
        T::parse_from_buf(&mut cursor)
    }
}

impl SerializeToBuf for Bytes {
    fn expected_size(&self) -> usize {
        self.len()
    }
    fn serialize_to_buf(&self, buf: &mut BufMut) -> Result<()> {
        BufMut::put_slice(buf, self);
        Ok(())
    }
}

/* Default implementations */
macro_rules! default_impl {
    ($t:ty) => {
        impl ParseFromBuf for $t {
            fn parse_from_buf(buf: &mut bytes::Buf) -> $crate::parse_serialize::Result<$t> {
                let mut data = [0; std::mem::size_of::<$t>()];
                if buf.remaining() < data.len() {
                    std::result::Result::Err($crate::parse_serialize::Error::InsufficientData)
                } else {
                    buf.copy_to_slice(&mut data);
                    std::result::Result::Ok(<$t>::from_le_bytes(data))
                }
            }
        }
        impl SerializeToBuf for $t {
            fn expected_size(&self) -> usize {
                return std::mem::size_of::<$t>();
            }
            fn serialize_to_buf(&self, buf: &mut BufMut) -> $crate::parse_serialize::Result<()> {
                buf.put_slice(&self.clone().to_le_bytes());
                std::result::Result::Ok(())
            }
        }
    };
}

default_impl!(u8);
default_impl!(u16);
default_impl!(u32);
default_impl!(u64);
default_impl!(u128);
default_impl!(i8);
default_impl!(i16);
default_impl!(i32);
default_impl!(i64);
default_impl!(i128);

#[macro_export]
macro_rules! default_parse_serialize_newtype {
    ($t:ident, $i:ident) => {
        impl $crate::parse_serialize::ParseFromBuf for $t {
            fn parse_from_buf(buf: &mut bytes::Buf) -> $crate::parse_serialize::Result<$t> {
                std::result::Result::Ok($t($i::parse_from_buf(buf)?))
            }
        }
        impl $crate::parse_serialize::SerializeToBuf for $t {
            fn expected_size(&self) -> usize {
                match self {
                    $t(inner) => inner.expected_size(),
                }
            }

            fn serialize_to_buf(
                &self,
                buf: &mut bytes::BufMut,
            ) -> $crate::parse_serialize::Result<()> {
                match self {
                    $t(inner) => inner.serialize_to_buf(buf),
                }
            }
        }
    };
}

#[macro_export]
macro_rules! default_parse_serialize_enum {
    ($t:ident, $i:ident) => {
        impl $crate::parse_serialize::ParseFromBuf for $t {
            fn parse_from_buf(buf: &mut bytes::Buf) -> $crate::parse_serialize::Result<$t> {
                $t::try_from($i::parse_from_buf(buf)?).map_err(|_| ParseError::UnexpectedData)
            }
        }
        impl $crate::parse_serialize::SerializeToBuf for $t {
            fn expected_size(&self) -> usize {
                // If you encounter errors here, be sure to derive Copy and Clone!
                (*self as $i).expected_size()
            }
            fn serialize_to_buf(
                &self,
                buf: &mut bytes::BufMut,
            ) -> $crate::parse_serialize::Result<()> {
                (*self as $i).serialize_to_buf(buf)
            }
        }
    };
}

pub trait ParseFromBufTagged<T>: Sized {
    fn parse_from_buf(tag: T, buf: &mut Buf) -> Result<Self>;
}

pub trait SerializeToBufTagged<T>: SerializeToBuf {
    fn get_serialize_tag(&self) -> Result<T>;
}
