//! Which microphones this machine has: each input device's stable id and the name it is shown by.
//!
//! The id is cpal's `DeviceId`, which survives a reboot where the platform allows, so a chosen
//! microphone is kept by it; the name is only for people.

use cpal::traits::{DeviceTrait as _, HostTrait as _};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Microphone {
    pub id: String,
    pub name: String,
}

/// The input devices, in the host's order. A host that cannot list them has none.
pub fn microphones() -> Vec<Microphone> {
    let host = cpal::default_host();
    let Ok(devices) = host.input_devices() else { return Vec::new() };
    devices
        .filter_map(|device| {
            let id = device.id().ok()?.to_string();
            let name = device.description().map(|description| description.name().to_owned()).unwrap_or_else(|_| device.to_string());
            Some(Microphone { id, name })
        })
        .collect()
}

/// The device `id` names, or the system's default input when `id` is none or no longer there.
pub(crate) fn input(id: Option<&str>) -> Option<cpal::Device> {
    let host = cpal::default_host();
    id.and_then(|id| id.parse().ok())
        .and_then(|id| host.device_by_id(&id))
        .or_else(|| host.default_input_device())
}
