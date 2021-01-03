use crate::pack::{ExtEnumError, PackTarget};
use impl_trait_for_tuples::impl_for_tuples;

#[derive(Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum UnpackError {
    NotEnoughData,  // Only valid for unpacking, not enough data available.
    InvalidEnumTag, // Invalid enum tag
    Unsupported(Option<&'static str>), // Unpacking of this structure is not supported (e.g. reserved data is not 0)
    Unimplemented(Option<&'static str>), // Unpacking of this structure was not yet properly implemented
}

#[derive(Debug, PartialEq, Eq)]
pub enum PackError<T> {
    NotAllowed(Option<&'static str>), // In case the packing is not allowed. e.g. attempting to pack a list of more than 255 items where the packing has a length prefix of 8 bits.
    TargetError(T),                   // Wraps around PackTarget::Error
}

/*
impl Into<UnpackError> for ExtEnumError {
    fn into(self) -> UnpackError {
        match self {
            ExtEnumError::InvalidTag => UnpackError::InvalidEnumTag,
        }
    }
}
*/
impl From<ExtEnumError> for UnpackError {
    fn from(e: ExtEnumError) -> Self {
        match e {
            ExtEnumError::InvalidTag => UnpackError::InvalidEnumTag,
        }
    }
}

pub trait Pack: Sized {
    fn unpack(data: &[u8]) -> Result<(Self, &[u8]), UnpackError>;
    fn pack<T: PackTarget>(&self, target: T) -> Result<T, PackError<T::Error>>;
}

pub trait PackTagged: Sized {
    type Tag;
    fn get_tag(&self) -> Self::Tag;
    fn unpack_data(tag: Self::Tag, data: &[u8]) -> Result<(Self, &[u8]), UnpackError>;
    fn pack_data<T: PackTarget>(&self, target: T) -> Result<T, PackError<T::Error>>;
}

/* Default implementations */
macro_rules! default_impl {
    ($t:ty) => {
        impl $crate::pack::Pack for $t {
            fn unpack(data: &[u8]) -> Result<(Self, &[u8]), $crate::pack::UnpackError> {
                let expected_size = core::mem::size_of::<$t>();
                if data.len() < expected_size {
                    Err(UnpackError::NotEnoughData)
                } else {
                    Ok((
                        <$t>::from_le_bytes(
                            core::convert::TryInto::try_into(&data[0..expected_size]).unwrap(),
                        ),
                        &data[expected_size..],
                    ))
                }
            }
            fn pack<T: $crate::pack::PackTarget>(
                &self,
                target: T,
            ) -> Result<T, $crate::pack::PackError<T::Error>> {
                target
                    .append(&(self.to_le_bytes()))
                    .map_err(PackError::TargetError)
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

impl Pack for bool {
    fn unpack(data: &[u8]) -> Result<(Self, &[u8]), UnpackError> {
        let (x, data) = u8::unpack(data)?;
        Ok((x > 0, data))
    }
    fn pack<T: PackTarget>(&self, target: T) -> Result<T, PackError<T::Error>> {
        (*self as u8).pack(target)
    }
}

#[impl_for_tuples(10)]
impl Pack for Tuple {
    fn unpack(data: &[u8]) -> Result<(Self, &[u8]), UnpackError> {
        for_tuples!( #( let (Tuple, data) = Tuple::unpack(data)?; )* );
        Ok(((for_tuples!( #( Tuple ),* )), data))
    }
    fn pack<T: PackTarget>(&self, target: T) -> Result<T, PackError<T::Error>> {
        for_tuples!( #( let target = Tuple.pack(target)?; )* );
        Ok(target)
    }
}

impl<T> PackTagged for Option<T>
where
    T: Pack,
{
    type Tag = bool;
    fn get_tag(&self) -> bool {
        self.is_some()
    }
    fn unpack_data(tag: bool, data: &[u8]) -> Result<(Self, &[u8]), UnpackError> {
        if tag {
            let (inner, data) = T::unpack(data)?;
            Ok((Some(inner), data))
        } else {
            Ok((None, data))
        }
    }
    fn pack_data<TA: PackTarget>(&self, target: TA) -> Result<TA, PackError<TA::Error>> {
        match self {
            Some(inner) => inner.pack(target),
            None => Ok(target),
        }
    }
}
