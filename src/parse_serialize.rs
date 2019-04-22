use bytes::{Buf, BufMut, Bytes};

#[derive(Debug)]
pub enum Error {
    InsufficientData,
    UnexpectedData,
    Unimplemented(&'static str),
}
pub type Result<T> = std::result::Result<T, Error>;

pub trait ParseFromBuf: Sized {
    fn parse_from_buf(buf: &mut Buf) -> Result<Self>;
}

pub trait SerializeToBuf {
    fn expected_size(&self) -> usize {
        return 0;
    }
    fn serialize_to_buf(&self, buf: &mut BufMut) -> Result<()>;
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
    ($t:ty, $get:ident, $put:ident) => {
        impl ParseFromBuf for $t {
            fn parse_from_buf(buf: &mut bytes::Buf) -> $crate::parse_serialize::Result<$t> {
                if buf.remaining() < std::mem::size_of::<$t>() {
                    std::result::Result::Err($crate::parse_serialize::Error::InsufficientData)
                } else {
                    std::result::Result::Ok(buf.$get())
                }
            }
        }
        impl SerializeToBuf for $t {
            fn expected_size(&self) -> usize {
                return std::mem::size_of::<$t>();
            }
            fn serialize_to_buf(&self, buf: &mut BufMut) -> $crate::parse_serialize::Result<()> {
                buf.$put(self.clone());
                std::result::Result::Ok(())
            }
        }
    };
}

default_impl!(u8, get_u8, put_u8);
default_impl!(u16, get_u16_le, put_u16_le);
default_impl!(u32, get_u32_le, put_u32_le);
default_impl!(u64, get_u64_le, put_u64_le);
default_impl!(i8, get_i8, put_i8);
default_impl!(i16, get_i16_le, put_i16_le);
default_impl!(i32, get_i32_le, put_i32_le);
default_impl!(i64, get_i64_le, put_i64_le);

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
