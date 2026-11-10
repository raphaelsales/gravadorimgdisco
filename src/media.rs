//! Discs the person told us about, and what the app does with each.

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Unknown,
    CdR,
    DvdR,
}

#[derive(Clone, Debug)]
pub struct MediaProfile {
    pub name: &'static str,
    pub kind: Kind,
    /// Speed printed on the box (x); 0 when unknown.
    pub box_speed: u32,
    /// Speed we suggest instead (x).
    pub safe_speed: u32,
    /// Can this disc receive a drawn photo in this version?
    pub picture: bool,
}

pub const PROFILES: [MediaProfile; 5] = [
    MediaProfile { name: "Não sei / outra marca", kind: Kind::Unknown, box_speed: 0, safe_speed: 0, picture: true },
    MediaProfile { name: "ELGIN CD-R 700 MB 52x 80 min", kind: Kind::CdR, box_speed: 52, safe_speed: crate::mechanics::CD_SAFE_SPEED, picture: true },
    MediaProfile { name: "Multilaser DVD-R 4,7 GB 16x 120 min", kind: Kind::DvdR, box_speed: 16, safe_speed: crate::mechanics::DVD_SAFE_SPEED, picture: false },
    MediaProfile { name: "Outro CD-R de 700 MB", kind: Kind::CdR, box_speed: 0, safe_speed: crate::mechanics::CD_SAFE_SPEED, picture: true },
    MediaProfile { name: "Outro DVD-R ou DVD+R de 4,7 GB", kind: Kind::DvdR, box_speed: 0, safe_speed: crate::mechanics::DVD_SAFE_SPEED, picture: false },
];
