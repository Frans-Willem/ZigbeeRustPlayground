use crate::ieee802154::mac::commands::Command;
use crate::ieee802154::{ExtendedAddress, ShortAddress, PANID};
use crate::pack::{ExtEnum, Pack, PackError, PackTagged, PackTarget, UnpackError};
use bitfield::bitfield;

// TODO: Move Address & FullAddress somewhere in the main 802154 package?
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, PackTagged)]
#[tag_type(AddressingMode)]
pub enum Address {
    #[tag(AddressingMode::Short)]
    Short(ShortAddress),
    #[tag(AddressingMode::Extended)]
    Extended(ExtendedAddress),
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub struct FullAddress {
    pub pan_id: PANID,
    pub address: Address,
}

impl PackTagged for FullAddress
where
    Address: PackTagged,
{
    type Tag = AddressingMode;
    fn get_tag(&self) -> Self::Tag {
        self.address.get_tag()
    }
    fn unpack_data(tag: Self::Tag, data: &[u8]) -> Result<(Self, &[u8]), UnpackError> {
        let (pan_id, data) = PANID::unpack(data)?;
        let (address, data) = Address::unpack_data(tag, data)?;
        Ok((FullAddress { pan_id, address }, data))
    }
    fn pack_data<T: PackTarget>(&self, target: T) -> Result<T, PackError<T::Error>> {
        let target = self.pan_id.pack(target)?;
        self.address.pack_data(target)
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, ExtEnum)]
#[tag_type(u16)]
pub enum AddressingMode {
    None = 0,
    Reserved = 1,
    Short = 2,
    Extended = 3,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Frame<P> {
    pub frame_pending: bool,
    pub acknowledge_request: bool,
    pub sequence_number: Option<u8>,
    pub destination: Option<FullAddress>,
    pub source: Option<FullAddress>,
    pub frame_type: FrameType<P>,
}

bitfield! {
    #[derive(Pack)]
    pub struct FrameControl(u16);
    impl Debug;
    pub frame_type, set_frame_type: 2, 0;
    pub security_enabled, set_security_enabled: 3, 3;
    pub frame_pending, set_frame_pending: 4, 4;
    pub acknowledge_request, set_acknowledge_request: 5, 5;
    pub pan_id_compression, set_pan_id_compression: 6, 6;
    pub reserved, set_reserved: 7, 7;
    pub sequence_number_supression, set_sequence_number_supression: 8, 8;
    pub information_elements_present, set_information_elements_present: 9, 9;
    pub destination_addressing_mode, set_destination_addressing_mode: 11, 10;
    pub frame_version, set_frame_version: 13, 12;
    pub source_addressing_mode, set_source_addressing_mode: 15, 14;
}

impl<P: Pack> Frame<P> {
    fn unpack_address(
        mode: AddressingMode,
        previous_pan: Option<PANID>,
        data: &[u8],
    ) -> Result<(Option<FullAddress>, &[u8]), UnpackError> {
        if mode == AddressingMode::None {
            Ok((None, data))
        } else if let Some(pan_id) = previous_pan {
            let (address, data) = Address::unpack_data(mode, data)?;
            Ok((Some(FullAddress { pan_id, address }), data))
        } else {
            let (address, data) = FullAddress::unpack_data(mode, data)?;
            Ok((Some(address), data))
        }
    }
}

impl<P: Pack> Pack for Frame<P> {
    fn unpack(data: &[u8]) -> Result<(Self, &[u8]), UnpackError> {
        let (fc, data) = FrameControl::unpack(data)?;
        let frame_pending = fc.frame_pending() != 0;
        let acknowledge_request = fc.acknowledge_request() != 0;
        let (sequence_number, data) =
            <Option<u8>>::unpack_data(fc.sequence_number_supression() == 0, data)?;
        let (destination, data) = <Frame<P>>::unpack_address(
            AddressingMode::try_from_tag(fc.destination_addressing_mode())?,
            None,
            data,
        )?;
        let (source, data) = <Frame<P>>::unpack_address(
            AddressingMode::try_from_tag(fc.source_addressing_mode())?,
            if fc.pan_id_compression() != 0 {
                destination.map(|d| d.pan_id)
            } else {
                None
            },
            data,
        )?;
        if fc.information_elements_present() != 0 {
            return Err(UnpackError::Unimplemented(Some(
                "Information elements not implemented",
            )));
        }
        if fc.security_enabled() != 0 {
            return Err(UnpackError::Unimplemented(Some(
                "Secured frames not yet supported",
            )));
        }
        let (frame_type, data) = <FrameType<P>>::unpack_data(fc.frame_type() as u8, data)?;
        Ok((
            Frame {
                frame_pending,
                acknowledge_request,
                sequence_number,
                destination,
                source,
                frame_type,
            },
            data,
        ))
    }

    fn pack<T: PackTarget>(&self, target: T) -> Result<T, PackError<T::Error>> {
        let mut fc = FrameControl(0);
        fc.set_frame_type(self.frame_type.get_tag() as u16);
        fc.set_security_enabled(0);
        fc.set_frame_pending(self.frame_pending.into());
        fc.set_acknowledge_request(self.acknowledge_request.into());
        let destination_pan_id = self.destination.map(|a| a.pan_id);
        let source_pan_id = self.source.map(|a| a.pan_id);
        let pan_id_compression =
            destination_pan_id.is_some() && destination_pan_id == source_pan_id;
        fc.set_pan_id_compression(pan_id_compression.into());
        fc.set_reserved(0);
        fc.set_sequence_number_supression(self.sequence_number.is_none().into());
        fc.set_information_elements_present(0);
        fc.set_destination_addressing_mode(
            self.destination
                .map(|a| a.get_tag())
                .unwrap_or(AddressingMode::None)
                .into_tag(),
        );
        fc.set_source_addressing_mode(
            self.source
                .map(|a| a.get_tag())
                .unwrap_or(AddressingMode::None)
                .into_tag(),
        );
        fc.set_frame_version(0);
        let target = fc.pack(target)?;
        let target = self.sequence_number.pack_data(target)?;
        let target = if let Some(destination) = self.destination {
            destination.pack_data(target)?
        } else {
            target
        };
        let target = if let Some(source) = self.source {
            if pan_id_compression {
                source.address.pack_data(target)?
            } else {
                source.pack_data(target)?
            }
        } else {
            target
        };
        let target = self.frame_type.pack_data(target)?;
        Ok(target)
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct SlicePayload<'t>(&'t [u8]);
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct VecPayload(pub Vec<u8>);

impl Pack for VecPayload {
    fn unpack(data: &[u8]) -> Result<(Self, &[u8]), UnpackError> {
        Ok((VecPayload(<Vec<u8>>::from(data)), &data[data.len()..]))
    }

    fn pack<T: PackTarget>(&self, target: T) -> Result<T, PackError<T::Error>> {
        target.append(&self.0).map_err(PackError::TargetError)
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Beacon<P> {
    pub beacon_order: usize,
    pub superframe_order: usize,
    pub final_cap_slot: usize,
    pub battery_life_extension: bool,
    pub pan_coordinator: bool,
    pub association_permit: bool,
    pub payload: P,
}

bitfield! {
    #[derive(Pack)]
    struct SuperframeSpecification(u16);
    impl Debug;
    pub beacon_order, set_beacon_order: 3, 0;
    pub superframe_order, set_superframe_order: 7, 4;
    pub final_cap_slot, set_final_cap_slot: 11, 8;
    pub battery_life_extension, set_battery_life_extension: 12, 12;
    pub reserved, set_reserved: 13, 13;
    pub pan_coordinator, set_pan_coordinator: 14, 14;
    pub association_permit, set_association_permit: 15, 15;
}

impl<P: Pack> Pack for Beacon<P> {
    fn unpack(data: &[u8]) -> Result<(Self, &[u8]), UnpackError> {
        let ((ss, gts, pending_addresses), data) =
            <(SuperframeSpecification, u8, u8)>::unpack(data)?;
        if gts != 0 || pending_addresses != 0 {
            return Err(UnpackError::Unimplemented(Some(
                "Non-zero GTS or pending-addresses not yet supported",
            )));
        }
        let (payload, data) = <P>::unpack(data)?;
        Ok((
            Beacon {
                beacon_order: ss.beacon_order() as usize,
                superframe_order: ss.superframe_order() as usize,
                final_cap_slot: ss.final_cap_slot() as usize,
                battery_life_extension: ss.battery_life_extension() != 0,
                pan_coordinator: ss.pan_coordinator() != 0,
                association_permit: ss.association_permit() != 0,
                payload,
            },
            data,
        ))
    }

    fn pack<T: PackTarget>(&self, target: T) -> Result<T, PackError<T::Error>> {
        let mut ss = SuperframeSpecification(0);
        if self.beacon_order > 0xF {
            return Err(PackError::NotAllowed(Some("Beacon order out of range")));
        } else {
            ss.set_beacon_order(self.beacon_order as u16);
        }
        if self.superframe_order > 0xF {
            return Err(PackError::NotAllowed(Some("Superframe order out of range")));
        } else {
            ss.set_superframe_order(self.superframe_order as u16);
        }
        if self.final_cap_slot > 0xF {
            return Err(PackError::NotAllowed(Some(
                "Final cap slot order out of range",
            )));
        } else {
            ss.set_final_cap_slot(self.final_cap_slot as u16);
        }
        ss.set_battery_life_extension(self.battery_life_extension as u16);
        ss.set_reserved(0);
        ss.set_pan_coordinator(self.pan_coordinator as u16);
        ss.set_association_permit(self.association_permit as u16);
        let target = (ss, 0_u8, 0_u8).pack(target)?;
        self.payload.pack(target)
    }
}

#[derive(Debug, PartialEq, Eq, Clone, PackTagged)]
#[tag_type(u8)]
pub enum FrameType<P> {
    #[tag(0)]
    Beacon(Beacon<P>),
    #[tag(1)]
    Data(P),
    #[tag(2)]
    Ack(P),
    #[tag(3)]
    Command(Command),
    #[tag(4)]
    Reserved(P),
    #[tag(5)]
    Multipurpose(P),
    #[tag(6)]
    Fragment(P),
    #[tag(7)]
    Extended(P),
}
