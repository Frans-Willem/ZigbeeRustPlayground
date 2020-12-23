use crate::pack::{
    ExtEnum, ExtEnumError, Pack, PackError, PackTagged, SlicePackError, SlicePackTarget,
    VecPackTarget,
};
use core::convert::{Into, TryFrom};

fn test_roundtrip<T: core::fmt::Debug + Eq + PartialEq + Pack>(input: T, packed: Vec<u8>) {
    let (unpacked, remaining) = T::unpack(&packed).unwrap();
    assert!(remaining.is_empty());
    assert_eq!(unpacked, input);

    let repacked: Vec<u8> = input.pack(VecPackTarget::new()).unwrap().into();
    assert_eq!(packed, repacked);
}

#[test]
fn test_default_impls() {
    test_roundtrip::<bool>(false, vec![0x00]);
    test_roundtrip::<bool>(true, vec![0x01]);
    test_roundtrip::<u8>(0x12, vec![0x12]);
    test_roundtrip::<u16>(0x1234, vec![0x34, 0x12]);
    test_roundtrip::<u32>(0x12345678, vec![0x78, 0x56, 0x34, 0x12]);
    test_roundtrip::<u64>(
        0x0123456789ABCDEF,
        vec![0xEF, 0xCD, 0xAB, 0x89, 0x67, 0x45, 0x23, 0x01],
    );
}

#[test]
fn test_tuples() {
    test_roundtrip::<()>((), vec![]);
    test_roundtrip::<(u8, u16, u32, u64)>(
        (0x01, 0x2345, 0x6789ABCD, 0xDEADBEEFCAFEBABE),
        vec![
            0x01, 0x45, 0x23, 0xCD, 0xAB, 0x89, 0x67, 0xBE, 0xBA, 0xFE, 0xCA, 0xEF, 0xBE, 0xAD,
            0xDE,
        ],
    );
}

#[test]
fn test_slice_target() {
    let mut packed = [0; 8];
    let unpacked: (u16, u16, u16) = (0x1234, 0x5678, 0x1020);
    let size: usize = unpacked
        .pack(SlicePackTarget::new(&mut packed[..]))
        .unwrap()
        .into();
    assert_eq!(size, 6);
    assert_eq!(packed, [0x34, 0x12, 0x78, 0x56, 0x20, 0x10, 0, 0]);

    let mut packed = [0; 4];
    let unpacked: (u16, u16, u16) = (0x1234, 0x5678, 0x1020);
    assert_eq!(
        Err(PackError::TargetError(SlicePackError::NotEnoughSpace)),
        unpacked
            .pack(SlicePackTarget::new(&mut packed[..]))
            .map(core::mem::drop)
    );
}

#[test]
fn test_simple_struct() {
    #[derive(PartialEq, Eq, Debug, Pack)]
    struct TestF {
        x: u8,
        y: u16,
        z: u32,
    };
    test_roundtrip(TestF { x: 1, y: 2, z: 3 }, vec![1, 2, 0, 3, 0, 0, 0]);

    #[derive(PartialEq, Eq, Debug, Pack)]
    struct TestT(u8, u16, u32);
    test_roundtrip(TestT(1, 2, 3), vec![1, 2, 0, 3, 0, 0, 0]);

    #[derive(PartialEq, Eq, Debug, Pack)]
    struct EmptyStruct;
    test_roundtrip(EmptyStruct, vec![]);
}

#[test]
fn test_simple_enum() {
    #[derive(PartialEq, Eq, Debug, Pack)]
    #[tag_type(u8)]
    enum Test8 {
        A = 12,
        B = 34,
        #[tag(56)]
        C,
    }
    #[derive(PartialEq, Eq, Debug, Pack)]
    #[tag_type(u32)]
    enum Test32 {
        A = 12,
        B = 34,
        #[tag(56)]
        C,
    }
    test_roundtrip(Test8::A, vec![12]);
    test_roundtrip(Test8::B, vec![34]);
    test_roundtrip(Test8::C, vec![56]);
    test_roundtrip(Test32::A, vec![12, 0, 0, 0]);
    test_roundtrip(Test32::B, vec![34, 0, 0, 0]);
    test_roundtrip(Test32::C, vec![56, 0, 0, 0]);
}

#[test]
fn test_data_enum() {
    #[derive(PartialEq, Eq, Debug, Pack)]
    #[tag_type(u16)]
    enum Test {
        #[tag(12)]
        A(u8),
        #[tag(34)]
        B(u16, u32),
        #[tag(56)]
        C { a: u8 },
        #[tag(78)]
        D { a: u16, b: u32 },
    }
    test_roundtrip(Test::A(10), vec![12, 0, 10]);
    test_roundtrip(Test::B(10, 20), vec![34, 0, 10, 0, 20, 0, 0, 0]);
    test_roundtrip(Test::C { a: 10 }, vec![56, 0, 10]);
    test_roundtrip(Test::D { a: 10, b: 20 }, vec![78, 0, 10, 0, 20, 0, 0, 0]);
}

fn test_roundtrip_tag<T: core::fmt::Debug + Eq + PartialEq + PackTagged>(
    input: T,
    tag: T::Tag,
    packed: Vec<u8>,
) where
    T::Tag: core::fmt::Debug + PartialEq + Eq,
{
    let expected_tag = input.get_tag();
    assert_eq!(tag, expected_tag);
    let (unpacked, remaining) = T::unpack_data(tag, &packed).unwrap();
    assert!(remaining.is_empty());
    assert_eq!(unpacked, input);

    let repacked: Vec<u8> = input.pack_data(VecPackTarget::new()).unwrap().into();
    assert_eq!(packed, repacked);
}

#[test]
fn test_enum_tagged() {
    #[derive(PartialEq, Eq, Debug, Clone, Copy)]
    enum TestTag {
        A,
        B,
        C,
        D,
    }
    #[derive(PartialEq, Eq, Debug, PackTagged)]
    #[tag_type(TestTag)]
    enum Test {
        #[tag(TestTag::A)]
        A(u8),
        #[tag(TestTag::B)]
        B(u16),
        #[tag(TestTag::C)]
        C(u32),
    }
    test_roundtrip_tag(Test::A(12), TestTag::A, vec![12]);
    test_roundtrip_tag(Test::B(0x0201), TestTag::B, vec![0x01, 0x02]);
    test_roundtrip_tag(
        Test::C(0x04030201),
        TestTag::C,
        vec![0x01, 0x02, 0x03, 0x04],
    );
}

#[test]
fn test_ext_enum() {
    #[derive(PartialEq, Eq, Debug, Clone, Copy, ExtEnum)]
    #[tag_type(u8)]
    enum Test {
        A = 0,
        B = 1,
        #[tag(2)]
        C,
        D = 3,
    }
    assert_eq!(Test::A.into_tag(), 0_u8);
    assert_eq!(Test::B.into_tag(), 1_u8);
    assert_eq!(Test::C.into_tag(), 2_u8);
    assert_eq!(Test::D.into_tag(), 3_u8);
    assert_eq!(core::convert::Into::<u8>::into(Test::A), 0_u8);
    assert_eq!(core::convert::Into::<u8>::into(Test::B), 1_u8);
    assert_eq!(core::convert::Into::<u8>::into(Test::C), 2_u8);
    assert_eq!(core::convert::Into::<u8>::into(Test::D), 3_u8);
    assert_eq!(Test::try_from_tag(0), Ok(Test::A));
    assert_eq!(Test::try_from_tag(1), Ok(Test::B));
    assert_eq!(Test::try_from_tag(2), Ok(Test::C));
    assert_eq!(Test::try_from_tag(3), Ok(Test::D));
    assert_eq!(Test::try_from_tag(4), Err(ExtEnumError::InvalidTag));
    assert_eq!(Test::try_from(0_u8), Ok(Test::A));
    assert_eq!(Test::try_from(1_u8), Ok(Test::B));
    assert_eq!(Test::try_from(2_u8), Ok(Test::C));
    assert_eq!(Test::try_from(3_u8), Ok(Test::D));
    assert_eq!(Test::try_from(4_u8), Err(ExtEnumError::InvalidTag));
}
