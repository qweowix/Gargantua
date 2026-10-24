use std::io::Read;
use std::sync::{mpsc, Arc, Mutex};

use anyhow::Result;
use slint::{Rgba8Pixel, SharedPixelBuffer};

use crate::spotify;
use crate::util::{agent, call};

pub type Buf = SharedPixelBuffer<Rgba8Pixel>;

const SIZE: u32 = 112;

/// 4 фоновых потока качают и ужимают обложки до квадрата 112px
#[derive(Clone)]
pub struct Loader {
    tx: mpsc::Sender<String>,
}

impl Loader {
    pub fn new(done: fn(String, Option<Buf>)) -> Self {
        let (tx, rx) = mpsc::channel::<String>();
        let rx = Arc::new(Mutex::new(rx));
        for _ in 0..4 {
            let rx = rx.clone();
            std::thread::spawn(move || loop {
                let Ok(url) = rx.lock().unwrap().recv() else { break };
                let buf = fetch(&url).ok();
                done(url, buf);
            });
        }
        Self { tx }
    }

    pub fn get(&self, url: String) {
        let _ = self.tx.send(url);
    }
}

/// spotify-oembed:ID -> реальная ссылка на картинку
fn real_url(url: &str) -> Result<String> {
    match url.strip_prefix(spotify::OEMBED) {
        Some(id) => {
            let v: serde_json::Value =
                agent().get("https://open.spotify.com/oembed").query("url", &format!("https://open.spotify.com/track/{id}")).call()?.into_json()?;
            Ok(v["thumbnail_url"].as_str().unwrap_or_default().to_string())
        }
        None => Ok(url.to_string()),
    }
}

fn load(url: &str) -> Result<image::DynamicImage> {
    let mut bytes = Vec::new();
    match url.strip_prefix(crate::local::COVER) {
        Some(path) => bytes = crate::local::picture(path).ok_or_else(|| anyhow::anyhow!("в файле нет обложки"))?,
        None => {
            call(agent().get(url))?.into_reader().take(8 << 20).read_to_end(&mut bytes)?;
        }
    }
    let img = image::load_from_memory(&bytes)?;
    let (w, h) = (img.width(), img.height());
    let s = w.min(h);
    Ok(img.crop_imm((w - s) / 2, (h - s) / 2, s, s))
}

fn buf(img: &image::RgbaImage) -> Buf {
    Buf::clone_from_slice(img.as_raw(), img.width(), img.height())
}

fn fetch(url: &str) -> Result<Buf> {
    Ok(buf(&load(&real_url(url)?)?.thumbnail(SIZE, SIZE).to_rgba8()))
}

/// Для экрана lyrics: большая обложка + два размытых слоя для фона а-ля Apple Music
pub struct Big {
    pub cover: Buf,
    pub bg: Buf,
    pub blob: Buf,
    pub art: Option<String>, // file:// для системных медиа-контролов (панели не все умеют http)
    pub palette: Option<[[u8; 3]; 8]>, // цвета темы по обложке, см. palette()
}

/// Обложка файлом в кэше. Имя по ссылке: панели кэшируют картинку по URL, одно имя на все треки
/// показывало бы старую обложку. Старые файлы удаляем — держим только текущую.
fn save_art(url: &str, img: &image::RgbaImage) -> Option<String> {
    use std::hash::{Hash, Hasher};
    let dir = dirs::cache_dir()?.join("BlackHole").join("art");
    std::fs::create_dir_all(&dir).ok()?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    url.hash(&mut h);
    let path = dir.join(format!("{:016x}.png", h.finish()));
    for e in std::fs::read_dir(&dir).ok()?.flatten() {
        if e.path() != path {
            let _ = std::fs::remove_file(e.path());
        }
    }
    image::imageops::thumbnail(img, 300, 300).save(&path).ok()?;
    Some(format!("file://{}", path.display()))
}

/// Те же картинки, только в нормальном размере
fn big_url(url: &str) -> Result<String> {
    Ok(real_url(url)?
        .replace("=w226-h226", "=w544-h544")
        .replace("-t300x300.", "-t500x500.")
        .replace("/mqdefault.", "/hqdefault.")
        .replace("00001e02", "0000b273"))
}

/// Обложка в JPEG для тегов скачанного mp3 (квадрат до 600px — плееры и телефоны больше не просят)
pub fn jpeg(url: &str) -> Result<Vec<u8>> {
    let img = load(&big_url(url)?)?;
    let side = img.width().min(img.height());
    let img = img.crop_imm((img.width() - side) / 2, (img.height() - side) / 2, side, side).thumbnail(600, 600).to_rgb8();
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 90).encode_image(&img)?;
    Ok(out)
}

pub fn big(url: &str) -> Result<Big> {
    let url = big_url(url)?;
    let img = load(&url)?;
    let cover = img.resize(480, 480, image::imageops::FilterType::Triangle).to_rgba8();

    // фон: крошечная размытая копия, растянутая на весь экран — сама превращается в мягкие пятна
    let bg = image::imageops::blur(&img.resize_exact(20, 20, image::imageops::FilterType::Triangle).to_rgba8(), 1.2);

    // пятно с мягкими прозрачными краями, его будем таскать поверх фона
    let mut blob = image::imageops::blur(&img.resize_exact(48, 48, image::imageops::FilterType::Triangle).to_rgba8(), 2.5);
    for (x, y, p) in blob.enumerate_pixels_mut() {
        let d = ((x as f32 - 23.5).powi(2) + (y as f32 - 23.5).powi(2)).sqrt() / 24.0;
        p[3] = ((1.0 - d).clamp(0.0, 1.0).powf(1.4) * 255.0) as u8;
    }
    // тёмная обложка с цветными бликами (синий свет на чёрном) после размытия становилась почти
    // чёрной — фон выходил просто чёрным. Вытягиваем яркость и цвет так, чтобы светлые места фона
    // были заметны; светлые обложки не трогаем
    let gain = lift_gain(&bg);
    let bg = lift(bg, gain);
    let blob = lift(blob, gain);
    let art = save_art(&url, &cover);
    let palette = palette(&img.resize_exact(48, 48, image::imageops::FilterType::Triangle).to_rgb8());
    Ok(Big { cover: buf(&cover), bg: buf(&bg), blob: buf(&blob), art, palette })
}

/// Во сколько раз поднять яркость фона: светлые 10% пикселей должны дотягивать до ~60%
fn lift_gain(img: &image::RgbaImage) -> f32 {
    let mut v: Vec<u8> = img.pixels().map(|p| p[0].max(p[1]).max(p[2])).collect();
    v.sort_unstable();
    let p90 = v[v.len() * 9 / 10] as f32 / 255.0;
    (0.6 / p90.max(0.02)).clamp(1.0, 6.0)
}

/// Ярче в gain раз и насыщеннее: цвет отходит от серого, пропорции оттенка те же
fn lift(mut img: image::RgbaImage, gain: f32) -> image::RgbaImage {
    if gain <= 1.0 {
        return img;
    }
    let sat = 1.0 + 0.3 * (gain - 1.0).min(1.0);
    for p in img.pixels_mut() {
        let [r, g, b] = [p[0], p[1], p[2]].map(|c| c as f32 * gain);
        let m = (r + g + b) / 3.0;
        for (i, c) in [r, g, b].into_iter().enumerate() {
            p[i] = (m + (c - m) * sat).clamp(0.0, 255.0) as u8;
        }
    }
    img
}

fn hsv(h: f32, s: f32, v: f32) -> [u8; 3] {
    let h = h.rem_euclid(360.0) / 60.0;
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [r, g, b].map(|k| ((k + m) * 255.0).round() as u8)
}

/// Тема по обложке: главный оттенок — самый насыщенный и частый, второй — другой заметный (или соседний).
/// Порядок как в Theme: c1, c2, soft1, pale1, soft2, deep1, deep2, glow. Серая обложка — None
pub fn palette(img: &image::RgbImage) -> Option<[[u8; 3]; 8]> {
    const BINS: usize = 36;
    let mut w = [0f32; BINS];
    let mut hx = [0f32; BINS]; // сумма векторов оттенка — чтобы взять точный оттенок, а не середину корзины
    let mut hy = [0f32; BINS];
    for p in img.pixels() {
        let [r, g, b] = p.0.map(|c| c as f32 / 255.0);
        let (max, min) = (r.max(g).max(b), r.min(g).min(b));
        let (s, v) = (if max > 0.0 { (max - min) / max } else { 0.0 }, max);
        if s < 0.2 || v < 0.15 {
            continue;
        }
        let d = max - min;
        let h = 60.0 * if max == r { ((g - b) / d).rem_euclid(6.0) } else if max == g { (b - r) / d + 2.0 } else { (r - g) / d + 4.0 };
        let k = ((h / 360.0 * BINS as f32) as usize).min(BINS - 1);
        let wt = s * v;
        w[k] += wt;
        hx[k] += wt * h.to_radians().cos();
        hy[k] += wt * h.to_radians().sin();
    }
    // сглаживаем по соседям: оттенок на границе двух корзин не должен проигрывать
    let sm: Vec<f32> = (0..BINS).map(|i| w[(i + BINS - 1) % BINS] * 0.5 + w[i] + w[(i + 1) % BINS] * 0.5).collect();
    let total: f32 = w.iter().sum();
    if total / ((img.width() * img.height()) as f32) < 0.04 {
        return None;
    }
    let hue = |i: usize| {
        let (x, y) = (-1..=1).map(|d| (i as i32 + d).rem_euclid(BINS as i32) as usize).fold((0.0, 0.0), |(x, y), k| (x + hx[k], y + hy[k]));
        y.atan2(x).to_degrees().rem_euclid(360.0)
    };
    let best = (0..BINS).max_by(|&a, &b| sm[a].total_cmp(&sm[b]))?;
    let h1 = hue(best);
    let far = |i: usize| (i as i32 - best as i32).rem_euclid(BINS as i32).min((best as i32 - i as i32).rem_euclid(BINS as i32)) >= 4;
    let h2 = (0..BINS).filter(|&i| far(i) && sm[i] > sm[best] * 0.05).max_by(|&a, &b| sm[a].total_cmp(&sm[b])).map_or(h1 + 35.0, hue);
    Some([
        hsv(h1, 0.65, 0.97),
        hsv(h2, 0.75, 1.0),
        hsv(h1, 0.58, 1.0),
        hsv(h1, 0.30, 1.0),
        hsv(h2, 0.58, 1.0),
        hsv(h1, 0.68, 0.28),
        hsv(h2, 0.85, 0.23),
        hsv(h1, 0.82, 0.85),
    ])
}

#[cfg(test)]
mod tests {
    #[test]
    fn dark_cover_keeps_colour() {
        // почти чёрная обложка с тусклым синим пятном: фон не должен уйти в чёрный
        let img = image::RgbaImage::from_fn(20, 20, |x, y| if (6..14).contains(&x) && y < 8 { image::Rgba([10, 25, 70, 255]) } else { image::Rgba([4, 4, 8, 255]) });
        let out = super::lift(img.clone(), super::lift_gain(&img));
        let p = out.get_pixel(10, 3);
        assert!(p[2] > 140 && p[2] > p[0] * 2, "синее пятно {p:?}");
        // светлую не трогаем
        let light = image::RgbaImage::from_pixel(20, 20, image::Rgba([200, 150, 90, 255]));
        assert_eq!(super::lift(light.clone(), super::lift_gain(&light)), light);
    }

    #[test]
    fn palette_follows_cover() {
        // синяя обложка с оранжевым пятном: главный — синий, второй — оранжевый
        let img = image::RgbImage::from_fn(48, 48, |x, y| if x < 12 && y < 12 { image::Rgb([240, 120, 30]) } else { image::Rgb([30, 70, 220]) });
        let p = super::palette(&img).unwrap();
        assert!(p[0][2] > p[0][0] && p[0][2] > p[0][1], "c1 {:?}", p[0]);
        assert!(p[1][0] > p[1][2], "c2 {:?}", p[1]);
        // серая — своей темы нет
        assert!(super::palette(&image::RgbImage::from_pixel(48, 48, image::Rgb([90, 90, 95]))).is_none());
    }
}
