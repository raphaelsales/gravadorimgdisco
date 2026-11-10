// SPDX-License-Identifier: GPL-3.0-or-later
//! "Draw a picture on the disc": turns a JPG/PNG into one CD audio track whose
//! bytes make a pattern you can see on the burned side of a CD-R.
//!
//! How it works (see MECHANICS.md): the data spiral of a CD is read at a constant
//! speed, so every byte of the track lands at a known radius and angle. For each
//! byte we look at the picture at that point and write 0x0D (dark) or 0xAA (light).
//! The drive shuffles bytes (CIRC interleaving) before burning, so we pre-shuffle
//! them the opposite way so each byte ends up where we aimed it.
//!
//! The CIRC delay table below is adapted from Disc Image Studio
//! (GPL-3.0-or-later, (c) 2026 Disc Image Studio contributors), which adapted it
//! from CDImage by arduinocelentano (GPL, (c) 2008-2022). See NOTICE.md.

use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

/// Raw bytes in one CD audio sector.
pub const CD_SECTOR: u64 = 2352;
/// Sectors read per second at 1x.
pub const SECTORS_PER_SECOND: f64 = 75.0;
/// Byte drawn for dark areas / light areas.
pub const DARK_BYTE: u8 = 0x0D;
pub const LIGHT_BYTE: u8 = 0xAA;
/// Radius of the picture canvas edge (mm): the square picture's half-width.
pub const CANVAS_RADIUS_MM: f64 = 57.5;
/// Sectors the drive adds before track 1 (2-second pregap).
pub const PREGAP_SECTORS: u64 = 150;
/// Working resolution of the prepared picture (pixels per side).
pub const WORK_SIZE: u32 = 1400;

#[derive(Clone, Debug, PartialEq)]
pub struct DiscGeometry {
    pub name: &'static str,
    pub note: &'static str,
    pub inner_mm: f64,
    pub outer_mm: f64,
    pub sectors: u64,
    pub velocity_mm_s: f64,
}

pub const PRESETS: [DiscGeometry; 4] = [
    DiscGeometry { name: "CD-R padrão de 80 minutos (700 MB)", note: "Valores do padrão de CD. Serve para a maioria das marcas, inclusive as que não têm medição própria (ex.: ELGIN).", inner_mm: 24.5, outer_mm: 56.8, sectors: 359_849, velocity_mm_s: 1200.0 },
    DiscGeometry { name: "CD-R padrão de 74 minutos (650 MB)", note: "Discos mais antigos de 650 MB.", inner_mm: 24.5, outer_mm: 56.8, sectors: 333_000, velocity_mm_s: 1200.0 },
    DiscGeometry { name: "Verbatim CD-R AZO 43438 (medido)", note: "Medido em disco real pelo projeto Disc Image Studio.", inner_mm: 24.837775, outer_mm: 58.020875, sectors: 359_848, velocity_mm_s: 1200.0 },
    DiscGeometry { name: "Ritek CD-R azul (medido)", note: "Medido em disco real pelo projeto Disc Image Studio.", inner_mm: 24.911275, outer_mm: 57.931155, sectors: 359_845, velocity_mm_s: 1200.0 },
];

impl DiscGeometry {
    fn total_bytes(&self) -> f64 {
        (self.sectors * CD_SECTOR) as f64
    }
    fn total_seconds(&self) -> f64 {
        self.sectors as f64 / SECTORS_PER_SECOND
    }
    /// Radius (mm) where a byte of the track lands: equal areas for equal time.
    pub fn radius_at(&self, byte: u64) -> f64 {
        let f = (byte as f64 / self.total_bytes()).clamp(0.0, 1.0);
        (self.inner_mm * self.inner_mm + f * (self.outer_mm * self.outer_mm - self.inner_mm * self.inner_mm)).sqrt()
    }
    /// Angle (radians) of the spiral at a radius, for a constant track pitch.
    pub fn angle_at(&self, r: f64) -> f64 {
        let range = self.outer_mm * self.outer_mm - self.inner_mm * self.inner_mm;
        2.0 * self.velocity_mm_s * self.total_seconds() / range * (r - self.inner_mm)
    }
}

#[derive(Clone, Debug)]
pub struct Adjust {
    pub brightness: f32,
    pub contrast: f32,
    pub dither: bool,
    pub invert: bool,
    pub mirror: bool,
}

impl Default for Adjust {
    fn default() -> Self {
        Adjust { brightness: 0.0, contrast: 1.0, dither: true, invert: false, mirror: false }
    }
}

pub struct Picture {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    gray: Vec<u8>,
    /// true = light (0xAA), false = dark (0x0D).
    pub bits: Vec<bool>,
}

pub fn is_photo_ext(ext: &str) -> bool {
    matches!(ext, "jpg" | "jpeg" | "png" | "bmp" | "gif" | "webp")
}

impl Picture {
    /// A made-up sample picture (sun and waves) for previews without a file.
    pub fn sample() -> Picture {
        let n = WORK_SIZE as usize;
        let mut gray = vec![0u8; n * n];
        for y in 0..n {
            for x in 0..n {
                let fx = x as f32 / n as f32 - 0.5;
                let fy = y as f32 / n as f32 - 0.5;
                let d = (fx * fx + (fy + 0.08) * (fy + 0.08)).sqrt();
                let wave = (fy - 0.18 - 0.03 * (fx * 25.0).sin()).max(0.0);
                let v = if d < 0.13 { 250.0 } else if wave > 0.0 { 60.0 + 120.0 * ((fx * 40.0 + fy * 60.0).sin() * 0.5 + 0.5) } else { 70.0 + 140.0 * (0.5 - fy) };
                gray[y * n + x] = v.clamp(0.0, 255.0) as u8;
            }
        }
        let mut p = Picture { path: PathBuf::from("exemplo.png"), width: WORK_SIZE, height: WORK_SIZE, gray, bits: vec![] };
        p.apply(&Adjust::default());
        p
    }

    pub fn load(path: &Path) -> Result<Picture, String> {
        let img = ::image::open(path).map_err(|e| format!("Não foi possível abrir a foto: {e}"))?;
        let (w, h) = (img.width(), img.height());
        // Fit the picture into a square canvas (cover: the centre is kept).
        let side = w.min(h);
        let cropped = img.crop_imm((w - side) / 2, (h - side) / 2, side, side);
        let square = cropped.resize_exact(WORK_SIZE, WORK_SIZE, ::image::imageops::FilterType::Triangle).to_luma8();
        let mut p = Picture { path: path.to_path_buf(), width: w, height: h, gray: square.into_raw(), bits: vec![] };
        p.apply(&Adjust::default());
        Ok(p)
    }

    /// Recomputes the two-level picture (my own Floyd–Steinberg dithering).
    pub fn apply(&mut self, a: &Adjust) {
        let n = WORK_SIZE as usize;
        let mut buf: Vec<f32> = self
            .gray
            .iter()
            .map(|&g| {
                let v = (g as f32 / 255.0 - 0.5) * a.contrast + 0.5 + a.brightness;
                let v = v.clamp(0.0, 1.0);
                if a.invert { 1.0 - v } else { v }
            })
            .collect();
        let mut bits = vec![false; n * n];
        for y in 0..n {
            for x in 0..n {
                let i = y * n + x;
                let old = buf[i];
                let new = if old >= 0.5 { 1.0 } else { 0.0 };
                bits[i] = new > 0.5;
                if a.dither {
                    let e = old - new;
                    if x + 1 < n {
                        buf[i + 1] += e * 7.0 / 16.0;
                    }
                    if y + 1 < n {
                        if x > 0 {
                            buf[i + n - 1] += e * 3.0 / 16.0;
                        }
                        buf[i + n] += e * 5.0 / 16.0;
                        if x + 1 < n {
                            buf[i + n + 1] += e / 16.0;
                        }
                    }
                }
            }
        }
        if a.mirror {
            for y in 0..n {
                bits[y * n..(y + 1) * n].reverse();
            }
        }
        self.bits = bits;
    }

    /// Light or dark at a point on the disc (mm from the centre).
    #[inline]
    pub fn light_at(&self, x_mm: f64, y_mm: f64) -> bool {
        let n = WORK_SIZE as f64;
        let px = (n / 2.0 + x_mm / CANVAS_RADIUS_MM * n / 2.0) as i64;
        let py = (n / 2.0 + y_mm / CANVAS_RADIUS_MM * n / 2.0) as i64;
        if px < 0 || py < 0 || px >= n as i64 || py >= n as i64 {
            return true;
        }
        self.bits[py as usize * WORK_SIZE as usize + px as usize]
    }

    /// A picture of how the burned side should look, `size` pixels wide.
    pub fn preview(&self, size: usize, g: &DiscGeometry, sectors: u64) -> egui::ColorImage {
        let outer_written = g.radius_at(sectors * CD_SECTOR);
        let mut px = vec![egui::Color32::TRANSPARENT; size * size];
        let c = size as f64 / 2.0;
        let mm_per_px = 60.0 / c; // the image shows the whole 120 mm disc
        for y in 0..size {
            for x in 0..size {
                let dx = (x as f64 + 0.5 - c) * mm_per_px;
                let dy = (y as f64 + 0.5 - c) * mm_per_px;
                let r = (dx * dx + dy * dy).sqrt();
                px[y * size + x] = if r > 60.0 {
                    egui::Color32::TRANSPARENT
                } else if r < 7.5 {
                    egui::Color32::TRANSPARENT
                } else if r < g.inner_mm || r > outer_written {
                    egui::Color32::from_rgb(0xc9, 0xcd, 0xd6)
                } else if self.light_at(dx, dy) {
                    egui::Color32::from_rgb(0xe9, 0xe4, 0xd2)
                } else {
                    egui::Color32::from_rgb(0x3a, 0x2f, 0x5a)
                };
            }
        }
        egui::ColorImage { size: [size, size], pixels: px }
    }
}

// ---------------------------------------------------------------- CIRC pre-shuffle

const FRAME: usize = 24;
const CYCLE: usize = 112; // 28 delay lines x depth 4

/// Delay (in bytes) the drive's CIRC encoder applies to each of the 24 bytes of a frame.
fn circ_delays() -> [i64; FRAME] {
    let even = [0i64, 8, 16, 2, 10, 18];
    let odd = [4i64, 12, 20, 6, 14, 22];
    let mut d = [0i64; FRAME];
    for (i, slot) in d.iter_mut().enumerate() {
        let i64v = i as i64;
        if i < 12 {
            let o = (i & 1) as i64;
            *slot = o - 24 * (i64v * 4 + if o == 1 { 2 } else { 3 }) + even[i / 2];
        } else {
            let o = ((i - 12) & 1) as i64;
            *slot = o - 24 * ((i64v + 4) * 4 + if o == 1 { 0 } else { 1 }) + odd[(i - 12) / 2];
        }
    }
    d
}

/// For each byte position of a frame: how many frames later it is written, and in which column.
fn circ_plan() -> [(usize, usize); FRAME] {
    let d = circ_delays();
    let mut plan = [(0usize, 0usize); FRAME];
    for p in 0..FRAME {
        let col = d[p].rem_euclid(FRAME as i64);
        let frames_back = (col - d[p]) / FRAME as i64;
        plan[p] = ((CYCLE as i64 - 1 - frames_back) as usize, col as usize);
    }
    plan
}

pub enum GenEvent {
    Progress(f32),
    Done(Result<TrackFiles, String>),
}

#[derive(Clone, Debug)]
pub struct TrackFiles {
    pub wav: PathBuf,
    pub cue: PathBuf,
    pub folder: PathBuf,
    pub sectors: u64,
}

/// Writes `<folder>/faixa-desenho.wav` and `.cue` for the picture.
pub fn generate(
    bits: Vec<bool>,
    geometry: DiscGeometry,
    sectors: u64,
    folder: PathBuf,
    tx: std::sync::mpsc::Sender<GenEvent>,
) {
    std::thread::spawn(move || {
        let r = write_track(&bits, &geometry, sectors, &folder, &tx);
        let _ = tx.send(GenEvent::Done(r));
    });
}

fn light(bits: &[bool], x: f64, y: f64) -> bool {
    let n = WORK_SIZE as f64;
    let px = (n / 2.0 + x / CANVAS_RADIUS_MM * n / 2.0) as i64;
    let py = (n / 2.0 + y / CANVAS_RADIUS_MM * n / 2.0) as i64;
    if px < 0 || py < 0 || px >= n as i64 || py >= n as i64 {
        return true;
    }
    bits[py as usize * WORK_SIZE as usize + px as usize]
}

/// The byte for each logical position of the track, before the pre-shuffle.
fn fill_logical(bits: &[bool], g: &DiscGeometry, first_byte: u64, out: &mut [u8]) {
    // One look at the picture every 4 bytes (about 27 micrometres of track).
    let mut k = 0;
    while k < out.len() {
        let b = first_byte + k as u64;
        let r = g.radius_at(b);
        let t = g.angle_at(r);
        let v = if light(bits, r * t.cos(), r * t.sin()) { LIGHT_BYTE } else { DARK_BYTE };
        let end = (k + 4).min(out.len());
        out[k..end].fill(v);
        k = end;
    }
}

fn write_track(
    bits: &[bool],
    g: &DiscGeometry,
    sectors: u64,
    folder: &Path,
    tx: &std::sync::mpsc::Sender<GenEvent>,
) -> Result<TrackFiles, String> {
    std::fs::create_dir_all(folder).map_err(|e| format!("Não foi possível criar a pasta: {e}"))?;
    let wav = folder.join("faixa-desenho.wav");
    let cue = folder.join("faixa-desenho.cue");
    let total = sectors * CD_SECTOR;
    let file = std::fs::File::create(&wav).map_err(|e| format!("Não foi possível criar o arquivo: {e}"))?;
    let mut w = BufWriter::with_capacity(1 << 22, file);
    write_wav_header(&mut w, total).map_err(|e| e.to_string())?;

    let plan = circ_plan();
    let mut ring = vec![0u8; CYCLE * FRAME];
    let block_sectors: u64 = 512;
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 16);
    let mut done = 0u64;
    let mut frame_index = 0usize;
    let mut block = vec![0u8; (block_sectors * CD_SECTOR) as usize * threads];
    let mut out_frame = [0u8; FRAME];
    while done < sectors {
        let n_sec = (block_sectors * threads as u64).min(sectors - done);
        let len = (n_sec * CD_SECTOR) as usize;
        let buf = &mut block[..len];
        let start = done * CD_SECTOR;
        let chunk = ((block_sectors * CD_SECTOR) as usize).max(1);
        std::thread::scope(|s| {
            for (ci, part) in buf.chunks_mut(chunk).enumerate() {
                let first = start + (ci * chunk) as u64;
                s.spawn(move || fill_logical(bits, g, first, part));
            }
        });
        // Pre-shuffle frame by frame, then write in WAV order (little-endian samples).
        for f in buf.chunks(FRAME) {
            for p in 0..FRAME {
                let (later, col) = plan[p];
                ring[((frame_index + later) % CYCLE) * FRAME + col] = f[p];
            }
            let slot = (frame_index % CYCLE) * FRAME;
            out_frame.copy_from_slice(&ring[slot..slot + FRAME]);
            for pair in out_frame.chunks_mut(2) {
                pair.swap(0, 1);
            }
            w.write_all(&out_frame).map_err(|e| format!("Erro ao gravar o arquivo (disco do computador cheio?): {e}"))?;
            frame_index += 1;
        }
        done += n_sec;
        let _ = tx.send(GenEvent::Progress(done as f32 / sectors as f32));
    }
    w.flush().map_err(|e| e.to_string())?;
    std::fs::write(&cue, "FILE \"faixa-desenho.wav\" WAVE\r\n  TRACK 01 AUDIO\r\n    INDEX 01 00:00:00\r\n")
        .map_err(|e| e.to_string())?;
    Ok(TrackFiles { wav, cue, folder: folder.to_path_buf(), sectors })
}

fn write_wav_header(w: &mut impl Write, data: u64) -> std::io::Result<()> {
    let d = data as u32;
    w.write_all(b"RIFF")?;
    w.write_all(&(d + 36).to_le_bytes())?;
    w.write_all(b"WAVEfmt ")?;
    w.write_all(&16u32.to_le_bytes())?;
    w.write_all(&1u16.to_le_bytes())?; // PCM
    w.write_all(&2u16.to_le_bytes())?; // stereo
    w.write_all(&44_100u32.to_le_bytes())?;
    w.write_all(&176_400u32.to_le_bytes())?;
    w.write_all(&4u16.to_le_bytes())?;
    w.write_all(&16u16.to_le_bytes())?;
    w.write_all(b"data")?;
    w.write_all(&d.to_le_bytes())
}

/// How many sectors to draw on a disc with `free` free sectors (0 = unknown).
pub fn sectors_for(g: &DiscGeometry, free: u64) -> u64 {
    let usable = g.sectors.saturating_sub(2);
    if free == 0 {
        usable
    } else {
        usable.min(free.saturating_sub(PREGAP_SECTORS + 2))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circ_columns_are_a_permutation() {
        let mut seen = [false; FRAME];
        for (later, col) in circ_plan() {
            assert!(later < CYCLE);
            assert!(!seen[col]);
            seen[col] = true;
        }
    }

    #[test]
    fn spiral_reaches_the_edges() {
        let g = &PRESETS[0];
        assert!((g.radius_at(0) - 24.5).abs() < 1e-9);
        assert!((g.radius_at(g.sectors * CD_SECTOR) - 56.8).abs() < 1e-9);
    }

    #[test]
    #[ignore]
    fn timing_20000_sectors() {
        let p = Picture::sample();
        let dir = std::env::temp_dir().join(format!("gg_time_{}", std::process::id()));
        let (tx, _rx) = std::sync::mpsc::channel();
        let t0 = std::time::Instant::now();
        write_track(&p.bits, &PRESETS[0], 20_000, &dir, &tx).unwrap();
        println!("20000 sectors in {:?}", t0.elapsed());
    }

    #[test]
    fn small_track_is_written() {
        let bits = vec![true; (WORK_SIZE * WORK_SIZE) as usize];
        let dir = std::env::temp_dir().join(format!("gg_track_{}", std::process::id()));
        let (tx, _rx) = std::sync::mpsc::channel();
        let t = write_track(&bits, &PRESETS[0], 10, &dir, &tx).unwrap();
        let len = std::fs::metadata(&t.wav).unwrap().len();
        assert_eq!(len, 44 + 10 * CD_SECTOR);
    }
}
