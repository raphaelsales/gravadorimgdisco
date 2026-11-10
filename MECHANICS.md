# Mechanics sheet — Gravador de Imagens (guided rebuild of ImgBurn 2.5.8.0 "Write image file to disc")

Studied: ImgBurn.exe taken from the NSIS installer and UPX-unpacked (sha256 241d7d8c…3c30).
Ghidra's decompiler was unavailable on this Mac, so the built-in decompiler was used.

## Rules found in the original code

| Rule | Original | Where |
|---|---|---|
| Capacity check: image sectors are compared with disc sectors; if image > disc, warn "There doesn't appear to be enough space on the disc to burn this image" (shows both in sectors and MiB) and offer overburn | ImgBurn offers overburn / BurnerMAX | sub_791378 (0x791378), strings 0xbf7c1d, 0xbf7e82 |
| Underburn warning: image that fits a smaller disc (e.g. DVD image on DL/Blu-ray) asks "Would you like to continue anyway?" | warning only | sub_791378 |
| Disc information shown: Status Empty / Incomplete / Complete / Others / Unknown; State of Last Session; Erasable; Free Sectors; Supported Write Speeds | same fields | sub_720398 (0x720398), sub_742c7c |
| Drive readiness before burning: poll up to 10 times, then up to 20 times, 100 ms apart; then "Program Not Ready For Burning! If you're sure there is a blank disc in the drive, try again in a few seconds!" | 10 + 20 tries × 100 ms | sub_444740 (0x444740) |
| Verify after write is a setting (ISOWRITE_Verify) | setting exists | sub_4c0140 (0x4c0140) |
| Disc manufacturer is looked up from the ATIP lead-in time (e.g. 97m26s6x = CMC Magnetics) | lookup table | sub_5fd008 (0x5fd008) |
| Drive error texts include "Incompatible Medium Installed", "Cannot Write Medium - Incompatible Format", "Power Calibration Area Is Full" | sense-code table | sub_5e7e70 (0x5e7e70) |

## What the rebuild changes (person's request)

* Six numbered steps (Imagem → Disco → Verificação → Opções → Gravar → Pronto), each with "O que fazer agora" instructions; "Próximo" stays locked with a sentence saying why.
* A compatibility check **before** burning that blocks (red) instead of only warning: not an image (.zip/.rar/.exe…), BIN without CUE, no disc, factory (ROM) disc, drive can't write that disc type, CD-R/DVD-R already used, DVD image on a CD, image bigger than free space. No overburn offer.
* Speed defaults to a safe value instead of the maximum; verify is on by default.

## Numbers (read from ImgBurn's code)

| Name | Value | Source |
|---|---|---|
| Sector size | 2048 bytes | ISO 9660 standard |
| Raw sector size (BIN) | 2352 bytes | CD standard |
| Standard 80-min CD limit | 359,845 sectors (`cmp eax, 0x57DA5` → "exceeds a standard 80 minute CD") | 0x54b212 (build mode, message 0xb28e90) |
| Smallest single-layer DVD (DVD+R) | 2,295,104 sectors (0x230540) | sub_791378 |
| DVD-R single layer | 2,297,888 sectors (0x231020) | sub_791378 |
| DVD-R double layer | 4,171,712 sectors (0x3fa7c0) | sub_791378 |
| DVD+R double layer | 4,173,824 sectors (0x3fb000) | sub_791378 |
| BD-R single layer | 12,219,392 sectors (0xba7400) | sub_791378 |
| BD-R double layer | 24,438,784 sectors (0x174e800) | sub_791378 |
| BD-R triple layer | 48,878,592 sectors (0x2e9d400) | sub_791378 |
| Fits rule | image sectors ≤ disc free sectors (`setae`, no margin); otherwise the overburn prompt | sub_791378: free = sub_5a02bc, image = sub_46f4a4 |
| Underburn rule | DVD-sized image on a DL or BD disc, or DL-sized on BD → "doesn't need to go on a … disc", continue anyway? (suppressed by a setting at 0xce77af) | sub_791378 |
| Disc status byte | 0 Empty, 1 Incomplete, 2 Complete, 3 Others | sub_720398 (strings 0xbdf6f2…0xbdf724) |
| Last session state | 0 Empty, 1 Incomplete, 2 Damaged, 3 Complete | sub_720398 (0xbdf733…0xbdf791) |
| Erasable | Yes/No flag; Free Sectors shown only when non-zero | sub_720398 (0xbdf7b1, 0xbdf8d3) |
| Ready poll, first loop | 10 tries | sub_444740 |
| Ready poll, second loop | 20 tries | sub_444740 |
| Ready poll interval | 100 ms (Sleep 0x64) | sub_444740 |
| Verify by default | on (`mov byte [0xce79fe], 1`, saved as ISOWRITE_Verify) | 0x4a8471, sub_4c0140 |
| Test mode by default | off (`mov byte [0xce79fd], 0`, ISOWRITE_TestMode) | 0x4a8468, sub_4c0140 |
| Write speed by default | 0 = MAX (`[0xce7a00] = 0`, ISOWRITE_WriteSpeed) | 0x4a8478, sub_4c0140 |
| Disc maker lookup | first 7 characters of the ATIP lead-in time (e.g. "97m26s6") matched against ~100 table entries (e.g. 97m26s6 → CMC Magnetics) | sub_5fd008 |
| "Almost full" warning | 98 % | estimated (mine, not in ImgBurn) |
| Safe CD-R speed | 16x | estimated (mine; ImgBurn defaults to MAX) |
| Safe DVD speed | 8x | estimated (mine) |
| 1x CD | 176 KB/s | standard |
| 1x DVD | 1385 KB/s | standard |
| ISO mark offset | byte 32769 ("CD001") | ISO 9660 standard |

### Where the rebuild knowingly differs
* Speed starts at the safe value, not ImgBurn's MAX: the person's ELGIN burns failed.
* Image larger than the free space is refused instead of offering overburn.
* The disc maker isn't shown for real drives: macOS drutil and Windows IMAPI2 don't report the ATIP lead-in, so sub_5fd008's table can't be used.

## Picture drawn on a CD-R ("Uma foto desenhada no CD")

Studied: Disc Image Studio source (folder the person pointed to), `src/DiscImageStudio.Cd` and
`src/DiscImageStudio.Core/Presets/built-in-disc-presets.json`. My own code except the CIRC delay
table (adapted, GPL-3.0-or-later, see NOTICE.md).

| Rule / number | Value | Source |
|---|---|---|
| The picture becomes one CD audio track (44.1 kHz, 16-bit, stereo WAV + CUE) | — | CdTrackGenerator.cs, CdWaveFile.cs |
| Dark byte / light byte | 0x0D / 0xAA | CdToneMapping.cs (two-level palette) |
| Bytes per audio sector | 2352 | CdDiscParameters.cs |
| Sectors per second | 75 | CD standard |
| Track speed | 1200 mm/s | CdDiscParameters.cs (ECMA-130) |
| Radius of byte b | r = √(r_in² + (b/B)·(r_out² − r_in²)) | CdDiscParameters.RadiusFromByte |
| Angle at radius r | θ = 2·v·T/(r_out² − r_in²) · (r − r_in) | CdDiscParameters.ThetaFromRadius (constant pitch) |
| Picture canvas edge | 57.5 mm | CdDiscParameters.StandardImageOuterRadiusMm |
| CIRC pre-shuffle | 24-byte frames, 28 delay lines × depth 4 = 112-frame ring | CddaInterleaveTable.cs (adapted) |
| WAV byte order | shuffle first, then swap each sample's 2 bytes (little-endian) | CdAudioByteOrder.cs |
| Standard 80 min CD | 359,849 sectors, 24.5–56.8 mm | built-in-disc-presets.json "cd-80" |
| Standard 74 min CD | 333,000 sectors, 24.5–56.8 mm | presets "cd-74" |
| Verbatim CD-R AZO 43438 | 359,848 sectors, 24.837775–58.020875 mm | presets (measured) |
| Ritek blue CD-R | 359,845 sectors, 24.911275–57.931155 mm | presets (measured) |
| Sectors drawn | preset − 2, and never more than free − 152 | presets usableSectors; 150-sector pregap (estimated) |
| Picture sampled every | 4 bytes (≈27 µm of track) | estimated (mine) |
| Dithering | Floyd–Steinberg, 1400×1400 working picture | mine |
| ELGIN CD-R measurements | none known; standard 80 min used | estimated |

Compatibility check for picture mode (checks.rs `run_picture`): needs a blank CD (CD-R best; CD-RW warns,
DVD/ROM refused), a drive that writes CD-R, enough free space, and warns when the disc brand has no
measurements (the drawing may come out slightly twisted).

Burning: macOS `drutil burn -audio` on the track's folder; Windows IMAPI2 Track-At-Once audio track
(untested); or "Só criar os arquivos" and open the .cue in ImgBurn's "Write image file to disc".

## Disc brands the person uses (step 2 "Seu disco")

| Disc | Image limit | Speed used | Photo drawing | Source |
|---|---|---|---|---|
| ELGIN CD-R 700 MB 52x 80 min | 359,845 sectors (ImgBurn's 80-min limit) | 16x or less (box says 52x = maximum) | yes, standard 80-min geometry | limit: 0x54b212; speed and geometry estimated |
| Multilaser DVD-R 4,7 GB 16x | 2,297,888 sectors (DVD-R limit, ≈4.38 GiB) | 8x or less | no (photo drawing is CD-only in this version) | limit: sub_791378 (0x231020); speed estimated |

The brand check (checks.rs `brand`) warns when the disc read by the drive doesn't match the chosen brand,
and refuses photo drawing on DVD-R. Neither brand has measured data in ImgBurn or Disc Image Studio.

## Write speed (read from ImgBurn's code)

| Rule | Value | Source |
|---|---|---|
| 1x CD | 176.4 KB/s (double at 0x4886c0) | table 0x4886c0, used at 0x484b8a |
| 1x DVD | 1385 KB/s (float at 0x4886c8) | used at 0x484bca |
| 1x HD DVD | 4568.75 KB/s (float at 0x4886cc) | used at 0x484c04 |
| 1x BD | 4495.5 KB/s (float at 0x4886d0) | used at 0x484c3e |
| Speed sent to the drive | KB/s = round(x × factor) (sub_4f5ce4 rounds) | 0x484b84..0x484c3e |
| "MAX" | stored as -1.0 (0x4886bc); sent as 0xFFFF for CD, 0xFFFFFFFF for DVD/BD | 0x484b7a, 0x484bbc |
| Speed list per disc | read from the drive's write speed descriptors ("Media Supported Write Speeds") | sub_638430 |

The app shows "16x = 2822 KB por segundo" with the same formula (mechanics.rs `speed_kbs`).

### Slower isn't always better (the person's question)
ImgBurn has no rule for this; the app's advice is estimated, from common burning guidance:
the top speed fails most often, but the very lowest speed can also burn worse on discs made for
high speeds (52x CD-R, 16x DVD-R). The app picks the fastest offered speed between a floor and
the safe speed: CD-R 8x–16x, DVD 4x–8x, and warns below the floor.

## Finishing options (read from ImgBurn's code)

Settings are saved by sub_4c0140 (each name → a byte in memory) and their defaults are set in the
block at 0x4a8xxx:

| Setting | Memory | Default | Where the default is set | Used in the app |
|---|---|---|---|---|
| DEVICE_EjectTrayWrite | 0xce7acb | off | 0x4a8844 | "Abrir a bandeja quando terminar" starts off |
| DEVICE_EjectTrayVerify | 0xce7acc | off | 0x4a884b | (same option) |
| ISOWRITE_CycleTrayBeforeVerify | 0xce79cb | on | 0x4a835f | not used: hdiutil/IMAPI verify by themselves |
| ISOWRITE_VerifyAgainstImageFile | 0xce79cc | on | 0x4a8366 | verify compares with the image (hdiutil -verifyburn) |
| SOUNDS_SuccessSoundAfterWrite | 0xce7b39 | on | 0x4a8aeb | system "done" sound (macOS Glass, Windows Asterisk) |
| SOUNDS_ErrorSoundAfterWrite | 0xce7b45 | on | 0x4a8c12 | system "error" sound (macOS Basso, Windows Hand) |

ImgBurn ships its own Success.wav / Error.wav (seen in the installer); the app uses the system's
sounds instead of copying those files.
