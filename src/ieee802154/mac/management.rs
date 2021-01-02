use crate::ieee802154::frame;
use crate::ieee802154::mac::data::{DataRequest, DataService};
use crate::ieee802154::pib;
use crate::ieee802154::services::mlme;
use crate::ieee802154::{ShortAddress, PANID};
use crate::unique_key::UniqueKey;

pub struct ManagementService {
    outgoing: Vec<frame::Frame>,
}

impl ManagementService {
    pub fn new(pib: &pib::PIB) -> Self {
        Self {
            outgoing: Vec::new(),
        }
    }

    fn update_radio_parameters(&mut self, pib: &pib::PIB) {
        // TODO: Update radio backed parameters from PIB.
    }

    /**
     * Handles MLME-BEACON.request
     * Returns a frame to be sent to the radio, or an error.
     */
    pub fn process_mlme_beacon_request(
        &mut self,
        pib: &mut pib::PIB,
        request: mlme::BeaconRequest,
    ) -> Result<(), mlme::Error> {
        if request.superframe_order != 15
            || request.channel != pib.phy_current_channel
            || request.channel_page != 0
        {
            return Err(mlme::Error::InvalidParameter);
        }
        let beacon = frame::Beacon {
            beacon_order: 15,
            superframe_order: request.superframe_order,
            final_cap_slot: 15,
            battery_life_extension: false,
            pan_coordinator: pib.mac_associated_pan_coord
                == Some((pib.mac_extended_address, pib.mac_short_address)),
            association_permit: pib.mac_association_permit,
            payload: frame::Payload(pib.mac_beacon_payload.clone()),
        };
        let frame = frame::Frame {
            frame_pending: false,
            acknowledge_request: false,
            sequence_number: Some(pib.next_beacon_sequence_nr()),
            destination: None,
            source: Some(pib.get_full_short_address()),
            frame_type: frame::FrameType::Beacon(beacon),
        };
        self.outgoing.push(frame);
        Ok(())
    }

    /**
     * Handles an MLME-RESET.request
     */
    pub fn process_mlme_reset_request(
        &mut self,
        data: &mut DataService,
        pib: &mut pib::PIB,
        request: mlme::ResetRequest,
    ) -> Result<(), mlme::Error> {
        if request.set_default_pib {
            pib.reset();
        }
        *self = Self::new(pib);
        *data = DataService::new();
        Ok(())
    }

    /**
     * Handles an MLME-GET.request
     */
    pub fn process_mlme_get_request(
        &mut self,
        pib: &pib::PIB,
        request: mlme::GetRequest,
    ) -> Result<pib::PIBValue, mlme::Error> {
        pib.get(request.attribute)
            .or(Err(mlme::Error::UnsupportedAttribute))
    }

    /**
     * Handles an MLME-SET.request
     */
    pub fn process_mlme_set_request(
        &mut self,
        pib: &mut pib::PIB,
        request: mlme::SetRequest,
    ) -> Result<(), mlme::Error> {
        let res = pib.set(request.attribute, request.value);
        self.update_radio_parameters(pib);
        res
    }

    /**
     * Handles an MLME-START.request
     */
    pub fn process_mlme_start_request(
        &mut self,
        pib: &mut pib::PIB,
        request: mlme::StartRequest,
    ) -> Result<(), mlme::Error> {
        if pib.mac_short_address == ShortAddress(0xFFFF) {
            return Err(mlme::Error::NoShortAddress);
        }
        if request.channel_page != 0
            || request.start_time != 0
            || request.beacon_order != 15
            || request.superframe_order != 15
            || !request.pan_coordinator
            || request.battery_life_extension
        {
            return Err(mlme::Error::InvalidParameter);
        }
        pib.phy_current_channel = request.channel_number;
        pib.mac_pan_id = request.pan_id;
        if request.pan_coordinator {
            pib.mac_associated_pan_coord = Some((pib.mac_extended_address, pib.mac_short_address));
        }
        self.update_radio_parameters(pib);
        Ok(())
    }

    /**
     * Handles MLME requests
     */
    pub fn process_mlme_request(
        &mut self,
        pib: &mut pib::PIB,
        data: &mut DataService,
        request: mlme::Request,
    ) -> Option<mlme::Confirm> {
        match request {
            mlme::Request::Beacon(request) => Some(mlme::Confirm::Beacon(
                self.process_mlme_beacon_request(pib, request),
            )),
            mlme::Request::Reset(request) => Some(mlme::Confirm::Reset(
                self.process_mlme_reset_request(data, pib, request),
            )),
            mlme::Request::Start(request) => Some(mlme::Confirm::Start(
                self.process_mlme_start_request(pib, request),
            )),
            mlme::Request::Get(request) => Some(mlme::Confirm::Get(
                request.attribute,
                self.process_mlme_get_request(pib, request),
            )),
            mlme::Request::Set(request) => Some(mlme::Confirm::Set(
                request.attribute,
                self.process_mlme_set_request(pib, request),
            )),
        }
    }
}

impl ManagementService {
    pub fn process_frame(
        &mut self,
        pib: &mut pib::PIB,
        frame: &frame::Frame,
    ) -> Option<mlme::Indication> {
        match &frame.frame_type {
            frame::FrameType::Command(frame::Command::BeaconRequest()) => {
                self.process_frame_beacon_request(pib, frame)
            }
            frame::FrameType::Command(frame::Command::AssociationRequest(req)) => {
                self.process_frame_association_request(pib, frame, req)
            }
            _ => None,
        }
    }

    pub fn process_frame_beacon_request(
        &mut self,
        pib: &mut pib::PIB,
        frame: &frame::Frame,
    ) -> Option<mlme::Indication> {
        let beacon_type = mlme::BeaconType::Beacon; // NOTE: Cheating, we should check the frame more carefully.
        if pib.mac_beacon_auto_respond {
            let request = mlme::BeaconRequest {
                beacon_type,
                channel: pib.phy_current_channel,
                channel_page: 0,
                superframe_order: 15,
                dst_addr: frame.source,
            };
            self.process_mlme_beacon_request(pib, request).unwrap_or(());
            None
        } else {
            Some(mlme::Indication::BeaconRequest {
                beacon_type: mlme::BeaconType::Beacon,
                src_addr: frame.source,
                dst_pan_id: frame
                    .destination
                    .map_or(PANID::broadcast(), |full_address| full_address.pan_id),
            })
        }
    }

    pub fn process_frame_association_request(
        &self,
        pib: &pib::PIB,
        frame: &frame::Frame,
        capability_information: &frame::CapabilityInformation,
    ) -> Option<mlme::Indication> {
        if !pib.mac_association_permit {
            println!("Ignoring: Association not allowed");
            return None;
        }
        if frame.destination != Some(pib.get_full_short_address()) {
            println!("Ignoring: Association request not meant for me");
            return None;
        }
        if let Some(frame::FullAddress {
            pan_id: source_pan_id,
            address: frame::Address::Extended(device_address),
        }) = frame.source
        {
            if source_pan_id != PANID::broadcast() {
                println!("Warning: Source PAN ID was not correctly set to broadcast");
            }
            let device_address = device_address;
            let capability_information = capability_information.clone();
            Some(mlme::Indication::Associate {
                device_address,
                capability_information,
            })
        } else {
            println!("Invalid source address in AssociationRequest");
            None
        }
    }
}
