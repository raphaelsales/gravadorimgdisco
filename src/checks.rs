//! The compatibility check run before burning: is this disc able to receive this image?

use crate::drive::{DiscInfo, DiscStatus, Drive};
use crate::image::{human_size, ImageInfo, MediaNeed};
use crate::mechanics::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Ok,
    Warn,
    Fail,
}

#[derive(Clone, Debug)]
pub struct Check {
    pub level: Level,
    pub title: String,
    pub detail: String,
    /// What the person should do about it (empty when nothing).
    pub todo: String,
}

fn ok(title: &str, detail: String) -> Check {
    Check { level: Level::Ok, title: title.into(), detail, todo: String::new() }
}
fn warn(title: &str, detail: String, todo: &str) -> Check {
    Check { level: Level::Warn, title: title.into(), detail, todo: todo.into() }
}
fn fail(title: &str, detail: String, todo: &str) -> Check {
    Check { level: Level::Fail, title: title.into(), detail, todo: todo.into() }
}

pub fn recommended_speed(disc: &DiscInfo) -> Option<u32> {
    let safe = if disc.is_cd() { CD_SAFE_SPEED } else { DVD_SAFE_SPEED };
    if disc.speeds.is_empty() {
        return None;
    }
    let floor = if disc.is_cd() { CD_MIN_GOOD_SPEED } else { DVD_MIN_GOOD_SPEED };
    // The highest offered speed between the floor and the safe speed; else the highest
    // offered at or below the safe speed; else the slowest offered.
    disc.speeds
        .iter()
        .copied()
        .filter(|s| *s <= safe && *s >= floor)
        .max()
        .or_else(|| disc.speeds.iter().copied().filter(|s| *s <= safe).max())
        .or_else(|| disc.speeds.iter().copied().min())
}

pub fn run(image: &ImageInfo, drive: &Drive, disc: &DiscInfo) -> Vec<Check> {
    let mut v = vec![];

    // 1. The image file.
    if image.usable() {
        let mut d = format!("{} de {} ({} setores).", image.kind_label(), human_size(image.bytes), image.sectors);
        if let Some(l) = &image.volume_label {
            d.push_str(&format!(" Nome do disco: {l}."));
        }
        if image.notes.is_empty() {
            v.push(ok("O arquivo é uma imagem de disco", d));
        } else {
            v.push(warn("O arquivo é uma imagem de disco, com um aviso", format!("{d} {}", image.notes.join(" ")), "Se a gravação falhar, baixe a imagem de novo."));
        }
    } else {
        v.push(fail(
            "O arquivo escolhido não pode ser gravado como imagem",
            image.problems.join(" "),
            "Volte ao passo 1 e escolha um arquivo .iso (ou o .cue de uma imagem BIN/CUE).",
        ));
    }

    // 2. The drive is there and ready.
    if !disc.ready {
        v.push(fail(
            "O gravador não respondeu",
            "O gravador não ficou pronto a tempo. Isso acontece logo depois de fechar a bandeja.".into(),
            "Espere a luz do gravador parar de piscar (uns 10 segundos) e clique em \"Ler o disco de novo\".",
        ));
        return v;
    }

    // 3. A disc is in the tray.
    if !disc.present {
        v.push(fail(
            "Não há disco na bandeja",
            "O gravador está pronto, mas não encontrou nenhum disco.".into(),
            "Coloque um disco virgem com o lado brilhante para baixo, feche a bandeja, espere 10 segundos e leia o disco de novo.",
        ));
        return v;
    }
    v.push(ok("Disco encontrado", format!("Tipo de disco: {}.", disc.media_type)));

    // 4. The disc type can be written at all.
    if disc.is_rom() {
        v.push(fail(
            "Este disco não aceita gravação",
            format!("É um {} (disco de fábrica, só para leitura). Discos assim nunca podem ser gravados.", disc.media_type),
            "Use um disco virgem gravável: CD-R, CD-RW, DVD-R ou DVD+R.",
        ));
        return v;
    }
    if disc.supported == Some(false) {
        v.push(fail(
            "O gravador não consegue gravar este disco",
            format!("O sistema informou que este gravador não grava discos {}.", disc.media_type),
            "Use outro tipo de disco que o gravador aceite, ou outro gravador.",
        ));
    }
    if !drive.can_write.is_empty() {
        let base = disc.media_type.split_whitespace().next().unwrap_or("");
        let can = drive.can_write.iter().any(|w| w.eq_ignore_ascii_case(&disc.media_type) || w.eq_ignore_ascii_case(base));
        if can {
            v.push(ok("O gravador grava este tipo de disco", format!("Ele grava: {}.", drive.can_write.join(", "))));
        } else {
            v.push(fail(
                "O gravador não grava este tipo de disco",
                format!("Este gravador grava: {}. O disco é {}.", drive.can_write.join(", "), disc.media_type),
                "Troque o disco por um tipo da lista acima.",
            ));
        }
    }

    // 5. The disc is blank.
    match disc.status {
        DiscStatus::Empty => v.push(ok("O disco está vazio (virgem)", "Pronto para receber a imagem.".into())),
        DiscStatus::Complete | DiscStatus::Incomplete | DiscStatus::Others if disc.is_rewritable() => v.push(warn(
            "O disco regravável já tem dados",
            format!("Situação: {}. Tudo o que está nele será apagado antes de gravar.", disc.status.label()),
            "Se não houver nada importante nele, pode seguir. O apagamento acontece sozinho.",
        )),
        DiscStatus::Complete => v.push(fail(
            "O disco já foi gravado e fechado",
            "Um CD-R/DVD-R só pode ser gravado uma vez. Este já foi usado e não aceita mais nada.".into(),
            "Coloque um disco novo, virgem.",
        )),
        DiscStatus::Incomplete => v.push(fail(
            "O disco já tem uma gravação",
            "Este disco já foi gravado antes (sessão aberta). Uma imagem precisa de um disco totalmente vazio.".into(),
            "Coloque um disco novo, virgem.",
        )),
        _ => v.push(warn(
            "Não deu para saber se o disco está vazio",
            "O sistema não informou a situação do disco.".into(),
            "Se o disco for novo, pode seguir. Se a gravação falhar, tente outro disco.",
        )),
    }

    if !image.usable() {
        return v;
    }

    // 6. Right kind of disc for the size of the image.
    let need = image.need();
    let cd_like = disc.is_cd();
    match need {
        MediaNeed::Cd if !cd_like => v.push(warn(
            "Esta imagem caberia num CD",
            format!("A imagem tem {} e cabe num CD. Gravar num {} funciona, mas usa um disco maior que o necessário.", human_size(image.bytes), disc.media_type),
            "Pode seguir, ou usar um CD-R para economizar.",
        )),
        MediaNeed::Cd => v.push(ok("O tipo de disco combina com a imagem", "Imagem do tamanho de um CD, num CD.".into())),
        MediaNeed::DvdSingle | MediaNeed::DvdDouble | MediaNeed::BluRay | MediaNeed::TooBig if cd_like => v.push(fail(
            "A imagem é de DVD, mas o disco é um CD",
            format!(
                "A imagem tem {} e precisa de {}. Um CD só guarda cerca de 700 MB. Este é o motivo mais comum de \"não gravou nada\".",
                human_size(image.bytes),
                need.label()
            ),
            "Use um DVD-R ou DVD+R virgem (ou o disco indicado acima).",
        )),
        MediaNeed::DvdDouble if !disc.media_type.contains("DL") => v.push(fail(
            "A imagem precisa de um DVD de camada dupla",
            format!("A imagem tem {} e não cabe num DVD comum de 4,7 GB.", human_size(image.bytes)),
            "Use um DVD+R DL (8,5 GB).",
        )),
        // ImgBurn (0x791378): a DVD-sized image on a double-layer or Blu-ray disc gets an "underburn" warning.
        MediaNeed::DvdSingle if disc.is_bd() || disc.media_type.contains("DL") => v.push(warn(
            "Esta imagem caberia num DVD comum",
            format!("A imagem tem {} e cabe num DVD de 4,7 GB. Gravar num {} funciona, mas usa um disco maior e mais caro.", human_size(image.bytes), disc.media_type),
            "Pode seguir, ou usar um DVD-R/DVD+R comum.",
        )),
        MediaNeed::DvdDouble if disc.is_bd() => v.push(warn(
            "Esta imagem caberia num DVD de camada dupla",
            format!("A imagem tem {} e cabe num DVD+R DL. Gravar num {} funciona.", human_size(image.bytes), disc.media_type),
            "Pode seguir.",
        )),
        _ => v.push(ok("O tipo de disco combina com a imagem", format!("A imagem precisa de {}.", need.label()))),
    }

    // 7. Enough free space (ImgBurn: image sectors vs disc sectors).
    let free = if disc.free_sectors > 0 { disc.free_sectors } else { disc.total_sectors };
    if free > 0 {
        let img = image.sectors;
        let detail = format!(
            "Tamanho da imagem: {img} setores ({:.0} MB). Espaço no disco: {free} setores ({:.0} MB).",
            mib(img),
            mib(free)
        );
        if img > free {
            v.push(fail(
                "Não há espaço suficiente no disco",
                detail,
                "Use um disco maior (por exemplo, DVD em vez de CD). Forçar a gravação além do limite quase sempre estraga o disco.",
            ));
        } else if img as f64 > free as f64 * NEAR_FULL_RATIO {
            v.push(warn(
                "A imagem ocupa quase o disco inteiro",
                format!("{detail} A borda do disco é a parte que mais falha em discos baratos."),
                "Grave na velocidade recomendada e deixe a conferência ligada.",
            ));
        } else {
            v.push(ok("Há espaço suficiente no disco", detail));
        }
    } else {
        v.push(warn("Não deu para medir o espaço livre", "O sistema não informou o tamanho do disco.".into(), "Confira se o disco é do tamanho certo para a imagem."));
    }

    // 8. Speed.
    if let Some(r) = recommended_speed(disc) {
        let max = disc.speeds.iter().copied().max().unwrap_or(r);
        v.push(Check {
            level: if max > r { Level::Warn } else { Level::Ok },
            title: format!("Velocidade recomendada: {r}x"),
            detail: format!(
                "O gravador oferece até {max}x neste disco. Discos de marcas populares costumam falhar na velocidade máxima, gravando um disco vazio ou ilegível. Devagar demais também não é o ideal: o meio é o mais seguro."
            ),
            todo: if max > r { format!("No próximo passo já deixamos {r}x escolhido. Não use a velocidade máxima.") } else { String::new() },
        });
    }

    v
}

pub fn worst(v: &[Check]) -> Level {
    v.iter().map(|c| c.level).max().unwrap_or(Level::Ok)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drive::{parse_drutil_status, practice_disc, practice_drive, PracticeDisc};
    use crate::image::inspect;

    fn fake_iso(name: &str, sectors: u64) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("gg_{}_{name}", std::process::id()));
        let mut data = vec![0u8; (sectors * SECTOR_BYTES) as usize];
        data[32768] = 1;
        data[32769..32774].copy_from_slice(b"CD001");
        data[32808..32813].copy_from_slice(b"TESTE");
        std::fs::write(&p, &data).unwrap();
        p
    }

    #[test]
    fn iso_is_recognised_and_fits_cd() {
        let p = fake_iso("a.iso", 40);
        let i = inspect(&p);
        assert!(i.usable());
        assert_eq!(i.file_system.as_deref(), Some("ISO 9660"));
        assert_eq!(i.volume_label.as_deref(), Some("TESTE"));
        let c = run(&i, &practice_drive(), &practice_disc(PracticeDisc::BlankCdR));
        assert_ne!(worst(&c), Level::Fail);
    }

    #[test]
    fn zip_is_refused() {
        let p = std::env::temp_dir().join(format!("gg_{}_b.zip", std::process::id()));
        std::fs::write(&p, b"PK..").unwrap();
        assert!(!inspect(&p).usable());
    }

    #[test]
    fn dvd_image_on_cd_fails() {
        let mut i = inspect(&fake_iso("c.iso", 40));
        i.sectors = CD80_SECTORS + 1000;
        let c = run(&i, &practice_drive(), &practice_disc(PracticeDisc::BlankCdR));
        assert_eq!(worst(&c), Level::Fail);
        let used = run(&inspect(&fake_iso("d.iso", 40)), &practice_drive(), &practice_disc(PracticeDisc::UsedCdR));
        assert_eq!(worst(&used), Level::Fail);
    }

    #[test]
    fn drutil_blank_cdr() {
        let out = " Vendor   Product  Rev\n  HL-DT-ST DVDRW  GX40N\n\n           Type: CD-R               Name: /dev/disk4\n       Sessions: 0                  Tracks: 0\n   Overwritable:   00:00:00         blocks:        0 /   0.00MB /   0.00MiB\n     Space Free:   79:57:74         blocks:   359849 / 736.97MB / 702.83MiB\n     Space Used:   00:00:00         blocks:        0 /   0.00MB /   0.00MiB\n    Writability: appendable, blank\n";
        let d = parse_drutil_status(out);
        assert!(d.present);
        assert_eq!(d.media_type, "CD-R");
        assert_eq!(d.free_sectors, 359849);
        assert_eq!(d.status, crate::drive::DiscStatus::Empty);
    }
}

/// Compatibility check for "draw a picture on the disc" (CD audio track).
pub fn run_picture(
    has_picture: bool,
    geometry: &crate::picture::DiscGeometry,
    measured: bool,
    drive: &Drive,
    disc: &DiscInfo,
) -> Vec<Check> {
    let mut v = vec![];
    if has_picture {
        v.push(ok("A foto está pronta", "A foto foi convertida em pontos claros e escuros para o disco.".into()));
    } else {
        v.push(fail("Nenhuma foto escolhida", "É preciso escolher uma foto (JPG ou PNG).".into(), "Volte ao passo 1 e escolha a foto."));
    }
    if !disc.ready {
        v.push(fail(
            "O gravador não respondeu",
            "O gravador não ficou pronto a tempo.".into(),
            "Espere a luz do gravador parar de piscar (uns 10 segundos) e clique em \"Ler o disco de novo\".",
        ));
        return v;
    }
    if !disc.present {
        v.push(fail(
            "Não há disco na bandeja",
            "O gravador está pronto, mas não encontrou nenhum disco.".into(),
            "Coloque um CD-R virgem com o lado brilhante para baixo, feche a bandeja, espere 10 segundos e leia o disco de novo.",
        ));
        return v;
    }
    if disc.is_rom() {
        v.push(fail(
            "Este disco não aceita gravação",
            format!("É um {} (disco de fábrica). Discos assim nunca podem ser gravados.", disc.media_type),
            "Use um CD-R virgem.",
        ));
        return v;
    }
    if !disc.is_cd() {
        v.push(fail(
            "O desenho por foto só funciona em CD",
            format!("O disco na bandeja é um {}. Esta versão desenha fotos apenas em CD-R.", disc.media_type),
            "Troque por um CD-R virgem.",
        ));
        return v;
    }
    v.push(ok("É um CD", format!("Tipo de disco: {}.", disc.media_type)));
    if disc.media_type.contains("RW") {
        v.push(warn(
            "CD-RW mostra o desenho bem fraco",
            "Discos regraváveis quase não mudam de cor ao gravar, então o desenho fica difícil de ver.".into(),
            "Para um desenho visível, use CD-R. Pode seguir se for só um teste.",
        ));
    }
    if !drive.can_write.is_empty() {
        if drive.can_write.iter().any(|w| w.eq_ignore_ascii_case("CD-R")) {
            v.push(ok("O gravador grava CD-R", format!("Ele grava: {}.", drive.can_write.join(", "))));
        } else {
            v.push(fail("O gravador não grava CD-R", format!("Este gravador grava: {}.", drive.can_write.join(", ")), "Use um gravador que grave CD-R."));
        }
    }
    if disc.supported == Some(false) {
        v.push(fail("O gravador não aceita este disco", "O sistema informou que não consegue gravar este disco.".into(), "Tente outro CD-R, de outra marca."));
    }
    match disc.status {
        DiscStatus::Empty => v.push(ok("O disco está vazio (virgem)", "O desenho precisa de um disco totalmente vazio, e este está.".into())),
        _ if disc.is_rewritable() => v.push(warn(
            "O disco regravável já tem dados",
            "Tudo o que está nele será apagado antes de desenhar.".into(),
            "Se não houver nada importante nele, pode seguir.",
        )),
        DiscStatus::Unknown | DiscStatus::Others => v.push(warn(
            "Não deu para saber se o disco está vazio",
            "O sistema não informou a situação do disco.".into(),
            "Se o disco for novo, pode seguir.",
        )),
        _ => v.push(fail(
            "O disco já foi usado",
            "Um CD-R só pode ser gravado uma vez, e o desenho precisa do disco inteiro vazio.".into(),
            "Coloque um CD-R novo, virgem.",
        )),
    }
    let want = geometry.sectors.saturating_sub(2);
    let got = crate::picture::sectors_for(geometry, disc.free_sectors);
    if disc.free_sectors == 0 {
        v.push(warn("Não deu para medir o espaço do disco", "Vamos desenhar em um CD inteiro de 80 minutos.".into(), "Se o disco for de 74 minutos, escolha esse tamanho no passo Opções."));
    } else if got * 10 < want {
        v.push(fail(
            "Quase não há espaço livre no disco",
            format!("O disco tem só {} setores livres.", disc.free_sectors),
            "Use um CD-R virgem de 700 MB.",
        ));
    } else if got * 100 < want * 99 {
        v.push(warn(
            "O desenho vai ocupar um pouco menos do disco",
            format!(
                "O disco tem {} setores livres e o desenho completo usaria {want}. A parte de fora do disco ficará sem desenho.",
                disc.free_sectors
            ),
            "Pode seguir. Se o disco for de 74 minutos, escolha esse tamanho no passo Opções.",
        ));
    } else {
        v.push(ok("Há espaço para o desenho inteiro", format!("O desenho usa {got} setores; o disco tem {} livres.", disc.free_sectors)));
    }
    if measured {
        v.push(ok("Medidas do disco conhecidas", format!("Usando as medidas de \"{}\".", geometry.name)));
    } else {
        v.push(warn(
            "Esta marca de disco não tem medidas próprias",
            "Cada marca enrola a trilha de gravação de um jeito um pouco diferente. Com as medidas padrão o desenho aparece, mas pode sair um pouco girado ou torcido.".into(),
            "Faça o primeiro com um disco de teste. Se sair torcido, conte para a gente como ficou.",
        ));
    }
    if let Some(r) = recommended_speed(disc) {
        v.push(ok(&format!("Velocidade recomendada: {r}x"), "Gravar devagar deixa o desenho mais nítido e diminui falhas.".into()));
    }
    v
}

/// What the app knows about the disc brand the person picked.
pub fn brand(profile: &crate::media::MediaProfile, disc: &DiscInfo, picture_mode: bool) -> Vec<Check> {
    use crate::media::Kind;
    let mut v = vec![];
    if profile.kind == Kind::Unknown {
        return v;
    }
    if disc.present {
        let matches = match profile.kind {
            Kind::CdR => disc.media_type.starts_with("CD-R") && !disc.media_type.contains("RW"),
            Kind::DvdR => disc.is_dvd(),
            Kind::Unknown => true,
        };
        if !matches {
            v.push(warn(
                "O disco lido não parece ser o que você escolheu",
                format!("Você escolheu \"{}\", mas o gravador leu um {}.", profile.name, disc.media_type),
                "Confira se colocou o disco certo na bandeja, ou mude a escolha da marca no passo 2.",
            ));
        }
    }
    match profile.kind {
        Kind::CdR => {
            let speed_note = if profile.box_speed > profile.safe_speed {
                format!(
                    " O \"{}x\" da embalagem é a velocidade MÁXIMA que o disco aguenta; gravar nela é o que mais falha em discos populares. Vamos gravar a {}x ou menos.",
                    profile.box_speed, profile.safe_speed
                )
            } else {
                format!(" Vamos gravar a {}x ou menos.", profile.safe_speed)
            };
            v.push(ok(
                &format!("{}: compatível", profile.name),
                format!(
                    "É um CD-R comum de 80 minutos. Serve para imagens de até {} setores (cerca de 702 MB), o mesmo limite do ImgBurn, e para o desenho de foto.{speed_note}",
                    crate::mechanics::CD80_SECTORS
                ),
            ));
        }
        Kind::DvdR => {
            if picture_mode {
                v.push(fail(
                    &format!("{}: não serve para desenhar foto", profile.name),
                    "Esta versão desenha fotos só em CD-R. DVD usa outro tipo de gravação, que ainda não foi feito aqui.".into(),
                    "Para o desenho use um CD-R (por exemplo ELGIN). O DVD-R serve para gravar imagens .iso: volte ao passo 1 e escolha \"Uma imagem de disco\".",
                ));
            } else {
                v.push(ok(
                    &format!("{}: compatível", profile.name),
                    format!(
                        "Serve para imagens de até {} setores (cerca de 4,38 GB no computador; os \"4,7 GB\" da embalagem são contados de outro jeito). Precisa de um gravador de DVD. Vamos gravar a {}x ou menos, mesmo a embalagem dizendo {}x.",
                        crate::mechanics::DVD_SL_SECTORS,
                        profile.safe_speed,
                        if profile.box_speed > 0 { profile.box_speed.to_string() } else { "mais".into() }
                    ),
                ));
            }
        }
        Kind::Unknown => {}
    }
    v
}

#[cfg(test)]
mod brand_tests {
    use super::*;
    use crate::drive::{practice_disc, PracticeDisc};
    use crate::media::PROFILES;

    #[test]
    fn elgin_cd_ok_and_multilaser_refused_for_pictures() {
        let cd = practice_disc(PracticeDisc::BlankCdR);
        assert_eq!(worst(&brand(&PROFILES[1], &cd, true)), Level::Ok);
        let dvd = practice_disc(PracticeDisc::BlankDvdR);
        assert_eq!(worst(&brand(&PROFILES[2], &dvd, false)), Level::Ok);
        assert_eq!(worst(&brand(&PROFILES[2], &dvd, true)), Level::Fail);
        assert_eq!(worst(&brand(&PROFILES[1], &dvd, false)), Level::Warn);
    }
}

#[cfg(test)]
mod speed_tests {
    use super::*;
    use crate::drive::{practice_disc, PracticeDisc};

    #[test]
    fn imgburn_speed_formula() {
        assert_eq!(speed_kbs(16, CD_1X_KBS), 2822);
        assert_eq!(speed_kbs(8, DVD_1X_KBS), 11080);
    }

    #[test]
    fn middle_speed_is_recommended() {
        assert_eq!(recommended_speed(&practice_disc(PracticeDisc::BlankCdR)), Some(16));
        assert_eq!(recommended_speed(&practice_disc(PracticeDisc::BlankDvdR)), Some(8));
    }
}
