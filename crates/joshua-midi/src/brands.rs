//! Catalogue of e-drum brands shown to the user. Each entry picks the note
//! map that brand's modules use by default; per-kit differences (extra rim,
//! edge or bell zones, user-edited maps) are fixed with MIDI learn.
//!
//! Connectivity notes are the general rule for current modules: USB is
//! class-compliant MIDI (no driver). Where the module also streams audio over
//! USB, Windows may need the vendor's audio driver; MIDI never does.

use serde::Serialize;

use crate::drums::MapPreset;

/// One brand / product family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KitBrand {
    pub id: &'static str,
    pub name: &'static str,
    /// Representative modules / kits.
    pub models: &'static str,
    pub preset: MapPreset,
    /// Some modules of this family also send audio over USB.
    pub usb_audio: bool,
}

pub const BRANDS: &[KitBrand] = &[
    KitBrand {
        id: "alesis",
        name: "Alesis",
        models: "Strata Prime, Strata Club, Strata Core, Strike Pro SE, Crimson II, Command, Surge, Turbo, Nitro / Nitro Max, Debut",
        preset: MapPreset::GeneralMidi,
        usb_audio: true,
    },
    KitBrand {
        id: "roland",
        name: "Roland V-Drums",
        models: "TD-02, TD-07, TD-17, TD-27, TD-50X, VAD series",
        preset: MapPreset::RolandVDrums,
        usb_audio: true,
    },
    KitBrand {
        id: "yamaha",
        name: "Yamaha DTX",
        models: "DTX402, DTX6, DTX8, DTX10, DTX-PRO",
        preset: MapPreset::GeneralMidi,
        usb_audio: true,
    },
    KitBrand {
        id: "efnote",
        name: "EFNOTE",
        models: "EFNOTE 3, 5, 7",
        preset: MapPreset::GeneralMidi,
        usb_audio: true,
    },
    KitBrand {
        id: "simmons",
        name: "Simmons",
        models: "Titan, SD600, SD1200",
        preset: MapPreset::GeneralMidi,
        usb_audio: false,
    },
    KitBrand { id: "pearl", name: "Pearl", models: "e/MERGE, Mimic", preset: MapPreset::GeneralMidi, usb_audio: true },
    KitBrand { id: "gewa", name: "GEWA", models: "G3, G5, G9", preset: MapPreset::GeneralMidi, usb_audio: true },
    KitBrand { id: "atv", name: "ATV", models: "aDrums, EXS", preset: MapPreset::GeneralMidi, usb_audio: true },
    KitBrand {
        id: "millenium",
        name: "Millenium (Thomann)",
        models: "MPS series",
        preset: MapPreset::GeneralMidi,
        usb_audio: false,
    },
    KitBrand {
        id: "budget",
        name: "Donner / Medeli / Behringer",
        models: "DED, DD, XD series",
        preset: MapPreset::GeneralMidi,
        usb_audio: false,
    },
    KitBrand {
        id: "generic",
        name: "Other / General MIDI",
        models: "Any module or USB-MIDI interface",
        preset: MapPreset::GeneralMidi,
        usb_audio: false,
    },
];

/// Looks up a brand by id (falls back to the generic entry).
#[must_use]
pub fn brand(id: &str) -> &'static KitBrand {
    BRANDS.iter().find(|b| b.id == id).unwrap_or(&BRANDS[BRANDS.len() - 1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_is_consistent() {
        let mut ids: Vec<_> = BRANDS.iter().map(|b| b.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), BRANDS.len(), "unique ids");
        assert_eq!(brand("alesis").name, "Alesis");
        assert_eq!(brand("roland").preset, MapPreset::RolandVDrums);
        assert_eq!(brand("../nope").id, "generic");
    }
}
