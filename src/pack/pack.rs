use crate::pack::{ExtEnumError, PackTarget};
use impl_trait_for_tuples::impl_for_tuples;

#[derive(Debug, PartialEq, Eq)]
pub enum PackError<T> {
    NotEnoughData,  // Only valid for unpacking, not enough data available.
    InvalidEnumTag, // Invalid enum tag
    TargetError(T), // Wraps around PackTarget::Error
}

impl<T> Into<PackError<T>> for ExtEnumError {
    fn into(self) -> PackError<T> {
        match self {
            ExtEnumError::InvalidTag => PackError::InvalidEnumTag,
        }
    }
}

pub trait Pack<ErrType>: Sized {
    fn unpack(data: &[u8]) -> Result<(Self, &[u8]), ErrType>;
    fn pack<T: PackTarget>(&self, target: T) -> Result<T, ErrType>
    where
        T::Error: Into<ErrType>;
}

pub trait PackTagged<ErrType>: Sized {
    type Tag;
    fn get_tag(&self) -> Self::Tag;
    fn unpack_data(tag: Self::Tag, data: &[u8]) -> Result<(Self, &[u8]), ErrType>;
    fn pack_data<T: PackTarget>(&self, target: T) -> Result<T, ErrType>
    where
        T::Error: Into<ErrType>;
}

/* Default implementations */
macro_rules! default_impl {
    ($t:ty) => {
        impl<E> $crate::pack::Pack<$crate::pack::PackError<E>> for $t {
            fn unpack(data: &[u8]) -> Result<(Self, &[u8]), $crate::pack::PackError<E>> {
                let expected_size = core::mem::size_of::<$t>();
                if data.len() < expected_size {
                    Err(PackError::NotEnoughData)
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
            ) -> Result<T, $crate::pack::PackError<E>>
            where
                T::Error: core::convert::Into<$crate::pack::PackError<E>>,
            {
                target.append(&(self.to_le_bytes())).map_err(|e| e.into())
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

#[impl_for_tuples(10)]
impl<ErrType> Pack<ErrType> for Tuple {
    fn unpack(data: &[u8]) -> Result<(Self, &[u8]), ErrType> {
        for_tuples!( #( let (Tuple, data) = Tuple::unpack(data)?; )* );
        Ok(((for_tuples!( #( Tuple ),* )), data))
    }
    fn pack<T: PackTarget>(&self, target: T) -> Result<T, ErrType>
    where
        T::Error: Into<ErrType>,
    {
        for_tuples!( #( let target = Tuple.pack(target)?; )* );
        Ok(target)
    }
}

impl<E> Pack<PackError<E>> for bool {
    fn unpack(data: &[u8]) -> Result<(Self, &[u8]), PackError<E>> {
        let (x, data) = u8::unpack(data)?;
        Ok((x > 0, data))
    }
    fn pack<T: PackTarget>(&self, target: T) -> Result<T, PackError<E>>
    where
        T::Error: Into<PackError<E>>,
    {
        (*self as u8).pack(target)
    }
}

impl<T, E> PackTagged<E> for Option<T>
where
    T: Pack<E>,
{
    type Tag = bool;
    fn get_tag(&self) -> bool {
        self.is_some()
    }
    fn unpack_data(tag: bool, data: &[u8]) -> Result<(Self, &[u8]), E> {
        if tag {
            let (inner, data) = T::unpack(data)?;
            Ok((Some(inner), data))
        } else {
            Ok((None, data))
        }
    }
    fn pack_data<TA: PackTarget>(&self, target: TA) -> Result<TA, E>
    where
        TA::Error: Into<E>,
    {
        match self {
            Some(inner) => inner.pack(target),
            None => Ok(target),
        }
    }
}
