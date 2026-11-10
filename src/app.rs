//! The guided window: six numbered steps, each with plain instructions,
//! and a compatibility check before anything is burned.

use crate::checks::{self, Check, Level};
use crate::drive::{self, BurnEvent, BurnSettings, DiscInfo, Drive, PracticeDisc};
use crate::image::{self, human_size, ImageInfo};
use crate::mechanics::*;
use crate::picture::{self, Adjust, GenEvent, Picture, TrackFiles, PRESETS};
use egui::{Color32, RichText, Rounding, Stroke, Vec2};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};

const ACCENT: Color32 = Color32::from_rgb(0x1f, 0x5f, 0xbf);
const ACCENT_SOFT: Color32 = Color32::from_rgb(0xe3, 0xed, 0xfb);
const OK: Color32 = Color32::from_rgb(0x1e, 0x8e, 0x3e);
const WARN: Color32 = Color32::from_rgb(0xc7, 0x7c, 0x02);
const FAIL: Color32 = Color32::from_rgb(0xc6, 0x28, 0x28);
const MUTED: Color32 = Color32::from_rgb(0x5f, 0x66, 0x70);
const CARD: Color32 = Color32::from_rgb(0xff, 0xff, 0xff);
const BG: Color32 = Color32::from_rgb(0xf3, 0xf5, 0xf8);

const STEPS: [&str; 6] = ["Escolher", "Disco", "Verificação", "Opções", "Gravar", "Pronto"];

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Iso,
    Picture,
}

pub struct GuidedBurner {
    step: usize,
    mode: Mode,
    picture: Option<Picture>,
    picture_error: Option<String>,
    adjust: Adjust,
    preset_idx: usize,
    preview_tex: Option<egui::TextureHandle>,
    preview_dirty: bool,
    gen_rx: Option<Receiver<GenEvent>>,
    track: Option<TrackFiles>,
    out_folder: Option<PathBuf>,
    save_only: bool,
    image: Option<ImageInfo>,
    drives: Vec<Drive>,
    drive_idx: usize,
    practice_disc: PracticeDisc,
    media_idx: usize,
    disc: Option<DiscInfo>,
    reading: Option<Receiver<DiscInfo>>,
    checks: Vec<Check>,
    speed: Option<u32>,
    verify: bool,
    test_mode: bool,
    eject: bool,
    sounds: bool,
    erase_ok: bool,
    burn_rx: Option<Receiver<BurnEvent>>,
    progress: f32,
    progress_text: String,
    result: Option<Result<String, String>>,
    log: Vec<String>,
    show_log: bool,
    screenshot: Option<PathBuf>,
    frames: u32,
    shot_requested: bool,
}

impl GuidedBurner {
    pub fn new(cc: &eframe::CreationContext<'_>, screenshot: Option<PathBuf>, demo_step: Option<usize>, demo_picture: bool) -> Self {
        setup_style(&cc.egui_ctx);
        let drives = drive::find_drives();
        let mut s = GuidedBurner {
            step: 0,
            mode: Mode::Iso,
            picture: None,
            picture_error: None,
            adjust: Adjust::default(),
            preset_idx: 0,
            preview_tex: None,
            preview_dirty: true,
            gen_rx: None,
            track: None,
            out_folder: None,
            save_only: false,
            image: None,
            drive_idx: 0,
            drives,
            practice_disc: PracticeDisc::BlankCdR,
            media_idx: 0,
            disc: None,
            reading: None,
            checks: vec![],
            speed: None,
            verify: VERIFY_DEFAULT,
            test_mode: TEST_MODE_DEFAULT,
            eject: EJECT_AFTER_WRITE_DEFAULT,
            sounds: SUCCESS_SOUND_DEFAULT && ERROR_SOUND_DEFAULT,
            erase_ok: false,
            burn_rx: None,
            progress: 0.0,
            progress_text: String::new(),
            result: None,
            log: vec![],
            show_log: false,
            screenshot,
            frames: 0,
            shot_requested: false,
        };
        let real = s.drives.iter().filter(|d| d.backend != drive::Backend::Practice).count();
        s.log(format!("Programa aberto. Gravadores encontrados: {real}."));
        if real == 0 {
            s.log("Nenhum gravador de verdade foi encontrado; o gravador de treino está disponível.".into());
        }
        if demo_picture {
            s.mode = Mode::Picture;
            s.picture = Some(Picture::sample());
        }
        if let Some(step) = demo_step {
            s.load_demo(step.min(5));
        }
        s
    }

    fn log(&mut self, m: String) {
        self.log.push(m);
    }

    /// Fills in a sample image and the practice disc, to preview later steps.
    fn load_demo(&mut self, step: usize) {
        self.image = Some(ImageInfo {
            path: PathBuf::from("exemplo.iso"),
            burn_path: PathBuf::from("exemplo.iso"),
            kind: image::ImageKind::Iso,
            bytes: 612 * 1024 * 1024,
            sectors: 612 * 512,
            volume_label: Some("EXEMPLO".into()),
            file_system: Some("ISO 9660".into()),
            problems: vec![],
            notes: vec![],
        });
        self.drive_idx = self.drives.len() - 1;
        let d = drive::practice_disc(PracticeDisc::BlankCdR);
        self.speed = checks::recommended_speed(&d);
        self.disc = Some(d);
        self.run_checks();
        self.step = step;
    }

    fn pick_picture(&mut self, path: PathBuf) {
        match Picture::load(&path) {
            Ok(mut p) => {
                p.apply(&self.adjust);
                self.log(format!("Foto escolhida: {} ({}x{}).", path.display(), p.width, p.height));
                self.picture = Some(p);
                self.picture_error = None;
                self.out_folder = None;
            }
            Err(e) => {
                self.log(format!("Problema: {e}"));
                self.picture = None;
                self.picture_error = Some(e);
            }
        }
        self.preview_dirty = true;
        self.checks.clear();
    }

    fn pick_any(&mut self, path: PathBuf) {
        let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        if picture::is_photo_ext(&ext) {
            self.mode = Mode::Picture;
            self.pick_picture(path);
        } else {
            self.mode = Mode::Iso;
            self.pick_image(path);
        }
    }

    fn pick_image(&mut self, path: PathBuf) {
        let info = image::inspect(&path);
        self.log(format!("Imagem escolhida: {} ({}).", path.display(), human_size(info.bytes)));
        for p in &info.problems {
            self.log(format!("Problema: {p}"));
        }
        self.image = Some(info);
        self.checks.clear();
    }

    fn start_reading(&mut self) {
        let Some(d) = self.drives.get(self.drive_idx).cloned() else { return };
        let practice = self.practice_disc;
        let (tx, rx) = channel();
        self.log(format!("Lendo o disco no gravador: {}...", d.name));
        std::thread::spawn(move || {
            let _ = tx.send(drive::read_disc(&d, practice));
        });
        self.reading = Some(rx);
        self.disc = None;
        self.checks.clear();
    }

    fn run_checks(&mut self) {
        if self.mode == Mode::Picture {
            if let (Some(disc), Some(d)) = (&self.disc, self.drives.get(self.drive_idx)) {
                self.checks = checks::run_picture(self.picture.is_some(), &PRESETS[self.preset_idx], self.preset_idx >= 2, d, disc);
                let extra = checks::brand(&crate::media::PROFILES[self.media_idx], disc, true);
                self.checks.splice(1..1, extra);
                let w = checks::worst(&self.checks);
                self.log(format!("Verificação do desenho: {}.", match w { Level::Ok => "tudo certo", Level::Warn => "pode gravar, com avisos", Level::Fail => "NÃO pode gravar" }));
            }
            return;
        }
        if let (Some(img), Some(disc), Some(d)) = (&self.image, &self.disc, self.drives.get(self.drive_idx)) {
            self.checks = checks::run(img, d, disc);
            let extra = checks::brand(&crate::media::PROFILES[self.media_idx], disc, false);
            let at = self.checks.len().min(1);
            self.checks.splice(at..at, extra);
            let w = checks::worst(&self.checks);
            self.log(format!(
                "Verificação de compatibilidade: {}.",
                match w {
                    Level::Ok => "tudo certo",
                    Level::Warn => "pode gravar, com avisos",
                    Level::Fail => "NÃO pode gravar",
                }
            ));
        }
    }

    fn picture_sectors(&self) -> u64 {
        picture::sectors_for(&PRESETS[self.preset_idx], self.disc.as_ref().map(|d| d.free_sectors).unwrap_or(0))
    }

    fn default_out_folder(&self) -> PathBuf {
        let base = self.picture.as_ref().and_then(|p| p.path.parent().map(PathBuf::from)).filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(std::env::temp_dir);
        let stem = self.picture.as_ref().and_then(|p| p.path.file_stem().map(|s| s.to_string_lossy().to_string())).unwrap_or_else(|| "foto".into());
        base.join(format!("{stem} - desenho no CD"))
    }

    fn start_picture(&mut self, save_only: bool) {
        let Some(bits) = self.picture.as_ref().map(|p| p.bits.clone()) else { return };
        let is_practice = self.drives.get(self.drive_idx).map(|d| d.backend == drive::Backend::Practice).unwrap_or(true);
        self.save_only = save_only;
        self.result = None;
        self.progress = 0.0;
        if is_practice && !save_only {
            // Practice: no 850 MB file, just walk through the burn.
            self.track = None;
            self.start_audio_burn(PathBuf::new());
            return;
        }
        let folder = self.out_folder.clone().unwrap_or_else(|| self.default_out_folder());
        let sectors = self.picture_sectors();
        self.log(format!("Criando a faixa do desenho ({sectors} setores, cerca de {:.0} MB) em {}...", (sectors * picture::CD_SECTOR) as f64 / 1048576.0, folder.display()));
        let (tx, rx) = channel();
        picture::generate(bits, PRESETS[self.preset_idx].clone(), sectors, folder, tx);
        self.gen_rx = Some(rx);
        self.progress_text = "Transformando a foto em trilha do disco...".into();
    }

    fn start_audio_burn(&mut self, wav: PathBuf) {
        let Some(d) = self.drives.get(self.drive_idx).cloned() else { return };
        let (tx, rx) = channel();
        let settings = BurnSettings { speed: self.speed, verify: false, test_mode: self.test_mode, eject: self.eject };
        self.log(format!("Gravação do desenho iniciada: velocidade {}.", self.speed.map(|s| format!("{s}x")).unwrap_or_else(|| "automática".into())));
        drive::burn_audio(d, wav, self.picture_sectors(), settings, tx);
        self.burn_rx = Some(rx);
        self.progress = 0.0;
        self.progress_text = "Preparando o gravador...".into();
    }

    fn start_burn(&mut self) {
        let (Some(img), Some(d)) = (self.image.clone(), self.drives.get(self.drive_idx).cloned()) else { return };
        let (tx, rx) = channel();
        let settings = BurnSettings { speed: self.speed, verify: self.verify, test_mode: self.test_mode, eject: self.eject };
        self.log(format!(
            "Gravação iniciada: velocidade {}, conferência {}, {}.",
            self.speed.map(|s| format!("{s}x")).unwrap_or_else(|| "automática".into()),
            if self.verify { "ligada" } else { "desligada" },
            if self.test_mode { "modo de teste (simulação)" } else { "gravação real" }
        ));
        drive::burn(d, img.burn_path.clone(), img.sectors, settings, tx);
        self.burn_rx = Some(rx);
        self.progress = 0.0;
        self.progress_text = "Preparando o gravador...".into();
        self.result = None;
    }

    fn poll(&mut self) {
        if let Some(rx) = &self.reading {
            if let Ok(info) = rx.try_recv() {
                self.reading = None;
                let msg = if info.present {
                    format!("Disco lido: {} — {} — {} setores livres.", info.media_type, info.status.label(), info.free_sectors)
                } else if info.ready {
                    "Nenhum disco na bandeja.".to_string()
                } else {
                    "O gravador não respondeu.".to_string()
                };
                self.log(msg);
                self.speed = checks::recommended_speed(&info);
                self.disc = Some(info);
                self.run_checks();
            }
        }
        let mut gen_events = vec![];
        if let Some(rx) = &self.gen_rx {
            while let Ok(e) = rx.try_recv() {
                gen_events.push(e);
            }
        }
        for e in gen_events {
            match e {
                GenEvent::Progress(f) => {
                    self.progress = f * if self.save_only { 1.0 } else { 0.999 };
                    self.progress_text = format!("Transformando a foto em trilha do disco: {:.0}%", f * 100.0);
                }
                GenEvent::Done(Ok(t)) => {
                    self.gen_rx = None;
                    self.log(format!("Faixa criada: {}", t.wav.display()));
                    self.track = Some(t.clone());
                    if self.save_only {
                        self.result = Some(Ok(format!("Arquivos salvos na pasta: {}", t.folder.display())));
                        self.step = 5;
                    } else {
                        self.start_audio_burn(t.wav.clone());
                    }
                }
                GenEvent::Done(Err(e)) => {
                    if self.sounds {
                        drive::play_sound(false);
                    }
                    self.gen_rx = None;
                    self.log(format!("FALHOU: {e}"));
                    self.result = Some(Err(e));
                    self.step = 5;
                }
            }
        }
        let mut events = vec![];
        if let Some(rx) = &self.burn_rx {
            while let Ok(e) = rx.try_recv() {
                events.push(e);
            }
        }
        for e in events {
            match e {
                BurnEvent::Log(m) => self.log(m),
                BurnEvent::Progress(p, t) => {
                    self.progress = p.clamp(0.0, 1.0);
                    self.progress_text = t;
                }
                BurnEvent::Done(r) => {
                    if self.sounds {
                        drive::play_sound(r.is_ok());
                    }
                    match &r {
                        Ok(m) => self.log(format!("Terminado: {m}")),
                        Err(m) => self.log(format!("FALHOU: {m}")),
                    }
                    self.result = Some(r);
                    self.burn_rx = None;
                    self.step = 5;
                }
            }
        }
    }

    fn can_go_next(&self) -> (bool, &'static str) {
        match self.step {
            0 if self.mode == Mode::Picture => {
                if self.picture.is_some() { (true, "") } else { (false, "Escolha uma foto (JPG ou PNG) para continuar.") }
            }
            0 => match &self.image {
                Some(i) if i.usable() => (true, ""),
                Some(_) => (false, "Escolha um arquivo de imagem válido para continuar."),
                None => (false, "Escolha o arquivo de imagem para continuar."),
            },
            1 => {
                if self.reading.is_some() {
                    (false, "Aguarde a leitura do disco terminar.")
                } else if self.disc.is_none() {
                    (false, "Clique em \"Ler o disco\" para continuar.")
                } else {
                    (true, "")
                }
            }
            2 => {
                if checks::worst(&self.checks) == Level::Fail {
                    (false, "Resolva os itens em vermelho para poder gravar.")
                } else if self.needs_erase_ok() && !self.erase_ok {
                    (false, "Confirme que o disco regravável pode ser apagado.")
                } else {
                    (true, "")
                }
            }
            3 => (true, ""),
            _ => (false, ""),
        }
    }

    fn needs_erase_ok(&self) -> bool {
        self.disc.as_ref().map(|d| d.is_rewritable() && d.status != drive::DiscStatus::Empty).unwrap_or(false)
    }

    // ------------------------------------------------------------------ drawing

    fn header(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(Vec2::splat(34.0), egui::Sense::hover());
            let p = ui.painter();
            p.circle_filled(r.center(), 16.0, Color32::WHITE);
            p.circle_filled(r.center(), 11.0, ACCENT_SOFT);
            p.circle_filled(r.center(), 4.0, ACCENT);
            ui.add_space(6.0);
            ui.vertical(|ui| {
                ui.label(RichText::new("Gravador de Imagens").size(20.0).strong().color(Color32::WHITE));
                ui.label(RichText::new("Grave uma imagem de disco (.iso) ou desenhe uma foto num CD-R, um passo de cada vez").size(13.0).color(Color32::from_rgb(0xd6, 0xe4, 0xf7)));
            });
        });
    }

    fn step_bar(&self, ui: &mut egui::Ui) {
        let w = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 64.0), egui::Sense::hover());
        let p = ui.painter();
        let n = STEPS.len() as f32;
        let seg = rect.width() / n;
        for (k, name) in STEPS.iter().enumerate() {
            let cx = rect.left() + seg * (k as f32 + 0.5);
            let c = egui::pos2(cx, rect.top() + 20.0);
            if k + 1 < STEPS.len() {
                let col = if k < self.step { ACCENT } else { Color32::from_gray(205) };
                p.line_segment([egui::pos2(cx + 18.0, c.y), egui::pos2(cx + seg - 18.0, c.y)], Stroke::new(3.0, col));
            }
            let (fill, text_col) = if k < self.step {
                (ACCENT, Color32::WHITE)
            } else if k == self.step {
                (ACCENT, Color32::WHITE)
            } else {
                (Color32::from_gray(225), MUTED)
            };
            if k == self.step {
                p.circle_filled(c, 19.0, ACCENT_SOFT);
            }
            p.circle_filled(c, 15.0, fill);
            let label = if k < self.step { "✔".to_string() } else { (k + 1).to_string() };
            p.text(c, egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(15.0), text_col);
            p.text(
                egui::pos2(cx, rect.top() + 50.0),
                egui::Align2::CENTER_CENTER,
                *name,
                egui::FontId::proportional(if k == self.step { 15.0 } else { 14.0 }),
                if k == self.step { ACCENT } else { MUTED },
            );
        }
    }

    fn instructions(ui: &mut egui::Ui, items: &[&str]) {
        egui::Frame::none().fill(ACCENT_SOFT).rounding(Rounding::same(8.0)).inner_margin(egui::Margin::same(12.0)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("O que fazer agora").strong().color(ACCENT).size(15.0));
            ui.add_space(4.0);
            for (k, t) in items.iter().enumerate() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(format!("{}.", k + 1)).strong().color(ACCENT));
                    ui.label(RichText::new(*t).size(15.0));
                });
            }
        });
    }

    fn info_row(ui: &mut egui::Ui, k: &str, v: &str) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(k).color(MUTED));
            ui.label(RichText::new(v).strong());
        });
    }

    fn check_row(ui: &mut egui::Ui, c: &Check) {
        let (icon, col, bg) = match c.level {
            Level::Ok => ("✔", OK, Color32::from_rgb(0xe8, 0xf5, 0xec)),
            Level::Warn => ("!", WARN, Color32::from_rgb(0xfd, 0xf4, 0xe1)),
            Level::Fail => ("✖", FAIL, Color32::from_rgb(0xfc, 0xe9, 0xe9)),
        };
        egui::Frame::none().fill(bg).rounding(Rounding::same(8.0)).inner_margin(egui::Margin::same(10.0)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(Vec2::splat(26.0), egui::Sense::hover());
                ui.painter().circle_filled(r.center(), 12.0, col);
                ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(14.0), Color32::WHITE);
                ui.vertical(|ui| {
                    ui.label(RichText::new(&c.title).strong().size(15.0).color(col));
                    ui.label(RichText::new(&c.detail).size(14.0));
                    if !c.todo.is_empty() {
                        ui.label(RichText::new(format!("▶ {}", c.todo)).size(14.0).strong());
                    }
                });
            });
        });
        ui.add_space(6.0);
    }

    fn mode_card(ui: &mut egui::Ui, selected: bool, title: &str, lines: &[&str]) -> bool {
        let fill = if selected { ACCENT_SOFT } else { CARD };
        let stroke = if selected { Stroke::new(2.0, ACCENT) } else { Stroke::new(1.0, Color32::from_gray(205)) };
        let r = egui::Frame::none().fill(fill).stroke(stroke).rounding(Rounding::same(10.0)).inner_margin(egui::Margin::same(14.0)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (c, _) = ui.allocate_exact_size(Vec2::splat(22.0), egui::Sense::hover());
                ui.painter().circle_stroke(c.center(), 9.0, Stroke::new(2.0, if selected { ACCENT } else { MUTED }));
                if selected {
                    ui.painter().circle_filled(c.center(), 5.0, ACCENT);
                }
                ui.label(RichText::new(title).size(16.0).strong().color(if selected { ACCENT } else { Color32::BLACK }));
            });
            for l in lines {
                ui.label(RichText::new(*l).color(MUTED));
            }
        });
        r.response.interact(egui::Sense::click()).clicked()
    }

    fn step_choose(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("O que você quer gravar?").size(17.0).strong());
        ui.add_space(4.0);
        let mut new_mode = self.mode;
        ui.columns(2, |cols| {
            if Self::mode_card(&mut cols[0], self.mode == Mode::Iso, "Uma imagem de disco (.iso)", &[
                "Para copiar um CD/DVD: programas, jogos, Windows, Linux.",
                "O arquivo .iso vira os dados do disco. Não é uma foto.",
            ]) {
                new_mode = Mode::Iso;
            }
            if Self::mode_card(&mut cols[1], self.mode == Mode::Picture, "Uma foto desenhada no CD (JPG/PNG)", &[
                "A foto aparece no lado gravado do CD-R, visível a olho nu.",
                "O disco não terá arquivos: só o desenho.",
            ]) {
                new_mode = Mode::Picture;
            }
        });
        if new_mode != self.mode {
            self.mode = new_mode;
            self.checks.clear();
        }
        ui.add_space(12.0);
        match self.mode {
            Mode::Iso => self.step_image(ui),
            Mode::Picture => self.step_picture(ui),
        }
    }

    fn step_picture(&mut self, ui: &mut egui::Ui) {
        Self::instructions(
            ui,
            &[
                "Clique em \"Escolher foto\" (ou arraste a foto para esta janela). Aceita JPG, PNG, BMP, GIF e WEBP.",
                "Veja à direita como o desenho deve ficar no disco. Ajuste o brilho e o contraste até gostar.",
                "Fotos com rosto ou objeto grande no meio e fundo simples ficam melhores.",
                "Clique em \"Próximo\".",
            ],
        );
        ui.add_space(10.0);
        if ui.add(egui::Button::new(RichText::new("Escolher foto").size(16.0)).min_size(Vec2::new(220.0, 40.0))).clicked() {
            if let Some(p) = rfd::FileDialog::new()
                .set_title("Escolha a foto para desenhar no CD")
                .add_filter("Fotos", &["jpg", "jpeg", "png", "bmp", "gif", "webp"])
                .pick_file()
            {
                self.pick_picture(p);
            }
        }
        if let Some(e) = &self.picture_error {
            ui.label(RichText::new(format!("✖ {e}")).color(FAIL));
        }
        ui.add_space(10.0);
        if self.picture.is_none() {
            egui::Frame::none().stroke(Stroke::new(1.5, Color32::from_gray(200))).rounding(Rounding::same(10.0)).inner_margin(egui::Margin::same(26.0)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("Nenhuma foto escolhida ainda").size(16.0).color(MUTED));
                    ui.label(RichText::new("O desenho sai em dois tons (claro e escuro), como uma foto de jornal.").color(MUTED));
                });
            });
            return;
        }
        let mut changed = false;
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(300.0);
                egui::Frame::none().fill(CARD).stroke(Stroke::new(1.0, Color32::from_gray(210))).rounding(Rounding::same(10.0)).inner_margin(egui::Margin::same(14.0)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("✔  Foto pronta").size(17.0).strong().color(OK));
                    if let Some(p) = &self.picture {
                        Self::info_row(ui, "Arquivo:", &p.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "exemplo".into()));
                        Self::info_row(ui, "Tamanho:", &format!("{} x {} pixels", p.width, p.height));
                    }
                    ui.label(RichText::new("Fotos que não são quadradas são cortadas no meio.").color(MUTED));
                    ui.add_space(8.0);
                    ui.label(RichText::new("Brilho").strong());
                    changed |= ui.add(egui::Slider::new(&mut self.adjust.brightness, -0.4..=0.4).show_value(false)).changed();
                    ui.label(RichText::new("Contraste").strong());
                    changed |= ui.add(egui::Slider::new(&mut self.adjust.contrast, 0.5..=2.5).show_value(false)).changed();
                    changed |= ui.checkbox(&mut self.adjust.dither, "Pontilhado (melhor para fotos)").changed();
                    changed |= ui.checkbox(&mut self.adjust.invert, "Inverter claro e escuro").changed();
                    changed |= ui.checkbox(&mut self.adjust.mirror, "Espelhar (virar de lado)").changed();
                    if ui.button("Voltar ao normal").clicked() {
                        self.adjust = Adjust::default();
                        changed = true;
                    }
                });
            });
            ui.add_space(12.0);
            ui.vertical(|ui| {
                ui.label(RichText::new("Como deve ficar o lado gravado do CD").strong());
                if let Some(t) = &self.preview_tex {
                    let side = ui.available_width().min(380.0).max(220.0);
                    ui.add(egui::Image::new((t.id(), Vec2::splat(side))));
                }
                ui.label(RichText::new("Na vida real o contraste é bem mais fraco; veja o disco inclinado contra a luz.").color(MUTED).size(13.0));
            });
        });
        if changed {
            if let Some(p) = &mut self.picture {
                p.apply(&self.adjust);
            }
            self.preview_dirty = true;
        }
    }

    fn step_image(&mut self, ui: &mut egui::Ui) {
        Self::instructions(
            ui,
            &[
                "Clique em \"Escolher arquivo de imagem\" (ou arraste o arquivo para esta janela).",
                "Escolha um arquivo .iso, .img ou o .cue de uma imagem BIN/CUE.",
                "Confira abaixo se aparece \"Imagem pronta\" em verde e clique em \"Próximo\".",
            ],
        );
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if ui.add(egui::Button::new(RichText::new("Escolher arquivo de imagem").size(16.0)).min_size(Vec2::new(280.0, 40.0))).clicked() {
                if let Some(p) = rfd::FileDialog::new()
                    .set_title("Escolha a imagem de disco")
                    .add_filter("Imagens de disco", &["iso", "img", "cue", "bin", "nrg", "mdf", "mds", "udf", "ccd", "cdi"])
                    .add_filter("Todos os arquivos", &["*"])
                    .pick_file()
                {
                    self.pick_any(p);
                }
            }
        });
        ui.add_space(12.0);
        match &self.image {
            None => {
                egui::Frame::none().stroke(Stroke::new(1.5, Color32::from_gray(200))).rounding(Rounding::same(10.0)).inner_margin(egui::Margin::same(26.0)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("Nenhuma imagem escolhida ainda").size(16.0).color(MUTED));
                        ui.label(RichText::new("Dica: um arquivo .zip ou .rar não é imagem. Extraia-o primeiro e escolha o .iso de dentro.").color(MUTED));
                    });
                });
            }
            Some(img) => {
                let usable = img.usable();
                egui::Frame::none().fill(CARD).stroke(Stroke::new(1.5, if usable { OK } else { FAIL })).rounding(Rounding::same(10.0)).inner_margin(egui::Margin::same(14.0)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(if usable { "✔  Imagem pronta" } else { "✖  Este arquivo não serve" }).size(17.0).strong().color(if usable { OK } else { FAIL }));
                    ui.add_space(6.0);
                    Self::info_row(ui, "Arquivo:", &img.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
                    Self::info_row(ui, "Tipo:", &img.kind_label());
                    Self::info_row(ui, "Tamanho:", &format!("{} ({} setores)", human_size(img.bytes), img.sectors));
                    if let Some(fs) = &img.file_system {
                        Self::info_row(ui, "Formato interno:", fs);
                    }
                    if let Some(l) = &img.volume_label {
                        Self::info_row(ui, "Nome do disco:", l);
                    }
                    if usable {
                        Self::info_row(ui, "Precisa de:", img.need().label());
                    }
                    for p in &img.problems {
                        ui.label(RichText::new(format!("✖ {p}")).color(FAIL));
                    }
                    for n in &img.notes {
                        ui.label(RichText::new(format!("! {n}")).color(WARN));
                    }
                });
            }
        }
    }

    fn step_disc(&mut self, ui: &mut egui::Ui) {
        let need = self.image.as_ref().map(|i| i.need().label()).unwrap_or("um disco virgem");
        let first = format!("Coloque um disco VIRGEM de {need} no gravador, com o lado brilhante para baixo, e feche a bandeja.");
        Self::instructions(
            ui,
            &[
                &first,
                "Espere a luz do gravador parar de piscar (cerca de 10 segundos).",
                "Escolha o gravador na lista e clique em \"Ler o disco\".",
                "Quando aparecerem os dados do disco, clique em \"Próximo\".",
            ],
        );
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Seu disco:").size(15.0));
            let before = self.media_idx;
            egui::ComboBox::from_id_salt("media").width(420.0).selected_text(crate::media::PROFILES[self.media_idx].name).show_ui(ui, |ui| {
                for (k, m) in crate::media::PROFILES.iter().enumerate() {
                    ui.selectable_value(&mut self.media_idx, k, m.name);
                }
            });
            if before != self.media_idx {
                match crate::media::PROFILES[self.media_idx].kind {
                    crate::media::Kind::CdR => self.practice_disc = PracticeDisc::BlankCdR,
                    crate::media::Kind::DvdR => self.practice_disc = PracticeDisc::BlankDvdR,
                    crate::media::Kind::Unknown => {}
                }
                self.run_checks();
            }
        });
        {
            let m = &crate::media::PROFILES[self.media_idx];
            let hint = match (m.kind, self.mode) {
                (crate::media::Kind::DvdR, Mode::Picture) => Some(("Este DVD-R não serve para desenhar foto nesta versão. Use um CD-R.", WARN)),
                (crate::media::Kind::DvdR, Mode::Iso) => Some(("DVD-R: serve para imagens .iso de até 4,38 GB. Precisa de gravador de DVD.", MUTED)),
                (crate::media::Kind::CdR, _) => Some(("CD-R de 80 min: serve para imagens de até 702 MB e para o desenho de foto.", MUTED)),
                _ => None,
            };
            if let Some((t, c)) = hint {
                ui.label(RichText::new(t).color(c));
            }
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Gravador:").size(15.0));
            let current = self.drives.get(self.drive_idx).map(|d| d.name.clone()).unwrap_or_default();
            egui::ComboBox::from_id_salt("drive").width(420.0).selected_text(current).show_ui(ui, |ui| {
                for (k, d) in self.drives.iter().enumerate() {
                    if ui.selectable_label(k == self.drive_idx, &d.name).clicked() {
                        self.drive_idx = k;
                        self.disc = None;
                        self.checks.clear();
                    }
                }
            });
            if ui.button("↻ Procurar gravadores").clicked() {
                self.drives = drive::find_drives();
                self.drive_idx = 0;
            }
        });
        let is_practice = self.drives.get(self.drive_idx).map(|d| d.backend == drive::Backend::Practice).unwrap_or(false);
        if is_practice {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Disco de treino na bandeja:").color(MUTED));
                egui::ComboBox::from_id_salt("practice").width(300.0).selected_text(self.practice_disc.label()).show_ui(ui, |ui| {
                    for d in PracticeDisc::ALL {
                        if ui.selectable_label(self.practice_disc == d, d.label()).clicked() {
                            self.practice_disc = d;
                            self.disc = None;
                            self.checks.clear();
                        }
                    }
                });
            });
            ui.label(RichText::new("O gravador de treino não grava nada: serve para aprender os passos.").color(MUTED).italics());
        }
        ui.add_space(10.0);
        let reading = self.reading.is_some();
        ui.horizontal(|ui| {
            let label = if self.disc.is_some() { "Ler o disco de novo" } else { "Ler o disco" };
            if ui.add_enabled(!reading, egui::Button::new(RichText::new(label).size(16.0)).min_size(Vec2::new(220.0, 40.0))).clicked() {
                self.start_reading();
            }
            if reading {
                ui.spinner();
                ui.label("Lendo o disco... (pode levar alguns segundos)");
            }
        });
        ui.add_space(10.0);
        if let Some(d) = &self.disc {
            egui::Frame::none().fill(CARD).stroke(Stroke::new(1.0, Color32::from_gray(210))).rounding(Rounding::same(10.0)).inner_margin(egui::Margin::same(14.0)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("Informações do disco").strong().size(16.0));
                ui.add_space(4.0);
                if !d.ready {
                    ui.label(RichText::new("O gravador não respondeu. Espere alguns segundos e leia de novo.").color(FAIL));
                } else if !d.present {
                    ui.label(RichText::new("Não há disco na bandeja.").color(FAIL));
                } else {
                    Self::info_row(ui, "Tipo de disco:", &d.media_type);
                    Self::info_row(ui, "Situação:", d.status.label());
                    Self::info_row(ui, "Regravável:", match d.erasable { Some(true) => "Sim", Some(false) => "Não", None => "Desconhecido" });
                    Self::info_row(ui, "Espaço livre:", &format!("{} setores ({:.0} MB)", d.free_sectors, mib(d.free_sectors)));
                    if !d.speeds.is_empty() {
                        let s: Vec<String> = d.speeds.iter().map(|s| format!("{s}x")).collect();
                        Self::info_row(ui, "Velocidades possíveis:", &s.join(", "));
                    }
                    if let Some(m) = &d.manufacturer {
                        Self::info_row(ui, "Fabricante do disco:", m);
                    }
                }
            });
        }
    }

    fn step_checks(&mut self, ui: &mut egui::Ui) {
        Self::instructions(
            ui,
            &[
                "Leia a lista abaixo. Verde = tudo bem. Amarelo = atenção. Vermelho = impede a gravação.",
                "Se houver algo em vermelho, faça o que está escrito depois da seta ▶ e clique em \"Verificar de novo\".",
                "Quando não houver nada em vermelho, clique em \"Próximo\".",
            ],
        );
        ui.add_space(10.0);
        let worst = checks::worst(&self.checks);
        let (txt, col) = match worst {
            Level::Ok => ("Este disco pode receber a imagem.", OK),
            Level::Warn => ("Este disco pode receber a imagem, mas leia os avisos.", WARN),
            Level::Fail => ("Este disco NÃO pode receber a imagem. Gravar agora não daria certo.", FAIL),
        };
        ui.horizontal(|ui| {
            ui.label(RichText::new(txt).size(17.0).strong().color(col));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let reading = self.reading.is_some();
                if reading {
                    ui.spinner();
                }
                if ui.add_enabled(!reading, egui::Button::new("↻ Verificar de novo")).clicked() {
                    self.start_reading();
                }
            });
        });
        ui.add_space(8.0);
        let list = self.checks.clone();
        for c in &list {
            Self::check_row(ui, c);
        }
        if self.needs_erase_ok() && worst != Level::Fail {
            ui.checkbox(&mut self.erase_ok, RichText::new("Entendi: tudo o que está neste disco regravável será apagado.").strong());
        }
    }

    fn step_options(&mut self, ui: &mut egui::Ui) {
        let rec = self.disc.as_ref().and_then(checks::recommended_speed);
        Self::instructions(
            ui,
            &[
                "Na maioria dos casos, deixe tudo como está: já escolhemos os valores mais seguros.",
                "Se um disco já falhou antes, escolha uma velocidade ainda menor.",
                "Clique em \"Próximo\" para revisar e gravar.",
            ],
        );
        ui.add_space(12.0);
        let speeds = self.disc.as_ref().map(|d| d.speeds.clone()).unwrap_or_default();
        egui::Frame::none().fill(CARD).stroke(Stroke::new(1.0, Color32::from_gray(210))).rounding(Rounding::same(10.0)).inner_margin(egui::Margin::same(14.0)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("Velocidade de gravação").strong().size(16.0));
            ui.label(RichText::new("A velocidade máxima é a causa mais comum de discos que saem vazios ou com erro. Mas devagar demais também não ajuda: o melhor é a velocidade do meio, já escolhida abaixo.").color(MUTED));
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                if speeds.is_empty() {
                    ui.radio_value(&mut self.speed, None, "Automática (o gravador escolhe)");
                }
                for s in &speeds {
                    let label = if Some(*s) == rec { format!("{s}x  (recomendada)") } else { format!("{s}x") };
                    ui.radio_value(&mut self.speed, Some(*s), label);
                }
            });
            if let (Some(chosen), Some(r)) = (self.speed, rec) {
                let disc = self.disc.as_ref();
                let is_cd = disc.map(|d| d.is_cd()).unwrap_or(true);
                let floor = if is_cd { CD_MIN_GOOD_SPEED } else { DVD_MIN_GOOD_SPEED };
                if chosen > r {
                    ui.label(RichText::new(format!("! {chosen}x é mais rápido que o recomendado ({r}x). Discos baratos podem falhar.")).color(WARN));
                } else if chosen < floor {
                    ui.label(RichText::new(format!("! {chosen}x é devagar demais para um disco feito para altas velocidades. O laser pode não estar ajustado para isso e a gravação pode piorar. Prefira {r}x.")).color(WARN));
                }
                let one_x = if is_cd { CD_1X_KBS } else if disc.map(|d| d.is_bd()).unwrap_or(false) { BD_1X_KBS } else { DVD_1X_KBS };
                ui.label(RichText::new(format!("{chosen}x = {} KB por segundo.", speed_kbs(chosen, one_x))).color(MUTED));
            }
        });
        ui.add_space(10.0);
        if self.mode == Mode::Picture {
            let before = self.preset_idx;
            egui::Frame::none().fill(CARD).stroke(Stroke::new(1.0, Color32::from_gray(210))).rounding(Rounding::same(10.0)).inner_margin(egui::Margin::same(14.0)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("Medidas do disco").strong().size(16.0));
                ui.label(RichText::new("O desenho é calculado a partir de onde a trilha passa no disco. Se a sua marca não está na lista, use o padrão de 80 minutos.").color(MUTED));
                for (k, g) in PRESETS.iter().enumerate() {
                    ui.radio_value(&mut self.preset_idx, k, g.name);
                    if self.preset_idx == k {
                        ui.label(RichText::new(format!("    {}", g.note)).color(MUTED).size(13.0));
                    }
                }
            });
            if before != self.preset_idx {
                self.preview_dirty = true;
                self.run_checks();
            }
            ui.add_space(10.0);
            egui::Frame::none().fill(CARD).stroke(Stroke::new(1.0, Color32::from_gray(210))).rounding(Rounding::same(10.0)).inner_margin(egui::Margin::same(14.0)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.checkbox(&mut self.test_mode, RichText::new("Só simular (gravação de teste, não grava de verdade)").strong());
                ui.label(RichText::new("Testa se o gravador aceita o disco sem gastá-lo. Nem todo gravador suporta.").color(MUTED));
                ui.add_space(8.0);
                ui.checkbox(&mut self.eject, RichText::new("Abrir a bandeja quando terminar").strong());
                ui.checkbox(&mut self.sounds, RichText::new("Tocar um som quando terminar (ou se der erro)").strong());
            });
            return;
        }
        egui::Frame::none().fill(CARD).stroke(Stroke::new(1.0, Color32::from_gray(210))).rounding(Rounding::same(10.0)).inner_margin(egui::Margin::same(14.0)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.checkbox(&mut self.verify, RichText::new("Conferir o disco depois de gravar (recomendado)").strong());
            ui.label(RichText::new("Lê o disco no final e compara com a imagem. Assim você fica sabendo na hora se a gravação deu certo.").color(MUTED));
            if cfg!(target_os = "windows") {
                ui.label(RichText::new("No Windows a conferência automática não está disponível nesta versão.").color(WARN));
            }
            ui.add_space(8.0);
            ui.checkbox(&mut self.test_mode, RichText::new("Só simular (gravação de teste, não grava de verdade)").strong());
            ui.label(RichText::new("Faz todo o processo com o laser desligado, para testar se o gravador aceita o disco. Nem todo gravador suporta.").color(MUTED));
            ui.add_space(8.0);
            ui.checkbox(&mut self.eject, RichText::new("Abrir a bandeja quando terminar").strong());
                ui.checkbox(&mut self.sounds, RichText::new("Tocar um som quando terminar (ou se der erro)").strong());
        });
    }

    fn step_burn(&mut self, ui: &mut egui::Ui) {
        if self.mode == Mode::Picture {
            self.step_burn_picture(ui);
            return;
        }
        let busy = self.burn_rx.is_some();
        Self::instructions(
            ui,
            &[
                "Confira o resumo abaixo.",
                "Clique em \"Gravar agora\".",
                "Durante a gravação NÃO abra a bandeja, não desligue o computador e evite usar programas pesados.",
            ],
        );
        ui.add_space(12.0);
        egui::Frame::none().fill(CARD).stroke(Stroke::new(1.0, Color32::from_gray(210))).rounding(Rounding::same(10.0)).inner_margin(egui::Margin::same(14.0)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("Resumo").strong().size(16.0));
            if let Some(i) = &self.image {
                Self::info_row(ui, "Imagem:", &format!("{} ({})", i.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), human_size(i.bytes)));
            }
            if let Some(d) = self.drives.get(self.drive_idx) {
                Self::info_row(ui, "Gravador:", &d.name);
            }
            if let Some(d) = &self.disc {
                Self::info_row(ui, "Disco:", &format!("{} — {}", d.media_type, d.status.label()));
            }
            Self::info_row(ui, "Velocidade:", &self.speed.map(|s| format!("{s}x")).unwrap_or_else(|| "automática".into()));
            Self::info_row(ui, "Conferir depois:", if self.verify { "Sim" } else { "Não" });
            Self::info_row(ui, "Modo:", if self.test_mode { "Só simulação" } else { "Gravação de verdade" });
        });
        ui.add_space(14.0);
        if busy {
            ui.label(RichText::new("Gravando... não abra a bandeja.").size(17.0).strong().color(ACCENT));
            ui.add_space(6.0);
            ui.add(egui::ProgressBar::new(self.progress).desired_height(26.0).show_percentage().animate(true));
            ui.label(&self.progress_text);
        } else {
            ui.vertical_centered(|ui| {
                let b = egui::Button::new(RichText::new("Gravar agora").size(19.0).strong().color(Color32::WHITE)).fill(ACCENT).min_size(Vec2::new(280.0, 52.0));
                if ui.add(b).clicked() {
                    self.start_burn();
                }
            });
        }
    }

    fn step_burn_picture(&mut self, ui: &mut egui::Ui) {
        let busy = self.burn_rx.is_some() || self.gen_rx.is_some();
        Self::instructions(
            ui,
            &[
                "Confira o resumo abaixo.",
                "Clique em \"Desenhar no disco agora\". Primeiro a foto vira uma faixa de som especial (uns 850 MB no computador, leva alguns minutos); depois ela é gravada.",
                "Durante a gravação NÃO abra a bandeja e não desligue o computador. A gravação leva de 5 a 20 minutos.",
                "Quer gravar em outro programa (por exemplo, no ImgBurn)? Use \"Só criar os arquivos\".",
            ],
        );
        ui.add_space(12.0);
        let sectors = self.picture_sectors();
        let folder = self.out_folder.clone().unwrap_or_else(|| self.default_out_folder());
        egui::Frame::none().fill(CARD).stroke(Stroke::new(1.0, Color32::from_gray(210))).rounding(Rounding::same(10.0)).inner_margin(egui::Margin::same(14.0)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("Resumo").strong().size(16.0));
            if let Some(p) = &self.picture {
                Self::info_row(ui, "Foto:", &p.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "exemplo".into()));
            }
            if let Some(d) = self.drives.get(self.drive_idx) {
                Self::info_row(ui, "Gravador:", &d.name);
            }
            if let Some(d) = &self.disc {
                Self::info_row(ui, "Disco:", &format!("{} — {}", d.media_type, d.status.label()));
            }
            Self::info_row(ui, "Medidas:", PRESETS[self.preset_idx].name);
            Self::info_row(ui, "Tamanho da faixa:", &format!("{sectors} setores ({:.0} MB, {:.0} minutos de som)", (sectors * picture::CD_SECTOR) as f64 / 1048576.0, sectors as f64 / 75.0 / 60.0));
            Self::info_row(ui, "Velocidade:", &self.speed.map(|s| format!("{s}x")).unwrap_or_else(|| "automática".into()));
            ui.horizontal(|ui| {
                ui.label(RichText::new("Pasta dos arquivos:").color(MUTED));
                ui.label(RichText::new(folder.display().to_string()).strong());
                if !busy && ui.small_button("Trocar...").clicked() {
                    if let Some(f) = rfd::FileDialog::new().set_title("Onde salvar a faixa do desenho").pick_folder() {
                        self.out_folder = Some(f);
                    }
                }
            });
            ui.label(RichText::new("O disco vai tocar como um CD de som com chiado. É normal: o \"som\" é o desenho.").color(MUTED).size(13.0));
        });
        ui.add_space(14.0);
        if busy {
            let txt = if self.gen_rx.is_some() { "Preparando o desenho..." } else { "Gravando o desenho... não abra a bandeja." };
            ui.label(RichText::new(txt).size(17.0).strong().color(ACCENT));
            ui.add_space(6.0);
            ui.add(egui::ProgressBar::new(self.progress).desired_height(26.0).show_percentage().animate(true));
            ui.label(&self.progress_text);
        } else {
            ui.horizontal(|ui| {
                let b = egui::Button::new(RichText::new("Desenhar no disco agora").size(19.0).strong().color(Color32::WHITE)).fill(ACCENT).min_size(Vec2::new(300.0, 52.0));
                if ui.add(b).clicked() {
                    self.start_picture(false);
                }
                ui.add_space(10.0);
                if ui.add(egui::Button::new(RichText::new("Só criar os arquivos").size(16.0)).min_size(Vec2::new(200.0, 52.0))).clicked() {
                    self.start_picture(true);
                }
            });
        }
    }

    fn step_done(&mut self, ui: &mut egui::Ui) {
        if self.mode == Mode::Picture {
            if let Some(Ok(m)) = &self.result {
                if self.save_only {
                    ui.label(RichText::new("✔  Arquivos do desenho criados").size(22.0).strong().color(OK));
                    ui.label(RichText::new(m).size(15.0));
                    ui.add_space(10.0);
                    Self::instructions(ui, &[
                        "Para gravar no ImgBurn: abra-o e clique em \"Write image file to disc\".",
                        "Em \"Source\", escolha o arquivo faixa-desenho.cue (não o .wav).",
                        "Coloque um CD-R virgem, escolha uma velocidade baixa (8x ou 16x) e clique no botão grande de gravar.",
                        "Mantenha o .cue e o .wav juntos na mesma pasta.",
                    ]);
                } else {
                    ui.label(RichText::new("✔  Pronto! O desenho foi gravado.").size(22.0).strong().color(OK));
                    ui.label(RichText::new(m).size(15.0));
                    ui.add_space(10.0);
                    Self::instructions(ui, &[
                        "Aperte o botão do gravador para abrir a bandeja. Vire o disco e olhe o lado gravado, inclinado contra uma luz forte.",
                        "O desenho aparece em tons de roxo/dourado, dependendo da marca do disco.",
                        "Se ficou torcido ou girado, as medidas desta marca são diferentes do padrão. Anote como ficou.",
                    ]);
                }
                ui.add_space(14.0);
                if ui.add(egui::Button::new(RichText::new("Desenhar em outro disco").size(16.0)).min_size(Vec2::new(240.0, 40.0))).clicked() {
                    self.step = 1;
                    self.disc = None;
                    self.checks.clear();
                    self.result = None;
                }
                return;
            }
        }
        match &self.result {
            Some(Ok(m)) => {
                ui.label(RichText::new("✔  Pronto! O disco foi gravado.").size(22.0).strong().color(OK));
                ui.label(RichText::new(m).size(15.0));
                ui.add_space(10.0);
                Self::instructions(
                    ui,
                    &[
                        "Aperte o botão do gravador para abrir a bandeja, retire o disco e escreva nele com caneta própria para CD (nunca etiqueta de papel).",
                        "Para gravar outra cópia, coloque outro disco virgem e clique em \"Gravar outro disco\".",
                    ],
                );
            }
            Some(Err(m)) => {
                ui.label(RichText::new("✖  A gravação não deu certo").size(22.0).strong().color(FAIL));
                ui.label(RichText::new(format!("O gravador respondeu: {m}")).size(14.0));
                ui.add_space(10.0);
                Self::instructions(
                    ui,
                    &[
                        "Este disco provavelmente foi perdido. Use um disco novo.",
                        "Grave numa velocidade menor (no passo Opções, escolha a menor da lista).",
                        "Se continuar falhando, experimente discos de outra marca: alguns gravadores não se dão bem com certas marcas.",
                        "Limpe a lente do gravador com um disco de limpeza, se tiver.",
                    ],
                );
            }
            None => {
                ui.label("Nada foi gravado ainda.");
            }
        }
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            if ui.add(egui::Button::new(RichText::new("Gravar outro disco").size(16.0)).min_size(Vec2::new(220.0, 40.0))).clicked() {
                self.step = 1;
                self.disc = None;
                self.checks.clear();
                self.result = None;
                self.erase_ok = false;
            }
            if ui.add(egui::Button::new(RichText::new("Começar do início").size(16.0)).min_size(Vec2::new(180.0, 40.0))).clicked() {
                self.step = 0;
                self.image = None;
                self.picture = None;
                self.disc = None;
                self.checks.clear();
                self.result = None;
                self.erase_ok = false;
            }
        });
    }

    fn side_panel(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);
        ui.label(RichText::new("Seu andamento").strong().size(16.0));
        ui.add_space(6.0);
        let img = if self.mode == Mode::Picture {
            self.picture.as_ref().map(|p| format!("Foto: {}", p.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "exemplo".into())))
        } else {
            self.image.as_ref().map(|i| i.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default())
        };
        let disc = self.disc.as_ref().map(|d| if d.present { format!("{} — {}", d.media_type, d.status.label()) } else { "sem disco".into() });
        let verdict = if self.checks.is_empty() {
            None
        } else {
            Some(match checks::worst(&self.checks) {
                Level::Ok => ("Compatível", OK),
                Level::Warn => ("Compatível, com avisos", WARN),
                Level::Fail => ("Não compatível", FAIL),
            })
        };
        let rows: [(&str, Option<String>, Color32); 3] = [
            (if self.mode == Mode::Picture { "O que gravar" } else { "Imagem" }, img, Color32::BLACK),
            ("Disco", disc, Color32::BLACK),
            ("Verificação", verdict.map(|v| v.0.to_string()), verdict.map(|v| v.1).unwrap_or(MUTED)),
        ];
        for (k, v, col) in rows {
            ui.label(RichText::new(k).color(MUTED).size(13.0));
            match v {
                Some(t) => ui.label(RichText::new(t).strong().color(col)),
                None => ui.label(RichText::new("ainda não").italics().color(MUTED)),
            };
            ui.add_space(4.0);
        }
        ui.separator();
        ui.label(RichText::new("Dúvidas comuns").strong());
        egui::CollapsingHeader::new("Imagem de disco ou foto?").default_open(true).show(ui, |ui| {
            ui.label("\"Imagem de disco\" (.iso) é uma cópia dos dados de um CD/DVD, não uma foto. Para fazer aparecer uma foto JPG/PNG na superfície do CD, escolha \"Uma foto desenhada no CD\" no passo 1.");
        });
        egui::CollapsingHeader::new("O disco saiu vazio. Por quê?").show(ui, |ui| {
            ui.label("As causas mais comuns: imagem maior que o disco (ex.: imagem de DVD num CD), arquivo .zip/.rar em vez de .iso, velocidade alta demais para o disco, ou disco já usado. A verificação do passo 3 confere tudo isso antes de gravar.");
        });
        egui::CollapsingHeader::new("Funciona com ELGIN e Multilaser?").show(ui, |ui| {
            ui.label("ELGIN CD-R 700 MB 80 min: sim, para imagens de até 702 MB e para o desenho de foto. Grave a 16x ou menos, não a 52x. Multilaser DVD-R 4,7 GB: sim, para imagens .iso de até 4,38 GB (a 8x ou menos); o desenho de foto é só para CD-R. Quem decide no fim é o gravador: a verificação do passo 3 confere o disco antes de gastá-lo.");
        });
        egui::CollapsingHeader::new("CD-R ou CD-RW?").show(ui, |ui| {
            ui.label("CD-R grava uma vez só. CD-RW pode ser apagado e regravado, mas nem todo aparelho lê.");
        });
        egui::CollapsingHeader::new("Qual velocidade usar?").show(ui, |ui| {
            ui.label(format!("Para CD-R, cerca de {CD_SAFE_SPEED}x. Para DVD, cerca de {DVD_SAFE_SPEED}x."));
            ui.label(format!("Mais devagar NÃO é sempre melhor: a velocidade máxima é a que mais falha, mas a mínima também pode piorar, porque discos de 52x (CD) ou 16x (DVD) são ajustados para velocidades médias. Fique no meio: CD-R entre {CD_MIN_GOOD_SPEED}x e {CD_SAFE_SPEED}x, DVD-R entre {DVD_MIN_GOOD_SPEED}x e {DVD_SAFE_SPEED}x."));
        });
        ui.separator();
        ui.checkbox(&mut self.show_log, "Mostrar registro detalhado");
        if self.show_log {
            egui::ScrollArea::vertical().max_height(220.0).stick_to_bottom(true).show(ui, |ui| {
                for l in &self.log {
                    ui.label(RichText::new(l).size(12.0).monospace());
                }
            });
        }
    }
}

impl eframe::App for GuidedBurner {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();
        if self.preview_dirty {
            self.preview_dirty = false;
            if let Some(p) = &self.picture {
                let img = p.preview(420, &PRESETS[self.preset_idx], self.picture_sectors());
                self.preview_tex = Some(ctx.load_texture("disc-preview", img, egui::TextureOptions::LINEAR));
            }
        }

        // Files dropped on the window go to step 1.
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        if let Some(p) = dropped.into_iter().next() {
            if self.burn_rx.is_none() {
                self.pick_any(p);
                self.step = 0;
            }
        }

        egui::TopBottomPanel::top("header")
            .frame(egui::Frame::none().fill(ACCENT).inner_margin(egui::Margin::symmetric(18.0, 12.0)))
            .show(ctx, |ui| self.header(ui));

        egui::TopBottomPanel::top("steps")
            .frame(egui::Frame::none().fill(CARD).inner_margin(egui::Margin::symmetric(18.0, 8.0)))
            .show(ctx, |ui| self.step_bar(ui));

        let (next_ok, why) = self.can_go_next();
        let busy = self.burn_rx.is_some();
        egui::TopBottomPanel::bottom("nav")
            .frame(egui::Frame::none().fill(CARD).inner_margin(egui::Margin::symmetric(18.0, 12.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let back_ok = self.step > 0 && self.step < 5 && !busy;
                    if ui.add_enabled(back_ok, egui::Button::new(RichText::new("◀  Voltar").size(15.0)).min_size(Vec2::new(120.0, 36.0))).clicked() {
                        self.step -= 1;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.step < 4 {
                            let b = egui::Button::new(RichText::new("Próximo  ▶").size(15.0).strong().color(if next_ok { Color32::WHITE } else { MUTED }))
                                .fill(if next_ok { ACCENT } else { Color32::from_gray(225) })
                                .min_size(Vec2::new(140.0, 36.0));
                            if ui.add_enabled(next_ok, b).clicked() {
                                if self.step == 1 {
                                    self.run_checks();
                                }
                                self.step += 1;
                            }
                        }
                        if !why.is_empty() {
                            ui.label(RichText::new(why).color(MUTED).size(14.0));
                        }
                    });
                });
            });

        egui::SidePanel::right("side")
            .exact_width(290.0)
            .frame(egui::Frame::none().fill(Color32::from_rgb(0xfa, 0xfb, 0xfc)).inner_margin(egui::Margin::symmetric(14.0, 6.0)).stroke(Stroke::new(1.0, Color32::from_gray(220))))
            .show(ctx, |ui| self.side_panel(ui));

        egui::CentralPanel::default().frame(egui::Frame::none().fill(BG).inner_margin(egui::Margin::same(20.0))).show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                ui.label(RichText::new(format!("Passo {} de {} — {}", self.step + 1, STEPS.len(), step_title(self.step))).size(22.0).strong());
                ui.label(RichText::new(step_why(self.step)).size(14.0).color(MUTED));
                ui.add_space(12.0);
                match self.step {
                    0 => self.step_choose(ui),
                    1 => self.step_disc(ui),
                    2 => self.step_checks(ui),
                    3 => self.step_options(ui),
                    4 => self.step_burn(ui),
                    _ => self.step_done(ui),
                }
            });
        });

        if self.reading.is_some() || busy {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }

        // --screenshot: draw ~120 frames, save the picture, close.
        if let Some(path) = self.screenshot.clone() {
            self.frames += 1;
            ctx.request_repaint();
            if self.frames >= 120 && !self.shot_requested {
                self.shot_requested = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
            }
            let shot = ctx.input(|i| {
                i.raw.events.iter().find_map(|e| if let egui::Event::Screenshot { image, .. } = e { Some(image.clone()) } else { None })
            });
            if let Some(img) = shot {
                let [w, h] = img.size;
                let bytes: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                if let Err(e) = ::image::save_buffer(&path, &bytes, w as u32, h as u32, ::image::ExtendedColorType::Rgba8) {
                    eprintln!("could not save screenshot: {e}");
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            if self.frames > 600 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
}

fn step_title(step: usize) -> &'static str {
    match step {
        0 => "Escolha o que gravar",
        1 => "Coloque o disco e leia",
        2 => "Verificação de compatibilidade",
        3 => "Opções de gravação",
        4 => "Gravar",
        _ => "Resultado",
    }
}

fn step_why(step: usize) -> &'static str {
    match step {
        0 => "Uma imagem de disco (.iso) copia um CD/DVD. Uma foto (JPG/PNG) é desenhada na superfície do CD-R.",
        1 => "O programa precisa ler o disco para saber o tipo, o tamanho e se ele está vazio.",
        2 => "Antes de gastar o disco, conferimos se ele consegue mesmo receber esta imagem.",
        3 => "Já deixamos escolhidos os valores mais seguros para o seu disco.",
        4 => "Última conferência antes de gravar.",
        _ => "Veja como foi a gravação e o que fazer em seguida.",
    }
}

fn setup_style(ctx: &egui::Context) {
    ctx.set_visuals(egui::Visuals::light());
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = Vec2::new(8.0, 6.0);
    style.spacing.button_padding = Vec2::new(12.0, 6.0);
    style.spacing.interact_size.y = 26.0;
    for (_, f) in style.text_styles.iter_mut() {
        f.size = (f.size * 1.08).round();
    }
    style.visuals.widgets.inactive.rounding = Rounding::same(6.0);
    style.visuals.widgets.hovered.rounding = Rounding::same(6.0);
    style.visuals.widgets.active.rounding = Rounding::same(6.0);
    style.visuals.selection.bg_fill = ACCENT;
    ctx.set_style(style);
}
