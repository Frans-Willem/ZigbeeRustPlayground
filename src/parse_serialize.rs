pub use parse_serialize_derive::{Deserialize, Serialize};
use generic_array::{ArrayLength, GenericArray};
use nom::IResult;
use std::iter::FromIterator;

#[derive(Debug)]
pub enum SerializeError {
    InsufficientData,
    UnexpectedData,
    Unimplemented(&'static str),
    DataLeft,
    NomError(nom::error::ErrorKind),
}

impl From<enum_tryfrom::InvalidEnumValue> for SerializeError {
    fn from(_: enum_tryfrom::InvalidEnumValue) -> Self {
        SerializeError::Unimplemented("Invalid enum value")
    }
}

#[derive(Debug)]
pub struct DeserializeError<I>(pub I, pub SerializeError);
pub type DeserializeResult<'lt, T> = IResult<&'lt [u8], T, DeserializeError<&'lt [u8]>>;

impl<I> nom::error::ParseError<I> for DeserializeError<I> {
    fn from_error_kind(input: I, kind: nom::error::ErrorKind) -> Self {
        DeserializeError(input, SerializeError::NomError(kind))
    }

    fn append(input: I, kind: nom::error::ErrorKind, _other: Self) -> Self {
        Self::from_error_kind(input, kind)
    }
}

impl<I> DeserializeError<I> {
    pub fn new(input: I, error: SerializeError) -> Self {
        DeserializeError(input, error)
    }

    pub fn insufficient_data(input: I) -> Self {
        Self::new(input, SerializeError::InsufficientData)
    }

    pub fn unexpected_data(input: I) -> Self {
        Self::new(input, SerializeError::UnexpectedData)
    }

    pub fn unimplemented(input: I, text: &'static str) -> Self {
        Self::new(input, SerializeError::Unimplemented(text))
    }

    pub fn data_left(input: I) -> Self {
        Self::new(input, SerializeError::DataLeft)
    }
}

impl<'lt, T> Into<DeserializeResult<'lt, T>> for DeserializeError<&'lt [u8]> {
    fn into(self) -> DeserializeResult<'lt, T> {
        Err(nom::Err::Error(self))
    }
}

pub trait Deserialize: Sized {
    fn deserialize(input: &[u8]) -> DeserializeResult<Self>;
    fn deserialize_complete(input: &[u8]) -> SerializeResult<Self> {
        match Self::deserialize(input) {
            Ok((remaining, result)) => {
                if remaining.len() != 0 {
                    Err(SerializeError::DataLeft)
                } else {
                    Ok(result)
                }
            }
            Err(nom::Err::Incomplete(_)) => Err(SerializeError::InsufficientData),
            Err(nom::Err::Error(e)) => Err(e.1),
            Err(nom::Err::Failure(e)) => Err(e.1),
        }
    }
}

pub trait DeserializeTagged: SerializeTagged + Sized {
    fn deserialize(tag: Self::TagType, input: &[u8]) -> DeserializeResult<Self>;
}

pub type SerializeResult<T> = std::result::Result<T, SerializeError>;

pub trait Serialize: Sized {
    fn serialize_to(&self, target: &mut Vec<u8>) -> SerializeResult<()>;
    fn serialize(&self) -> SerializeResult<Vec<u8>> {
        let mut result = vec![];
        self.serialize_to(&mut result)?;
        Ok(result)
    }
}

pub trait SerializeTagged {
    type TagType: Copy;
    fn serialize_tag(&self) -> SerializeResult<Self::TagType>;
}

/* Default implementations */
macro_rules! default_impl {
    ($t:ty) => {
        impl $crate::parse_serialize::Deserialize for $t {
            fn deserialize(input: &[u8]) -> $crate::parse_serialize::DeserializeResult<$t> {
                let mut data = [0; std::mem::size_of::<$t>()];
                let (input, parsed) = nom::bytes::streaming::take(data.len())(input)?;
                data.copy_from_slice(parsed);
                std::result::Result::Ok((input, <$t>::from_le_bytes(data)))
            }
        }
        impl $crate::parse_serialize::Serialize for $t {
            fn serialize_to(
                &self,
                target: &mut Vec<u8>,
            ) -> $crate::parse_serialize::SerializeResult<()> {
                target.extend(&self.clone().to_le_bytes());
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
macro_rules! default_serialization_enum {
    ($t:ident, $i:ident) => {
        impl $crate::parse_serialize::Deserialize for $t {
            fn deserialize(input: &[u8]) -> $crate::parse_serialize::DeserializeResult<$t> {
                let (input, parsed) = $i::deserialize(input)?;
                let result = $t::try_from(parsed).map_err(|_| {
                    $crate::nom::Err::Error($crate::parse_serialize::DeserializeError(
                        input,
                        SerializeError::UnexpectedData,
                    ))
                })?;
                std::result::Result::Ok((input, result))
            }
        }
        impl $crate::parse_serialize::Serialize for $t {
            fn serialize_to(
                &self,
                target: &mut Vec<u8>,
            ) -> $crate::parse_serialize::SerializeResult<()> {
                (*self as $i).serialize_to(target)
            }
        }
    };
}

impl Serialize for bool {
    fn serialize_to(&self, target: &mut Vec<u8>) -> SerializeResult<()> {
        (*self as u8).serialize_to(target)
    }
}

impl Deserialize for bool {
    fn deserialize(input: &[u8]) -> DeserializeResult<bool> {
        nom::combinator::map(u8::deserialize, |v: u8| v >= 0)(input)
    }
}

impl<T: Serialize> Serialize for &T {
    fn serialize_to(&self, target: &mut Vec<u8>) -> SerializeResult<()> {
        (*self).serialize_to(target)
    }
}

// TODO: Macro-ify deze
impl<T1: Deserialize, T2: Deserialize> Deserialize for (T1, T2) {
    fn deserialize(input: &[u8]) -> DeserializeResult<(T1, T2)> {
        nom::sequence::tuple((T1::deserialize, T2::deserialize))(input)
    }
}
impl<T1: Deserialize, T2: Deserialize, T3: Deserialize> Deserialize for (T1, T2, T3) {
    fn deserialize(input: &[u8]) -> DeserializeResult<(T1, T2, T3)> {
        nom::sequence::tuple((T1::deserialize, T2::deserialize, T3::deserialize))(input)
    }
}
impl<T1: Deserialize, T2: Deserialize, T3: Deserialize, T4: Deserialize> Deserialize
    for (T1, T2, T3, T4)
{
    fn deserialize(input: &[u8]) -> DeserializeResult<(T1, T2, T3, T4)> {
        nom::sequence::tuple((
            T1::deserialize,
            T2::deserialize,
            T3::deserialize,
            T4::deserialize,
        ))(input)
    }
}

impl<T1: Serialize, T2: Serialize> Serialize for (T1, T2) {
    fn serialize_to(&self, target: &mut Vec<u8>) -> SerializeResult<()> {
        self.0.serialize_to(target)?;
        self.1.serialize_to(target)?;
        Ok(())
    }
}
impl<T1: Serialize, T2: Serialize, T3: Serialize> Serialize for (T1, T2, T3) {
    fn serialize_to(&self, target: &mut Vec<u8>) -> SerializeResult<()> {
        self.0.serialize_to(target)?;
        self.1.serialize_to(target)?;
        self.2.serialize_to(target)?;
        Ok(())
    }
}

impl<T1: Serialize, T2: Serialize, T3: Serialize, T4: Serialize> Serialize for (T1, T2, T3, T4) {
    fn serialize_to(&self, target: &mut Vec<u8>) -> SerializeResult<()> {
        self.0.serialize_to(target)?;
        self.1.serialize_to(target)?;
        self.2.serialize_to(target)?;
        self.3.serialize_to(target)?;
        Ok(())
    }
}

impl<T1: Serialize, T2: Serialize, T3: Serialize, T4: Serialize, T5: Serialize> Serialize
    for (T1, T2, T3, T4, T5)
{
    fn serialize_to(&self, target: &mut Vec<u8>) -> SerializeResult<()> {
        self.0.serialize_to(target)?;
        self.1.serialize_to(target)?;
        self.2.serialize_to(target)?;
        self.3.serialize_to(target)?;
        self.4.serialize_to(target)?;
        Ok(())
    }
}

impl<T: Serialize, N: ArrayLength<T>> Serialize for GenericArray<T, N> {
    fn serialize_to(&self, target: &mut Vec<u8>) -> SerializeResult<()> {
        for i in self {
            i.serialize_to(target)?;
        }
        Ok(())
    }
}

impl<T: Deserialize, N: ArrayLength<T>> Deserialize for GenericArray<T, N> {
    fn deserialize(input: &[u8]) -> DeserializeResult<Self> {
        nom::combinator::map(
            nom::multi::count(T::deserialize, N::to_usize()),
            |vec: Vec<T>| GenericArray::from_iter(vec.into_iter()),
        )(input)
    }
}
