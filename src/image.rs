//! Looks at the image file the person picked and says, in plain words, what it is.

use crate::mechanics::*;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub enum ImageKind {
    Iso,
    CueBin,
    BinOnly,
    OtherImage(String),
    NotAnImage(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum MediaNeed {
    Cd,
    DvdSingle,
    DvdDouble,
    BluRay,
    TooBig,
}

impl MediaNeed {
    pub fn label(&self) -> &'static str {
        match self {
            MediaNeed::Cd => "CD (CD-R ou CD-RW, 700 MB)",
            MediaNeed::DvdSingle => "DVD comum (DVD-R / DVD+R, 4,7 GB)",
            MediaNeed::DvdDouble => "DVD de camada dupla (DVD+R DL, 8,5 GB)",
            MediaNeed::BluRay => "Blu-ray (BD-R, 25 GB)",
            MediaNeed::TooBig => "nenhum disco comum (a imagem é grande demais)",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ImageInfo {
    pub path: PathBuf,
    /// The file that actually gets burned (the .cue for CUE/BIN, else the image).
    pub burn_path: PathBuf,
    pub kind: ImageKind,
    pub bytes: u64,
    /// Sectors the image needs on the disc.
    pub sectors: u64,
    pub volume_label: Option<String>,
    pub file_system: Option<String>,
    pub problems: Vec<String>,
    pub notes: Vec<String>,
}

impl ImageInfo {
    pub fn usable(&self) -> bool {
        !matches!(self.kind, ImageKind::NotAnImage(_)) && self.sectors > 0 && self.problems.is_empty()
    }

    pub fn need(&self) -> MediaNeed {
        need_for(self.sectors)
    }

    pub fn kind_label(&self) -> String {
        match &self.kind {
            ImageKind::Iso => "Imagem ISO".into(),
            ImageKind::CueBin => "Imagem BIN com folha CUE".into(),
            ImageKind::BinOnly => "Imagem BIN (sem o arquivo .cue)".into(),
            ImageKind::OtherImage(e) => format!("Imagem .{e}"),
            ImageKind::NotAnImage(e) => format!("Arquivo .{e} (não é imagem de disco)"),
        }
    }
}

pub fn need_for(sectors: u64) -> MediaNeed {
    if sectors <= CD80_SECTORS {
        MediaNeed::Cd
    } else if sectors <= DVD_SL_SECTORS {
        MediaNeed::DvdSingle
    } else if sectors <= DVD_MINUS_DL_SECTORS {
        MediaNeed::DvdDouble
    } else if sectors <= BD_SL_SECTORS {
        MediaNeed::BluRay
    } else {
        MediaNeed::TooBig
    }
}

pub fn human_size(bytes: u64) -> String {
    let b = bytes as f64;
    if b >= 1024.0 * 1024.0 * 1024.0 {
        format!("{:.2} GB", b / (1024.0 * 1024.0 * 1024.0))
    } else if b >= 1024.0 * 1024.0 {
        format!("{:.1} MB", b / (1024.0 * 1024.0))
    } else {
        format!("{:.0} KB", b / 1024.0)
    }
}

fn read_at(f: &mut File, off: u64, len: usize) -> Option<Vec<u8>> {
    let mut buf = vec![0u8; len];
    f.seek(SeekFrom::Start(off)).ok()?;
    f.read_exact(&mut buf).ok()?;
    Some(buf)
}

pub fn inspect(path: &Path) -> ImageInfo {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let mut info = ImageInfo {
        path: path.to_path_buf(),
        burn_path: path.to_path_buf(),
        kind: ImageKind::Iso,
        bytes: 0,
        sectors: 0,
        volume_label: None,
        file_system: None,
        problems: vec![],
        notes: vec![],
    };
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(_) => {
            info.problems.push("O arquivo não foi encontrado ou não pode ser aberto.".into());
            return info;
        }
    };
    if meta.is_dir() {
        info.kind = ImageKind::NotAnImage("pasta".into());
        info.problems.push(
            "Você escolheu uma pasta. Este programa grava um arquivo de imagem (.iso), não uma pasta.".into(),
        );
        return info;
    }
    info.bytes = meta.len();
    if info.bytes == 0 {
        info.problems.push("O arquivo está vazio (0 bytes). O download pode ter falhado.".into());
        return info;
    }

    match ext.as_str() {
        "zip" | "rar" | "7z" | "gz" | "tar" => {
            info.kind = ImageKind::NotAnImage(ext.clone());
            info.problems.push(format!(
                "Este é um arquivo compactado (.{ext}), não uma imagem de disco. Extraia-o primeiro (clique com o botão direito → Extrair) e escolha o arquivo .iso que estiver dentro."
            ));
        }
        "jpg" | "jpeg" | "png" | "bmp" | "gif" | "webp" => {
            info.kind = ImageKind::NotAnImage(ext.clone());
            info.problems.push(
                "Isto é uma foto, não uma imagem de disco. Para desenhar a foto na superfície do CD, escolha \"Uma foto desenhada no CD\" acima.".into(),
            );
        }
        "exe" | "msi" | "dmg" | "pkg" | "app" | "pdf" | "mp3" | "mp4" | "avi" | "mkv" | "doc" | "docx" => {
            info.kind = ImageKind::NotAnImage(ext.clone());
            info.problems.push(format!(
                "Arquivos .{ext} não são imagens de disco. Gravar este arquivo no modo \"imagem\" não funciona. Escolha um arquivo .iso, .img ou .cue."
            ));
        }
        "cue" => inspect_cue(path, &mut info),
        "bin" => {
            info.kind = ImageKind::BinOnly;
            let cue = path.with_extension("cue");
            if cue.exists() {
                info.notes.push("Encontramos o arquivo .cue ao lado do .bin; ele será usado.".into());
                inspect_cue(&cue, &mut info);
                info.path = path.to_path_buf();
            } else {
                let raw = info.bytes % RAW_SECTOR_BYTES == 0;
                info.sectors = if raw { info.bytes / RAW_SECTOR_BYTES } else { info.bytes.div_ceil(SECTOR_BYTES) };
                info.problems.push(
                    "Falta o arquivo .cue que acompanha este .bin. Sem ele o gravador não sabe como gravar. Coloque o .cue na mesma pasta ou escolha o .cue em vez do .bin.".into(),
                );
            }
        }
        "iso" | "img" | "udf" => inspect_iso(path, &mut info, &ext),
        "nrg" | "mdf" | "mds" | "ccd" | "cdi" => {
            info.kind = ImageKind::OtherImage(ext.clone());
            info.sectors = info.bytes.div_ceil(SECTOR_BYTES);
            info.notes.push(format!(
                "Imagens .{ext} são de outros programas e nem todo gravador aceita. Se possível, use a versão .iso."
            ));
        }
        _ => {
            // Unknown extension: look for the ISO mark anyway.
            inspect_iso(path, &mut info, &ext);
            if info.file_system.is_none() {
                info.kind = ImageKind::NotAnImage(if ext.is_empty() { "sem extensão".into() } else { ext.clone() });
                info.problems.push(
                    "Não reconhecemos este arquivo como imagem de disco (não tem a marca de um CD/DVD dentro).".into(),
                );
            }
        }
    }
    info
}

fn inspect_iso(path: &Path, info: &mut ImageInfo, ext: &str) {
    info.kind = if ext == "iso" || ext.is_empty() { ImageKind::Iso } else { ImageKind::OtherImage(ext.to_string()) };
    info.sectors = info.bytes.div_ceil(SECTOR_BYTES);
    if info.bytes % SECTOR_BYTES != 0 {
        info.notes.push(
            "O tamanho do arquivo não é múltiplo de 2048 bytes. Ele pode estar incompleto (download interrompido).".into(),
        );
    }
    let Ok(mut f) = File::open(path) else {
        info.problems.push("Não conseguimos ler o arquivo (sem permissão ou em uso).".into());
        return;
    };
    if let Some(desc) = read_at(&mut f, ISO_MARK_OFFSET - 1, 2048) {
        if &desc[1..6] == b"CD001" {
            info.file_system = Some("ISO 9660".into());
            let label = String::from_utf8_lossy(&desc[40..72]).trim_matches(|c: char| c == '\0' || c.is_whitespace()).to_string();
            if !label.is_empty() {
                info.volume_label = Some(label);
            }
            let declared = u32::from_le_bytes([desc[80], desc[81], desc[82], desc[83]]) as u64;
            if declared > 0 && declared > info.sectors + 16 {
                info.notes.push(format!(
                    "A imagem diz ter {declared} setores, mas o arquivo só tem {}. Ela parece estar cortada.",
                    info.sectors
                ));
            }
        } else if &desc[1..6] == b"BEA01" || &desc[1..6] == b"NSR02" || &desc[1..6] == b"NSR03" {
            info.file_system = Some("UDF".into());
        }
    }
    if info.file_system.is_none() {
        // Look a little further for a UDF anchor.
        for s in 17..20u64 {
            if let Some(d) = read_at(&mut f, s * SECTOR_BYTES, 6) {
                if &d[1..6] == b"BEA01" || &d[1..6] == b"NSR02" || &d[1..6] == b"NSR03" {
                    info.file_system = Some("UDF".into());
                }
            }
        }
    }
    if info.file_system.is_none() && ext == "iso" {
        info.notes.push(
            "Não encontramos a marca normal de CD/DVD dentro do arquivo. Ele pode estar corrompido ou não ser uma ISO de verdade.".into(),
        );
    }
}

fn inspect_cue(cue: &Path, info: &mut ImageInfo) {
    info.kind = ImageKind::CueBin;
    info.burn_path = cue.to_path_buf();
    let text = match std::fs::read_to_string(cue) {
        Ok(t) => t,
        Err(_) => {
            info.problems.push("Não conseguimos ler o arquivo .cue.".into());
            return;
        }
    };
    let dir = cue.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut total_bytes = 0u64;
    let mut sector = RAW_SECTOR_BYTES;
    let mut files = 0;
    for line in text.lines() {
        let l = line.trim();
        let upper = l.to_uppercase();
        if upper.starts_with("FILE") {
            files += 1;
            let name = l
                .split('"')
                .nth(1)
                .map(str::to_string)
                .or_else(|| l.split_whitespace().nth(1).map(str::to_string))
                .unwrap_or_default();
            let p = dir.join(&name);
            match std::fs::metadata(&p) {
                Ok(m) => total_bytes += m.len(),
                Err(_) => info.problems.push(format!(
                    "O .cue aponta para \"{name}\", mas esse arquivo não está na mesma pasta."
                )),
            }
        } else if upper.starts_with("TRACK") {
            if upper.contains("/2048") {
                sector = SECTOR_BYTES;
            } else if upper.contains("/2336") {
                sector = 2336;
            }
        }
    }
    if files == 0 {
        info.problems.push("O arquivo .cue não indica nenhum arquivo de dados (linha FILE).".into());
    }
    info.bytes = total_bytes.max(info.bytes);
    info.sectors = total_bytes / sector.max(1);
}
