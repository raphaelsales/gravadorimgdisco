//! Disc drives: finding them, reading what disc is inside, and burning.
//! Real drives use the system's own burning tools (macOS: drutil/hdiutil,
//! Windows: the IMAPI2 service through PowerShell). A practice drive lets the
//! person walk through every step without a burner.

use crate::mechanics::*;
use std::process::Command;

#[derive(Clone, Debug, PartialEq)]
pub enum Backend {
    Practice,
    MacOs { index: usize },
    Windows { recorder_id: String },
}

#[derive(Clone, Debug)]
pub struct Drive {
    pub name: String,
    pub backend: Backend,
    /// What the drive says it can write (CD-R, DVD-R...). Empty when unknown.
    pub can_write: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DiscStatus {
    Empty,
    Incomplete,
    Complete,
    Others,
    Unknown,
}

impl DiscStatus {
    pub fn label(&self) -> &'static str {
        match self {
            DiscStatus::Empty => "Vazio (virgem)",
            DiscStatus::Incomplete => "Já tem dados, ainda aceita mais (sessão aberta)",
            DiscStatus::Complete => "Já gravado e fechado (finalizado)",
            DiscStatus::Others => "Outro",
            DiscStatus::Unknown => "Desconhecido",
        }
    }
}

#[derive(Clone, Debug)]
pub struct DiscInfo {
    pub present: bool,
    pub ready: bool,
    /// e.g. "CD-R", "CD-RW", "DVD-R", "DVD+R DL", "CD-ROM", "DVD-ROM".
    pub media_type: String,
    pub status: DiscStatus,
    pub erasable: Option<bool>,
    pub free_sectors: u64,
    pub total_sectors: u64,
    /// Write speeds the drive offers for this disc (x).
    pub speeds: Vec<u32>,
    /// The system said this disc can be written in this drive.
    pub supported: Option<bool>,
    pub manufacturer: Option<String>,
    pub raw: String,
}

impl DiscInfo {
    pub fn none(raw: String) -> Self {
        DiscInfo {
            present: false,
            ready: false,
            media_type: String::new(),
            status: DiscStatus::Unknown,
            erasable: None,
            free_sectors: 0,
            total_sectors: 0,
            speeds: vec![],
            supported: None,
            manufacturer: None,
            raw,
        }
    }
    pub fn is_cd(&self) -> bool {
        self.media_type.starts_with("CD")
    }
    pub fn is_dvd(&self) -> bool {
        self.media_type.starts_with("DVD")
    }
    pub fn is_bd(&self) -> bool {
        self.media_type.starts_with("BD")
    }
    pub fn is_rom(&self) -> bool {
        self.media_type.contains("ROM")
    }
    pub fn is_rewritable(&self) -> bool {
        self.media_type.contains("RW") || self.media_type.contains("RE") || self.erasable == Some(true)
    }
}

/// Discs the practice drive can pretend to hold.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PracticeDisc {
    None,
    BlankCdR,
    UsedCdR,
    BlankCdRw,
    BlankDvdR,
    PressedCd,
}

impl PracticeDisc {
    pub const ALL: [PracticeDisc; 6] = [
        PracticeDisc::BlankCdR,
        PracticeDisc::BlankDvdR,
        PracticeDisc::BlankCdRw,
        PracticeDisc::UsedCdR,
        PracticeDisc::PressedCd,
        PracticeDisc::None,
    ];
    pub fn label(&self) -> &'static str {
        match self {
            PracticeDisc::None => "Nenhum disco na bandeja",
            PracticeDisc::BlankCdR => "CD-R virgem (700 MB)",
            PracticeDisc::UsedCdR => "CD-R já gravado",
            PracticeDisc::BlankCdRw => "CD-RW regravável",
            PracticeDisc::BlankDvdR => "DVD-R virgem (4,7 GB)",
            PracticeDisc::PressedCd => "CD comprado/original (só leitura)",
        }
    }
}

pub fn practice_disc(d: PracticeDisc) -> DiscInfo {
    let mut i = DiscInfo::none("Gravador de treino (simulado)".into());
    let cd_speeds = vec![4, 8, 16, 24, 32, 48];
    match d {
        PracticeDisc::None => return i,
        PracticeDisc::BlankCdR => {
            i.media_type = "CD-R".into();
            i.status = DiscStatus::Empty;
            i.erasable = Some(false);
            i.free_sectors = CD80_SECTORS + 4;
            i.speeds = cd_speeds;
            i.manufacturer = Some("CMC Magnetics Corp. (exemplo)".into());
        }
        PracticeDisc::UsedCdR => {
            i.media_type = "CD-R".into();
            i.status = DiscStatus::Complete;
            i.erasable = Some(false);
            i.free_sectors = 0;
            i.speeds = cd_speeds;
        }
        PracticeDisc::BlankCdRw => {
            i.media_type = "CD-RW".into();
            i.status = DiscStatus::Complete;
            i.erasable = Some(true);
            i.free_sectors = CD80_SECTORS + 4;
            i.speeds = vec![4, 10, 16];
        }
        PracticeDisc::BlankDvdR => {
            i.media_type = "DVD-R".into();
            i.status = DiscStatus::Empty;
            i.erasable = Some(false);
            i.free_sectors = DVD_SL_SECTORS;
            i.speeds = vec![4, 8, 16];
        }
        PracticeDisc::PressedCd => {
            i.media_type = "CD-ROM".into();
            i.status = DiscStatus::Complete;
            i.erasable = Some(false);
            i.supported = Some(false);
        }
    }
    i.present = true;
    i.ready = true;
    i.total_sectors = i.free_sectors.max(if i.is_cd() { CD80_SECTORS } else { DVD_SL_SECTORS });
    if i.supported.is_none() {
        i.supported = Some(true);
    }
    i
}

pub fn practice_drive() -> Drive {
    Drive {
        name: "Gravador de treino (simulado, não grava de verdade)".into(),
        backend: Backend::Practice,
        can_write: vec!["CD-R".into(), "CD-RW".into(), "DVD-R".into(), "DVD+R".into()],
    }
}

/// A command that doesn't flash a black console window on Windows.
fn quiet(cmd: &str) -> Command {
    #[allow(unused_mut)]
    let mut c = Command::new(cmd);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    c
}

fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let out = quiet(cmd).args(args).output().ok()?;
    let mut s = String::from_utf8_lossy(&out.stdout).to_string();
    s.push_str(&String::from_utf8_lossy(&out.stderr));
    Some(s)
}

// ---------------------------------------------------------------- finding drives

pub fn find_drives() -> Vec<Drive> {
    let mut v = vec![];
    if cfg!(target_os = "macos") {
        v.extend(find_mac());
    } else if cfg!(target_os = "windows") {
        v.extend(find_windows());
    }
    v.push(practice_drive());
    v
}

fn find_mac() -> Vec<Drive> {
    let Some(out) = run("drutil", &["list"]) else { return vec![] };
    let mut v = vec![];
    for line in out.lines().skip(1) {
        let t = line.trim();
        let mut parts = t.split_whitespace();
        let Some(idx) = parts.next().and_then(|n| n.parse::<usize>().ok()) else { continue };
        let rest: Vec<&str> = parts.collect();
        let name = rest.iter().take(rest.len().saturating_sub(2)).cloned().collect::<Vec<_>>().join(" ");
        let mut can_write = vec![];
        if let Some(info) = run("drutil", &["info", "-drive", &idx.to_string()]) {
            for l in info.lines() {
                if l.contains("CD-Write") || l.contains("DVD-Write") || l.contains("BD-Write") {
                    if let Some((_, val)) = l.split_once(':') {
                        can_write.extend(val.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()));
                    }
                }
            }
        }
        v.push(Drive { name: if name.is_empty() { format!("Gravador {idx}") } else { name }, backend: Backend::MacOs { index: idx }, can_write });
    }
    v
}

const PS_LIST: &str = r#"
$ErrorActionPreference='SilentlyContinue'
$m = New-Object -ComObject IMAPI2.MsftDiscMaster2
foreach ($id in $m) {
  $r = New-Object -ComObject IMAPI2.MsftDiscRecorder2
  $r.InitializeDiscRecorder($id)
  $f = New-Object -ComObject IMAPI2.MsftDiscFormat2Data
  $types = @()
  foreach ($t in $r.SupportedProfiles) { $types += $t }
  Write-Output ("DRIVE|" + $id + "|" + ($r.VolumePathNames -join ',') + " " + $r.VendorId.Trim() + " " + $r.ProductId.Trim() + "|" + ($types -join ','))
}
"#;

fn powershell(script: &str) -> Option<String> {
    run("powershell", &["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script])
}

fn profile_name(code: u32) -> Option<&'static str> {
    // IMAPI_PROFILE_TYPE values for writable media.
    Some(match code {
        0x08 => "CD-ROM",
        0x09 => "CD-R",
        0x0A => "CD-RW",
        0x10 => "DVD-ROM",
        0x11 => "DVD-R",
        0x13 | 0x14 => "DVD-RW",
        0x15 | 0x16 => "DVD-R DL",
        0x1A => "DVD+RW",
        0x1B => "DVD+R",
        0x2B => "DVD+R DL",
        0x41 => "BD-R",
        0x43 => "BD-RE",
        _ => return None,
    })
}

fn find_windows() -> Vec<Drive> {
    let Some(out) = powershell(PS_LIST) else { return vec![] };
    out.lines()
        .filter_map(|l| l.strip_prefix("DRIVE|"))
        .filter_map(|l| {
            let p: Vec<&str> = l.split('|').collect();
            if p.len() < 3 {
                return None;
            }
            let can_write = p
                .get(2)
                .map(|s| {
                    s.split(',')
                        .filter_map(|c| c.trim().parse::<u32>().ok())
                        .filter_map(profile_name)
                        .filter(|n| !n.contains("ROM"))
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default();
            Some(Drive { name: p[1].trim().to_string(), backend: Backend::Windows { recorder_id: p[0].to_string() }, can_write })
        })
        .collect()
}

// ---------------------------------------------------------------- reading the disc

pub fn read_disc(drive: &Drive, practice: PracticeDisc) -> DiscInfo {
    match &drive.backend {
        Backend::Practice => {
            // Same patience as the original: wait for the drive to settle.
            std::thread::sleep(std::time::Duration::from_millis(READY_SLEEP_MS * 6));
            practice_disc(practice)
        }
        Backend::MacOs { index } => read_mac(*index),
        Backend::Windows { recorder_id } => read_windows(recorder_id),
    }
}

/// Parses `drutil status` output.
pub fn parse_drutil_status(out: &str) -> DiscInfo {
    let mut i = DiscInfo::none(out.to_string());
    if out.contains("No Media Inserted") || out.contains("No media") {
        i.ready = true;
        return i;
    }
    for l in out.lines() {
        let t = l.trim();
        if let Some(rest) = t.strip_prefix("Type:") {
            let ty = rest.split_whitespace().next().unwrap_or("").to_string();
            if !ty.is_empty() {
                i.present = true;
                i.ready = true;
                i.media_type = ty;
            }
        } else if t.starts_with("Space Free:") {
            if let Some(b) = t.split("blocks:").nth(1) {
                i.free_sectors = b.split_whitespace().next().and_then(|n| n.parse().ok()).unwrap_or(0);
            }
        } else if t.starts_with("Space Used:") {
            if let Some(b) = t.split("blocks:").nth(1) {
                let used: u64 = b.split_whitespace().next().and_then(|n| n.parse().ok()).unwrap_or(0);
                i.total_sectors = used;
            }
        } else if t.starts_with("Writability:") {
            let w = t.to_lowercase();
            i.status = if w.contains("blank") {
                DiscStatus::Empty
            } else if w.contains("appendable") {
                DiscStatus::Incomplete
            } else {
                DiscStatus::Complete
            };
            i.erasable = Some(w.contains("erasable") || w.contains("overwritable"));
        }
    }
    i.total_sectors += i.free_sectors;
    if i.present && i.status == DiscStatus::Unknown {
        i.status = if i.media_type.contains("ROM") { DiscStatus::Complete } else { DiscStatus::Unknown };
    }
    i
}

fn read_mac(index: usize) -> DiscInfo {
    let idx = index.to_string();
    let mut last = String::new();
    // Poll like the original: up to READY_FIRST_TRIES + READY_SECOND_TRIES tries.
    for _ in 0..(READY_FIRST_TRIES + READY_SECOND_TRIES) {
        last = run("drutil", &["status", "-drive", &idx]).unwrap_or_default();
        if !last.contains("Becoming Ready") && !last.contains("not ready") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(READY_SLEEP_MS * 5));
    }
    let mut i = parse_drutil_status(&last);
    if i.present {
        // Speeds offered for this disc.
        if let Some(info) = run("drutil", &["info", "-drive", &idx]) {
            for l in info.lines() {
                if l.contains("Write Speeds") || l.contains("Speeds") {
                    if let Some((_, val)) = l.split_once(':') {
                        i.speeds = val
                            .split(',')
                            .filter_map(|s| s.trim().trim_end_matches('x').trim().parse::<f32>().ok())
                            .map(|f| f as u32)
                            .filter(|s| *s > 0)
                            .collect();
                    }
                }
            }
        }
        i.supported = Some(!i.is_rom());
    }
    i
}

fn ps_disc_script(id: &str) -> String {
    format!(
        r#"
$ErrorActionPreference='SilentlyContinue'
$r = New-Object -ComObject IMAPI2.MsftDiscRecorder2
$r.InitializeDiscRecorder('{id}')
$f = New-Object -ComObject IMAPI2.MsftDiscFormat2Data
$sup = $f.IsCurrentMediaSupported($r)
Write-Output ("SUPPORTED=" + $sup)
if ($sup) {{
  $f.Recorder = $r
  $f.ClientName = 'GravadorGuiado'
  Write-Output ("TYPE=" + [int]$f.CurrentPhysicalMediaType)
  Write-Output ("BLANK=" + $f.MediaHeuristicallyBlank)
  Write-Output ("STATUS=" + [int]$f.CurrentMediaStatus)
  Write-Output ("FREE=" + $f.FreeSectorsOnMedia)
  Write-Output ("TOTAL=" + $f.TotalSectorsOnMedia)
  $sp = @(); foreach ($s in $f.SupportedWriteSpeeds) {{ $sp += $s }}
  Write-Output ("SPEEDS=" + ($sp -join ','))
}} else {{
  $cur = $r.CurrentProfiles
  $c = @(); foreach ($p in $cur) {{ $c += $p }}
  Write-Output ("PROFILES=" + ($c -join ','))
}}
"#
    )
}

// IMAPI_MEDIA_PHYSICAL_TYPE values.
fn physical_type(code: u32) -> &'static str {
    match code {
        1 => "CD-ROM",
        2 => "CD-R",
        3 => "CD-RW",
        4 => "DVD-ROM",
        5 => "DVD-RAM",
        6 => "DVD+R",
        7 => "DVD+RW",
        8 => "DVD+R DL",
        9 => "DVD-R",
        10 => "DVD-RW",
        11 => "DVD-R DL",
        12 => "Disco",
        13 => "DVD+RW DL",
        14 => "HD DVD-ROM",
        15 => "HD DVD-R",
        16 => "HD DVD-RAM",
        17 => "BD-ROM",
        18 => "BD-R",
        19 => "BD-RE",
        _ => "Desconhecido",
    }
}

pub fn parse_windows_status(out: &str) -> DiscInfo {
    let mut i = DiscInfo::none(out.to_string());
    let get = |k: &str| out.lines().find_map(|l| l.trim().strip_prefix(k).map(|v| v.trim().to_string()));
    let supported = get("SUPPORTED=").map(|v| v.eq_ignore_ascii_case("true"));
    i.supported = supported;
    i.ready = true;
    if supported == Some(true) {
        i.present = true;
        i.media_type = physical_type(get("TYPE=").and_then(|v| v.parse().ok()).unwrap_or(0)).into();
        let blank = get("BLANK=").map(|v| v.eq_ignore_ascii_case("true")).unwrap_or(false);
        let status: u32 = get("STATUS=").and_then(|v| v.parse().ok()).unwrap_or(0);
        // IMAPI_FORMAT2_DATA_MEDIA_STATE flags.
        i.status = if blank || status & 0x2 != 0 {
            DiscStatus::Empty
        } else if status & 0x4 != 0 {
            DiscStatus::Incomplete
        } else if status & 0x4000 != 0 || status & 0x8000 != 0 {
            DiscStatus::Complete
        } else {
            DiscStatus::Others
        };
        i.erasable = Some(status & 0x1 != 0 || i.media_type.contains("RW"));
        i.free_sectors = get("FREE=").and_then(|v| v.parse().ok()).unwrap_or(0);
        i.total_sectors = get("TOTAL=").and_then(|v| v.parse().ok()).unwrap_or(0);
        // Speeds come as sectors/second; 1x CD = 75 sectors/s, 1x DVD = 675.
        let per_x = if i.media_type.starts_with("CD") { 75.0 } else { 675.0 };
        i.speeds = get("SPEEDS=")
            .unwrap_or_default()
            .split(',')
            .filter_map(|s| s.trim().parse::<f64>().ok())
            .map(|s| (s / per_x).round() as u32)
            .filter(|s| *s > 0)
            .collect();
        i.speeds.sort();
        i.speeds.dedup();
    } else if let Some(p) = get("PROFILES=") {
        let codes: Vec<u32> = p.split(',').filter_map(|c| c.trim().parse().ok()).collect();
        if let Some(name) = codes.iter().filter_map(|c| profile_name(*c)).next() {
            i.present = true;
            i.media_type = name.into();
            i.status = if name.contains("ROM") { DiscStatus::Complete } else { DiscStatus::Unknown };
        }
    }
    i
}

fn read_windows(id: &str) -> DiscInfo {
    let mut last = String::new();
    for _ in 0..READY_FIRST_TRIES {
        last = powershell(&ps_disc_script(id)).unwrap_or_default();
        if last.contains("SUPPORTED=") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(READY_SLEEP_MS * 5));
    }
    parse_windows_status(&last)
}

// ---------------------------------------------------------------- burning

#[derive(Clone, Debug)]
pub struct BurnSettings {
    pub speed: Option<u32>,
    pub verify: bool,
    pub test_mode: bool,
    pub eject: bool,
}

pub enum BurnEvent {
    Log(String),
    Progress(f32, String),
    Done(Result<String, String>),
}

pub fn burn(
    drive: Drive,
    image: std::path::PathBuf,
    sectors: u64,
    settings: BurnSettings,
    tx: std::sync::mpsc::Sender<BurnEvent>,
) {
    std::thread::spawn(move || {
        let r = match &drive.backend {
            Backend::Practice => burn_practice(sectors, &settings, &tx),
            Backend::MacOs { index } => burn_mac(*index, &image, &settings, &tx),
            Backend::Windows { recorder_id } => burn_windows(recorder_id, &image, &settings, &tx),
        };
        let _ = tx.send(BurnEvent::Done(r));
    });
}

fn burn_practice(sectors: u64, s: &BurnSettings, tx: &std::sync::mpsc::Sender<BurnEvent>) -> Result<String, String> {
    let steps = [
        ("Calibrando o laser no disco (OPC)...", 0.03),
        ("Gravando o início do disco (lead-in)...", 0.06),
    ];
    for (msg, p) in steps {
        let _ = tx.send(BurnEvent::Log(msg.into()));
        let _ = tx.send(BurnEvent::Progress(p, msg.into()));
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    let total = 60;
    for k in 0..=total {
        let f = k as f32 / total as f32;
        let written = (sectors as f32 * f) as u64;
        let _ = tx.send(BurnEvent::Progress(
            0.06 + f * if s.verify { 0.6 } else { 0.9 },
            format!("Gravando: {written} de {sectors} setores"),
        ));
        std::thread::sleep(std::time::Duration::from_millis(60));
    }
    let _ = tx.send(BurnEvent::Log("Fechando o disco (finalizando)...".into()));
    std::thread::sleep(std::time::Duration::from_millis(400));
    if s.verify {
        let _ = tx.send(BurnEvent::Log("Conferindo o que foi gravado (verificação)...".into()));
        for k in 0..=30 {
            let f = k as f32 / 30.0;
            let _ = tx.send(BurnEvent::Progress(0.66 + f * 0.34, format!("Conferindo: {:.0}%", f * 100.0)));
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
    Ok(if s.test_mode {
        "Simulação concluída (nada foi gravado, é só treino).".into()
    } else {
        "Treino concluído. Com um gravador de verdade, o disco estaria pronto.".into()
    })
}

fn burn_mac(index: usize, image: &std::path::Path, s: &BurnSettings, tx: &std::sync::mpsc::Sender<BurnEvent>) -> Result<String, String> {
    use std::io::{BufRead, BufReader};
    let mut args: Vec<String> = vec!["burn".into(), image.to_string_lossy().into(), "-puppetstrings".into()];
    args.push("-device".into());
    args.push(index.to_string());
    if let Some(sp) = s.speed {
        args.push("-speed".into());
        args.push(sp.to_string());
    }
    args.push(if s.verify { "-verifyburn".into() } else { "-noverifyburn".into() });
    if s.test_mode {
        args.push("-testburn".into());
    }
    args.push(if s.eject { "-eject".into() } else { "-noeject".into() });
    let _ = tx.send(BurnEvent::Log(format!("hdiutil {}", args.join(" "))));
    let mut child = quiet("hdiutil")
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Não foi possível iniciar a gravação: {e}"))?;
    let out = child.stdout.take().unwrap();
    let mut failed = None;
    for line in BufReader::new(out).lines().map_while(Result::ok) {
        if let Some(p) = line.strip_prefix("PERCENT:") {
            if let Ok(v) = p.trim().parse::<f32>() {
                if v >= 0.0 {
                    let _ = tx.send(BurnEvent::Progress(v / 100.0, format!("{v:.0}%")));
                }
            }
        } else if let Some(m) = line.strip_prefix("MESSAGE:") {
            let _ = tx.send(BurnEvent::Log(m.trim().to_string()));
        } else if line.contains("ERROR") || line.contains("failed") {
            failed = Some(line.clone());
            let _ = tx.send(BurnEvent::Log(line));
        } else if !line.trim().is_empty() {
            let _ = tx.send(BurnEvent::Log(line));
        }
    }
    let mut err = String::new();
    if let Some(mut e) = child.stderr.take() {
        use std::io::Read;
        let _ = e.read_to_string(&mut err);
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    if status.success() && failed.is_none() {
        Ok("Gravação concluída com sucesso.".into())
    } else {
        Err(failed.unwrap_or_else(|| if err.trim().is_empty() { format!("O gravador parou com erro ({status}).") } else { err.trim().to_string() }))
    }
}

fn burn_windows(id: &str, image: &std::path::Path, s: &BurnSettings, tx: &std::sync::mpsc::Sender<BurnEvent>) -> Result<String, String> {
    let path = image.to_string_lossy().replace('\'', "''");
    let speed = s.speed.map(|x| x as i64).unwrap_or(-1);
    let script = format!(
        r#"
$ErrorActionPreference='Stop'
Add-Type -TypeDefinition @'
using System; using System.Runtime.InteropServices; using System.Runtime.InteropServices.ComTypes;
public static class GgStream {{
  [DllImport("shlwapi.dll", CharSet=CharSet.Unicode, ExactSpelling=true, PreserveSig=false)]
  static extern void SHCreateStreamOnFileEx(string f, uint m, uint a, bool c, IStream t, out IStream s);
  public static IStream Open(string f) {{ IStream s; SHCreateStreamOnFileEx(f, 0x20, 0, false, null, out s); return s; }}
}}
'@
try {{
  $r = New-Object -ComObject IMAPI2.MsftDiscRecorder2
  $r.InitializeDiscRecorder('{id}')
  $f = New-Object -ComObject IMAPI2.MsftDiscFormat2Data
  $f.Recorder = $r
  $f.ClientName = 'GravadorGuiado'
  $f.ForceMediaToBeClosed = $true
  if ({speed} -gt 0) {{ $per = 75; if ($f.CurrentPhysicalMediaType -ge 4) {{ $per = 675 }}; $f.SetWriteSpeed({speed} * $per, $false) }}
  Write-Output 'MESSAGE:Gravando a imagem...'
  $st = [GgStream]::Open('{path}')
  $f.Write($st)
  Write-Output 'MESSAGE:Gravação terminada.'
  if ({eject}) {{ $r.EjectMedia() }}
  Write-Output 'RESULT:OK'
}} catch {{ Write-Output ('RESULT:ERRO ' + $_.Exception.Message) }}
"#,
        eject = if s.eject { "$true" } else { "$false" }
    );
    let _ = tx.send(BurnEvent::Progress(0.05, "Gravando (o Windows não informa o andamento; aguarde)...".into()));
    let out = powershell(&script).unwrap_or_default();
    for l in out.lines() {
        if let Some(m) = l.strip_prefix("MESSAGE:") {
            let _ = tx.send(BurnEvent::Log(m.into()));
        }
    }
    if out.contains("RESULT:OK") {
        let _ = tx.send(BurnEvent::Progress(1.0, "100%".into()));
        Ok("Gravação concluída.".into())
    } else {
        Err(out
            .lines()
            .find_map(|l| l.strip_prefix("RESULT:ERRO "))
            .map(str::to_string)
            .unwrap_or_else(|| "O Windows não conseguiu gravar o disco.".into()))
    }
}

// ---------------------------------------------------------------- burning a picture track (audio CD)

/// Burns the drawing track (`wav`, with its folder) as a one-track audio CD.
pub fn burn_audio(drive: Drive, wav: std::path::PathBuf, sectors: u64, settings: BurnSettings, tx: std::sync::mpsc::Sender<BurnEvent>) {
    std::thread::spawn(move || {
        let r = match &drive.backend {
            Backend::Practice => burn_practice(sectors, &BurnSettings { verify: false, ..settings.clone() }, &tx),
            Backend::MacOs { index } => burn_audio_mac(*index, &wav, &settings, &tx),
            Backend::Windows { recorder_id } => burn_audio_windows(recorder_id, &wav, &settings, &tx),
        };
        let _ = tx.send(BurnEvent::Done(r));
    });
}

fn burn_audio_mac(index: usize, wav: &std::path::Path, s: &BurnSettings, tx: &std::sync::mpsc::Sender<BurnEvent>) -> Result<String, String> {
    use std::io::{BufRead, BufReader};
    let folder = wav.parent().ok_or("Pasta da faixa não encontrada.")?;
    let mut args: Vec<String> = vec!["-drive".into(), index.to_string(), "burn".into(), "-audio".into(), "-noverify".into()];
    if let Some(sp) = s.speed {
        args.push("-speed".into());
        args.push(sp.to_string());
    }
    if s.test_mode {
        args.push("-test".into());
    }
    args.push(if s.eject { "-eject".into() } else { "-noeject".into() });
    args.push(folder.to_string_lossy().into());
    let _ = tx.send(BurnEvent::Log(format!("drutil {}", args.join(" "))));
    let mut child = quiet("drutil")
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Não foi possível iniciar a gravação: {e}"))?;
    let out = child.stdout.take().unwrap();
    let mut failed = None;
    // drutil redraws its progress line with carriage returns.
    for line in BufReader::new(out).split(b'\r').map_while(Result::ok) {
        let text = String::from_utf8_lossy(&line).trim().to_string();
        if text.is_empty() {
            continue;
        }
        if let Some(pos) = text.find('%') {
            let digits: String = text[..pos].chars().rev().take_while(|c| c.is_ascii_digit() || *c == '.').collect::<Vec<_>>().into_iter().rev().collect();
            if let Ok(v) = digits.parse::<f32>() {
                let _ = tx.send(BurnEvent::Progress(v / 100.0, text.clone()));
                continue;
            }
        }
        if text.to_lowercase().contains("error") || text.to_lowercase().contains("fail") {
            failed = Some(text.clone());
        }
        let _ = tx.send(BurnEvent::Log(text));
    }
    let mut err = String::new();
    if let Some(mut e) = child.stderr.take() {
        use std::io::Read;
        let _ = e.read_to_string(&mut err);
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    if status.success() && failed.is_none() {
        Ok("Desenho gravado no disco.".into())
    } else {
        Err(failed.unwrap_or_else(|| if err.trim().is_empty() { format!("O gravador parou com erro ({status}).") } else { err.trim().to_string() }))
    }
}

fn burn_audio_windows(id: &str, wav: &std::path::Path, s: &BurnSettings, tx: &std::sync::mpsc::Sender<BurnEvent>) -> Result<String, String> {
    // IMAPI wants raw samples without the 44-byte WAV header.
    let pcm = wav.with_extension("pcm");
    let _ = tx.send(BurnEvent::Progress(0.02, "Preparando a faixa para o Windows...".into()));
    {
        use std::io::{Read, Seek, SeekFrom};
        let mut src = std::fs::File::open(wav).map_err(|e| e.to_string())?;
        src.seek(SeekFrom::Start(44)).map_err(|e| e.to_string())?;
        let mut dst = std::fs::File::create(&pcm).map_err(|e| e.to_string())?;
        let mut buf = vec![0u8; 1 << 20];
        loop {
            let n = src.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            std::io::Write::write_all(&mut dst, &buf[..n]).map_err(|e| e.to_string())?;
        }
    }
    let path = pcm.to_string_lossy().replace('\'', "''");
    let speed = s.speed.map(|x| x as i64).unwrap_or(-1);
    if s.speed.is_none() {
        let _ = tx.send(BurnEvent::Log(format!("Velocidade MAX (código {} para CD, como no ImgBurn).", crate::mechanics::CD_MAX_KBS_CODE)));
    }
    let script = format!(
        r#"
$ErrorActionPreference='Stop'
Add-Type -TypeDefinition @'
using System; using System.Runtime.InteropServices; using System.Runtime.InteropServices.ComTypes;
public static class GgStream {{
  [DllImport("shlwapi.dll", CharSet=CharSet.Unicode, ExactSpelling=true, PreserveSig=false)]
  static extern void SHCreateStreamOnFileEx(string f, uint m, uint a, bool c, IStream t, out IStream s);
  public static IStream Open(string f) {{ IStream s; SHCreateStreamOnFileEx(f, 0x20, 0, false, null, out s); return s; }}
}}
'@
try {{
  $r = New-Object -ComObject IMAPI2.MsftDiscRecorder2
  $r.InitializeDiscRecorder('{id}')
  $t = New-Object -ComObject IMAPI2.MsftDiscFormat2TrackAtOnce
  $t.Recorder = $r
  $t.ClientName = 'GravadorGuiado'
  if ({speed} -gt 0) {{ $t.SetWriteSpeed({speed} * 75, $false) }}
  $t.PrepareMedia()
  Write-Output 'MESSAGE:Gravando o desenho...'
  $st = [GgStream]::Open('{path}')
  $t.AddAudioTrack($st)
  $t.ReleaseMedia()
  if ({eject}) {{ $r.EjectMedia() }}
  Write-Output 'RESULT:OK'
}} catch {{ Write-Output ('RESULT:ERRO ' + $_.Exception.Message) }}
"#,
        eject = if s.eject { "$true" } else { "$false" }
    );
    let _ = tx.send(BurnEvent::Progress(0.05, "Gravando (o Windows não informa o andamento; aguarde)...".into()));
    let out = powershell(&script).unwrap_or_default();
    let _ = std::fs::remove_file(&pcm);
    if out.contains("RESULT:OK") {
        let _ = tx.send(BurnEvent::Progress(1.0, "100%".into()));
        Ok("Desenho gravado no disco.".into())
    } else {
        Err(out.lines().find_map(|l| l.strip_prefix("RESULT:ERRO ")).map(str::to_string).unwrap_or_else(|| "O Windows não conseguiu gravar o disco.".into()))
    }
}

/// Plays the system's "done" or "error" sound (ImgBurn plays one after writing by default).
pub fn play_sound(success: bool) {
    std::thread::spawn(move || {
        if cfg!(target_os = "macos") {
            let f = if success { "/System/Library/Sounds/Glass.aiff" } else { "/System/Library/Sounds/Basso.aiff" };
            let _ = quiet("afplay").arg(f).status();
        } else if cfg!(target_os = "windows") {
            let s = if success { "Asterisk" } else { "Hand" };
            let _ = quiet("powershell").args(["-NoProfile", "-Command", &format!("[System.Media.SystemSounds]::{s}.Play(); Start-Sleep -Milliseconds 800")]).status();
        }
    });
}
