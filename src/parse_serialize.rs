use cookie_factory::{GenResult, WriteContext};
use impl_trait_for_tuples::impl_for_tuples;
use nom::IResult;
pub use parse_serialize_derive::{
    Deserialize, DeserializeTagged, Serialize, SerializeTagged, Tagged,
};
use std::io::Write;

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
                if remaining.is_empty() {
                    Ok(result)
                } else {
                    Err(SerializeError::DataLeft)
                }
            }
            Err(nom::Err::Incomplete(_)) => Err(SerializeError::InsufficientData),
            Err(nom::Err::Error(e)) => Err(e.1),
            Err(nom::Err::Failure(e)) => Err(e.1),
        }
    }
}

pub type SerializeResult<T> = std::result::Result<T, SerializeError>;

pub trait Serialize {
    fn serialize<W: Write>(&self, ctx: WriteContext<W>) -> GenResult<W>;
}

pub trait Tagged {
    type TagType: Copy;
    fn get_tag(&self) -> SerializeResult<Self::TagType>;
}

pub trait SerializeTagged: Tagged {
    fn serialize_data<W: Write>(&self, ctx: WriteContext<W>) -> GenResult<W>;
}

pub trait DeserializeTagged: Tagged + Sized {
    fn deserialize_data(tag: Self::TagType, input: &[u8]) -> DeserializeResult<Self>;
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
            fn serialize<W: Write>(&self, ctx: WriteContext<W>) -> GenResult<W> {
                cookie_factory::combinator::slice(&self.clone().to_le_bytes())(ctx)
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

impl Serialize for bool {
    fn serialize<W: Write>(&self, ctx: WriteContext<W>) -> GenResult<W> {
        (*self as u8).serialize(ctx)
    }
}

impl Deserialize for bool {
    fn deserialize(input: &[u8]) -> DeserializeResult<bool> {
        nom::combinator::map(u8::deserialize, |v: u8| v > 0)(input)
    }
}

#[impl_for_tuples(10)]
impl Serialize for Tuple {
    fn serialize<W: Write>(&self, ctx: WriteContext<W>) -> GenResult<W> {
        for_tuples!( #( let ctx = Tuple.serialize(ctx)?; )* );
        Ok(ctx)
    }
}

#[impl_for_tuples(10)]
impl Deserialize for Tuple {
    fn deserialize(input: &[u8]) -> DeserializeResult<Self> {
        for_tuples!( #( let (input, Tuple) = Tuple::deserialize(input)?; )* );
        Ok((input, (for_tuples!( #( Tuple ),* ))))
    }
}

#[cfg(test)]
fn test_simple_serialization_roundtrip<T: Serialize + Deserialize + Eq + std::fmt::Debug>(
    input: T,
    output: Vec<u8>,
) {
    let deserialized = T::deserialize_complete(&output).unwrap();
    assert_eq!(input, deserialized);
    let mut serialized = Vec::new();
    cookie_factory::gen(move |ctx| input.serialize(ctx), &mut serialized).unwrap();
    assert_eq!(output, serialized);
}

#[test]
fn test_simple_serialize() {
    test_simple_serialization_roundtrip(
        (
            1 as u8,
            2 as u16,
            3 as u32,
            4 as u64,
            true as bool,
            false as bool,
        ),
        vec![1, 2, 0, 3, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 1, 0],
    );
}

#[test]
fn test_structure_derive() {
    #[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
    struct Test {
        x: u8,
        y: u16,
        z: u32,
    };
    test_simple_serialization_roundtrip(Test { x: 1, y: 2, z: 3 }, vec![1, 2, 0, 3, 0, 0, 0]);
}

#[test]
fn test_unnamed_structure_derive() {
    #[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
    struct Test(u8, u16, u32);
    test_simple_serialization_roundtrip(Test(1, 2, 3), vec![1, 2, 0, 3, 0, 0, 0]);
}

#[test]
fn test_simple_enum_derive() {
    #[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
    #[serialize_tag_type(u8)]
    enum Test8 {
        A = 12,
        B = 34,
        #[serialize_tag(56)]
        C,
    }
    #[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
    #[serialize_tag_type(u32)]
    enum Test32 {
        A = 12,
        B = 34,
        #[serialize_tag(56)]
        C,
    }
    test_simple_serialization_roundtrip(Test8::A, vec![12]);
    test_simple_serialization_roundtrip(Test8::B, vec![34]);
    test_simple_serialization_roundtrip(Test8::C, vec![56]);
    test_simple_serialization_roundtrip(Test32::A, vec![12, 0, 0, 0]);
    test_simple_serialization_roundtrip(Test32::B, vec![34, 0, 0, 0]);
    test_simple_serialization_roundtrip(Test32::C, vec![56, 0, 0, 0]);
}

#[test]
fn test_data_enum_derive() {
    #[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
    #[serialize_tag_type(u16)]
    enum Test {
        #[serialize_tag(12)]
        A(u8),
        #[serialize_tag(34)]
        B(u16, u32),
        #[serialize_tag(56)]
        C { a: u8 },
        #[serialize_tag(78)]
        D { a: u16, b: u32 },
    }
    test_simple_serialization_roundtrip(Test::A(10), vec![12, 0, 10]);
    test_simple_serialization_roundtrip(Test::B(10, 20), vec![34, 0, 10, 0, 20, 0, 0, 0]);
    test_simple_serialization_roundtrip(Test::C { a: 10 }, vec![56, 0, 10]);
    test_simple_serialization_roundtrip(Test::D { a: 10, b: 20 }, vec![78, 0, 10, 0, 20, 0, 0, 0]);
}

#[test]
fn test_enum_get_tag() {
    #[derive(PartialEq, Eq, Debug, Clone, Copy)]
    enum TestTag {
        A,
        B,
        C,
    }
    #[derive(PartialEq, Eq, Debug, Tagged)]
    #[serialize_tag_type(TestTag)]
    enum Test {
        #[serialize_tag(TestTag::A)]
        A(u8),
        #[serialize_tag(TestTag::B)]
        B(u16),
        #[serialize_tag(TestTag::C)]
        C(u32),
    }
    assert_eq!(Test::A(12).get_tag().unwrap(), TestTag::A);
    assert_eq!(Test::B(12).get_tag().unwrap(), TestTag::B);
    assert_eq!(Test::C(12).get_tag().unwrap(), TestTag::C);
}
#[test]
fn test_enum_serialize_tagged() {
    #[derive(PartialEq, Eq, Debug, Clone, Copy)]
    enum TestTag {
        A,
        B,
        C,
        D,
    }
    #[derive(PartialEq, Eq, Debug, Tagged, SerializeTagged)]
    #[serialize_tag_type(TestTag)]
    enum Test {
        #[serialize_tag(TestTag::A)]
        A(u8),
        #[serialize_tag(TestTag::B)]
        B(u16),
        #[serialize_tag(TestTag::C)]
        C(u32),
    }
    let mut serialized = Vec::new();
    cookie_factory::gen(move |ctx| Test::A(12).serialize_data(ctx), &mut serialized).unwrap();
    assert_eq!(serialized, vec![12]);
    let mut serialized = Vec::new();
    cookie_factory::gen(move |ctx| Test::B(12).serialize_data(ctx), &mut serialized).unwrap();
    assert_eq!(serialized, vec![12, 0]);
    let mut serialized = Vec::new();
    cookie_factory::gen(move |ctx| Test::C(12).serialize_data(ctx), &mut serialized).unwrap();
    assert_eq!(serialized, vec![12, 0, 0, 0]);
}

#[test]
fn test_enum_deserialize_tagged() {
    #[derive(PartialEq, Eq, Debug, Clone, Copy)]
    enum TestTag {
        A,
        B,
        C,
        D,
    }
    #[derive(PartialEq, Eq, Debug, Tagged, DeserializeTagged)]
    #[serialize_tag_type(TestTag)]
    enum Test {
        #[serialize_tag(TestTag::A)]
        A(u8),
        #[serialize_tag(TestTag::B)]
        B(u16),
        #[serialize_tag(TestTag::C)]
        C(u32),
    }
    assert_eq!(
        Test::A(12),
        Test::deserialize_data(TestTag::A, &[12]).unwrap().1
    );
    assert_eq!(
        Test::B(0x0201),
        Test::deserialize_data(TestTag::B, &[0x01, 0x02]).unwrap().1
    );
    assert_eq!(
        Test::C(0x04030201),
        Test::deserialize_data(TestTag::C, &[0x01, 0x02, 0x03, 0x04])
            .unwrap()
            .1
    );
    assert_eq!(
        false,
        Test::deserialize_data(TestTag::D, &[1, 2, 3, 4, 5]).is_ok()
    );
}

/*

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
*/
