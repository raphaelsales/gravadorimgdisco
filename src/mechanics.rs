//! Numbers and rules taken from MECHANICS.md (keep both in sync with mechanics.json).

/// Bytes in one data sector of a CD/DVD image (Mode 1 / ISO 9660).
pub const SECTOR_BYTES: u64 = 2048;
/// Bytes in one raw sector of a BIN image (Mode 2352 / audio).
pub const RAW_SECTOR_BYTES: u64 = 2352;

/// Largest image ImgBurn accepts as "fits a standard 80 minute CD" (sectors).
/// Build mode compares image sectors with 0x57DA5 (0x54b212).
pub const CD80_SECTORS: u64 = 359_845;
/// Smallest single-layer DVD (DVD+R) capacity; ImgBurn's "fits on a DVD" test (0x791378).
pub const DVD_PLUS_SL_SECTORS: u64 = 2_295_104;
/// Single-layer DVD-R capacity (0x791378, constant 0x231020).
pub const DVD_SL_SECTORS: u64 = 2_297_888;
/// Double-layer DVD-R capacity (0x791378, constant 0x3fa7c0).
pub const DVD_MINUS_DL_SECTORS: u64 = 4_171_712;
/// Double-layer DVD+R capacity (0x791378, constant 0x3fb000).
pub const DVD_DL_SECTORS: u64 = 4_173_824;
/// Single-layer BD-R capacity (0x791378, constant 0xba7400).
pub const BD_SL_SECTORS: u64 = 12_219_392;
/// Double-layer BD-R capacity (0x791378, constant 0x174e800).
pub const BD_DL_SECTORS: u64 = 24_438_784;
/// Triple-layer BD-R capacity (0x791378, constant 0x2e9d400).
pub const BD_TL_SECTORS: u64 = 48_878_592;

/// Drive readiness: first loop tries, then second loop tries, each 100 ms apart (sub_444740).
pub const READY_FIRST_TRIES: u32 = 10;
pub const READY_SECOND_TRIES: u32 = 20;
pub const READY_SLEEP_MS: u64 = 100;

/// Share of the disc above which the image is "very close to the edge" (estimated).
pub const NEAR_FULL_RATIO: f64 = 0.98;

/// Safest speed we suggest for CD-R (x). Estimated (common guidance for budget media).
pub const CD_SAFE_SPEED: u32 = 16;
/// Safest speed we suggest for DVD±R (x). Estimated.
pub const DVD_SAFE_SPEED: u32 = 8;

/// Speed unit sizes in KB/s (1x).
/// ImgBurn's speed table (0x4886c0..0x4886d0), used as KB/s = round(x * factor) at 0x484b84..0x484c3e.
pub const CD_1X_KBS: f64 = 176.4;
pub const DVD_1X_KBS: f64 = 1385.0;
pub const HDDVD_1X_KBS: f64 = 4568.75;
pub const BD_1X_KBS: f64 = 4495.5;
/// "MAX" is stored as -1.0 (0x4886bc) and sent as 0xFFFF for CD, 0xFFFFFFFF for DVD/BD.
pub const SPEED_MAX_MARK: f64 = -1.0;
pub const CD_MAX_KBS_CODE: u32 = 65535;

/// Slowest speed we still suggest (x): below this, a disc made for high speeds
/// may not be tuned well. Estimated (common burning guidance, not in ImgBurn).
pub const CD_MIN_GOOD_SPEED: u32 = 8;
pub const DVD_MIN_GOOD_SPEED: u32 = 4;

/// KB/s for a speed in x, the way ImgBurn computes it.
pub fn speed_kbs(x: u32, one_x: f64) -> u32 {
    (x as f64 * one_x).round() as u32
}

/// Verify after writing: ImgBurn's default for ISOWRITE_Verify (byte 0xce79fe set to 1 at 0x4a8471).
pub const VERIFY_DEFAULT: bool = true;
/// Test mode: ImgBurn's default for ISOWRITE_TestMode (0xce79fd set to 0 at 0x4a8468).
pub const TEST_MODE_DEFAULT: bool = false;
/// ImgBurn's default write speed is 0 = "MAX" (ISOWRITE_WriteSpeed, 0xce7a00 set to 0 at 0x4a8478).
/// This rebuild deliberately starts at a safe speed instead (the person's failed burns).
pub const ORIGINAL_DEFAULT_SPEED_IS_MAX: bool = true;

/// DEVICE_EjectTrayWrite default: off (`mov byte [0xce7acb], 0` at 0x4a8844).
pub const EJECT_AFTER_WRITE_DEFAULT: bool = false;
/// SOUNDS_SuccessSoundAfterWrite default: on (0xce7b39 = 1 at 0x4a8aeb).
pub const SUCCESS_SOUND_DEFAULT: bool = true;
/// SOUNDS_ErrorSoundAfterWrite default: on (0xce7b45 = 1 at 0x4a8c12).
pub const ERROR_SOUND_DEFAULT: bool = true;

/// Where the ISO 9660 "CD001" mark sits in an image (byte offset of sector 16 + 1).
pub const ISO_MARK_OFFSET: u64 = 32_769;

pub fn mib(sectors: u64) -> f64 {
    (sectors * SECTOR_BYTES) as f64 / (1024.0 * 1024.0)
}
