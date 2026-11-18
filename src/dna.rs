//! Music DNA: отпечаток звучания трека, посчитанный по самому звуку, а не по тегам.
//! Звук берётся только с диска (офлайн-кэш, скачанное, локальные файлы): ffmpeg → моно 22 050 Гц →
//! кадры по 2048 отсчётов с шагом 512 → спектр. Из него — темп, тональность, громкость, энергия,
//! спектральные признаки, тембр (MFCC), гармония (хрома) и границы секций.
//! Чего по звуку без ML-модели честно не понять (инструменты, вокал, танцевальность, valence…) —
//! того в отпечатке нет, а не выдумано. См. docs/ecosystem.md.

use std::collections::HashMap;
use std::f32::consts::PI;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

/// Меняется вместе с алгоритмом: отпечатки старой версии пересчитываются
pub const VERSION: u32 = 1;
pub const SR: u32 = 22_050;
const N: usize = 2048;
const HOP: usize = 512;
const MELS: usize = 40;
pub const MFCC: usize = 13;
/// больше 8 минут не анализируем: признаки за это время уже устоялись, а длинные миксы жрали бы проц
const MAX_SECS: u32 = 480;

pub const NOTES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Dna {
    pub v: u32,
    /// сколько секунд звука разобрано
    pub secs: f32,
    /// темп; None — выраженного бита нет (эмбиент, речь)
    pub bpm: Option<f32>,
    /// выраженность бита 0..1: насколько чётко повторяются атаки с найденным периодом
    pub pulse: f32,
    /// атак в секунду
    pub onsets: f32,
    /// тоника 0..11 (C..B) и лад; None — тональность не определяется
    pub key: Option<u8>,
    pub minor: bool,
    /// насколько лучшая тональность обошла вторую, 0..1
    pub key_conf: f32,
    /// средняя громкость, дБFS
    pub rms_db: f32,
    pub peak_db: f32,
    /// разброс громкости между частями трека, дБ (95-й процентиль минус 10-й по окнам в 3 с)
    pub dyn_db: f32,
    /// энергия 0..1: громкость и плотность атак
    pub energy: f32,
    /// спектральный центроид, Гц — «яркость»
    pub centroid: f32,
    /// ниже этой частоты 85 % энергии
    pub rolloff: f32,
    /// 0 — тон, 1 — шум
    pub flatness: f32,
    /// доли энергии: низ (<250 Гц), середина (250–4000), верх (>4000)
    pub bands: [f32; 3],
    /// тембр: среднее и разброс MFCC 1..13 (нулевой — громкость, он в rms)
    pub mfcc: [f32; MFCC],
    pub mfcc_sd: [f32; MFCC],
    /// гармония: средняя хрома C..B, сумма 1
    pub chroma: [f32; 12],
    /// границы секций, секунды от начала
    pub sections: Vec<f32>,
}

impl Dna {
    pub fn key_name(&self) -> Option<String> {
        self.key.map(|k| format!("{}{}", NOTES[k as usize % 12], if self.minor { "m" } else { "" }))
    }
}

// ── раскодирование ──

/// Любой формат (webm/opus, m4a, mp3, flac) → моно float на 22 050 Гц, не длиннее MAX_SECS
pub fn decode(path: &Path) -> Result<Vec<f32>> {
    let ffmpeg = crate::util::find_bin("ffmpeg").context("для Music DNA нужен ffmpeg")?;
    let mut child = crate::util::command(&ffmpeg)
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(path)
        .args(["-t", &MAX_SECS.to_string(), "-vn", "-ac", "1", "-ar", &SR.to_string(), "-f", "f32le", "-"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .context("ffmpeg не запустился")?;
    let mut raw = Vec::new();
    child.stdout.take().context("нет вывода ffmpeg")?.read_to_end(&mut raw)?;
    let st = child.wait()?;
    if !st.success() && raw.is_empty() {
        bail!("ffmpeg не раскодировал {}", path.display());
    }
    Ok(raw.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect())
}

// ── спектр ──

/// Комплексное БПФ на месте, длина — степень двойки
fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * PI / len as f32;
        let (wr, wi) = (ang.cos(), ang.sin());
        for s in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let (a, b) = (s + k, s + k + len / 2);
                let tr = re[b] * cr - im[b] * ci;
                let ti = re[b] * ci + im[b] * cr;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let nc = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = nc;
            }
        }
        len <<= 1;
    }
}

fn hz_to_mel(f: f32) -> f32 {
    2595.0 * (1.0 + f / 700.0).log10()
}

fn mel_to_hz(m: f32) -> f32 {
    700.0 * (10f32.powf(m / 2595.0) - 1.0)
}

/// Треугольные мел-фильтры 30 Гц – 8 кГц: для каждого — (первый бин, веса)
fn mel_bank() -> Vec<(usize, Vec<f32>)> {
    let bin_hz = SR as f32 / N as f32;
    let (lo, hi) = (hz_to_mel(30.0), hz_to_mel(8000.0));
    let pts: Vec<f32> = (0..MELS + 2).map(|i| mel_to_hz(lo + (hi - lo) * i as f32 / (MELS + 1) as f32) / bin_hz).collect();
    (0..MELS)
        .map(|m| {
            let (a, c, b) = (pts[m], pts[m + 1], pts[m + 2]);
            let first = a.ceil() as usize;
            let w = (first..=b.floor() as usize).map(|k| {
                let k = k as f32;
                if k <= c { (k - a) / (c - a).max(1e-6) } else { (b - k) / (b - c).max(1e-6) }
            });
            (first, w.collect())
        })
        .collect()
}

/// Профили тональностей Крумхансла–Кесслер
const MAJOR: [f32; 12] = [6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88];
const MINOR: [f32; 12] = [6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17];

fn pearson(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len() as f32;
    let (ma, mb) = (a.iter().sum::<f32>() / n, b.iter().sum::<f32>() / n);
    let (mut s, mut sa, mut sb) = (0.0, 0.0, 0.0);
    for (x, y) in a.iter().zip(b) {
        s += (x - ma) * (y - mb);
        sa += (x - ma).powi(2);
        sb += (y - mb).powi(2);
    }
    if sa <= 0.0 || sb <= 0.0 { 0.0 } else { s / (sa * sb).sqrt() }
}

fn db(x: f32) -> f32 {
    20.0 * x.max(1e-9).log10()
}

fn percentile(v: &mut [f32], p: f32) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(f32::total_cmp);
    v[((v.len() - 1) as f32 * p).round() as usize]
}

// ── анализ ──

pub fn analyze(pcm: &[f32]) -> Result<Dna> {
    if pcm.len() < N * 8 {
        bail!("слишком короткий звук");
    }
    let hann: Vec<f32> = (0..N).map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / N as f32).cos()).collect();
    let bins = N / 2 + 1;
    let bin_hz = SR as f32 / N as f32;
    let bank = mel_bank();
    // нота каждого бина 55 Гц – 5 кГц для хромы
    let pc: Vec<Option<usize>> = (0..bins)
        .map(|k| {
            let f = k as f32 * bin_hz;
            (55.0..5000.0).contains(&f).then(|| ((12.0 * (f / 440.0).log2()).round() as i32 + 9).rem_euclid(12) as usize)
        })
        .collect();
    let frames = (pcm.len() - N) / HOP + 1;
    let fps = SR as f32 / HOP as f32;

    let (mut re, mut im) = (vec![0f32; N], vec![0f32; N]);
    let mut prev = vec![0f32; bins];
    let mut flux = Vec::with_capacity(frames);
    let mut rms = Vec::with_capacity(frames);
    let mut mfcc_frames: Vec<[f32; MFCC]> = Vec::with_capacity(frames);
    let mut chroma = [0f32; 12];
    let (mut cen_sum, mut roll_sum, mut flat_sum, mut wsum) = (0f64, 0f64, 0f64, 0f64);
    let mut bands = [0f64; 3];
    let mut peak = 0f32;
    for f in 0..frames {
        let s = &pcm[f * HOP..f * HOP + N];
        let mut e = 0f32;
        for i in 0..N {
            re[i] = s[i] * hann[i];
            im[i] = 0.0;
            e += s[i] * s[i];
            peak = peak.max(s[i].abs());
        }
        let r = (e / N as f32).sqrt();
        rms.push(r);
        fft(&mut re, &mut im);
        let mag: Vec<f32> = (0..bins).map(|k| (re[k] * re[k] + im[k] * im[k]).sqrt()).collect();
        // атаки: рост логарифма спектра
        let mut fl = 0f32;
        for k in 0..bins {
            let l = (1.0 + 100.0 * mag[k]).ln();
            fl += (l - prev[k]).max(0.0);
            prev[k] = l;
        }
        flux.push(fl);
        // тишину в средние признаки не берём — она тянула бы всё к нулю
        if r < 1e-3 {
            continue;
        }
        let pow: Vec<f32> = mag.iter().map(|m| m * m).collect();
        let total: f32 = pow.iter().sum::<f32>().max(1e-12);
        let w = r as f64;
        let cen = (0..bins).map(|k| k as f32 * bin_hz * pow[k]).sum::<f32>() / total;
        let mut acc = 0f32;
        let roll = (0..bins).find(|&k| {
            acc += pow[k];
            acc >= 0.85 * total
        });
        let (lg, ar) = mag[1..].iter().fold((0f64, 0f64), |(g, a), &m| (g + (m as f64 + 1e-9).ln(), a + m as f64));
        let flat = ((lg / (bins - 1) as f64).exp() / (ar / (bins - 1) as f64).max(1e-12)).min(1.0);
        cen_sum += cen as f64 * w;
        roll_sum += roll.unwrap_or(bins - 1) as f64 * bin_hz as f64 * w;
        flat_sum += flat * w;
        wsum += w;
        for k in 0..bins {
            let hz = k as f32 * bin_hz;
            bands[if hz < 250.0 { 0 } else if hz < 4000.0 { 1 } else { 2 }] += pow[k] as f64;
            if let Some(p) = pc[k] {
                chroma[p] += mag[k];
            }
        }
        // MFCC: лог мел-энергий → ДКП-II
        let mel: Vec<f32> = bank
            .iter()
            .map(|(first, w)| (w.iter().enumerate().map(|(i, x)| x * pow.get(first + i).copied().unwrap_or(0.0)).sum::<f32>() + 1e-10).ln())
            .collect();
        let mut c = [0f32; MFCC];
        for (n, cn) in c.iter_mut().enumerate() {
            let n = n + 1;
            *cn = mel.iter().enumerate().map(|(m, x)| x * (PI * n as f32 * (m as f32 + 0.5) / MELS as f32).cos()).sum::<f32>() * (2.0 / MELS as f32).sqrt();
        }
        mfcc_frames.push(c);
    }
    if mfcc_frames.len() < 16 {
        bail!("почти тишина");
    }

    // ── темп: автокорреляция огибающей атак ──
    let mean = flux.iter().sum::<f32>() / flux.len() as f32;
    let env: Vec<f32> = flux.iter().map(|x| (x - mean).max(0.0)).collect();
    let ac = |lag: usize| env.iter().zip(&env[lag..]).map(|(a, b)| a * b).sum::<f32>();
    let zero = ac(0).max(1e-9);
    let (lmin, lmax) = ((fps * 60.0 / 200.0) as usize, (fps * 60.0 / 60.0).ceil() as usize);
    let mut best = (0usize, 0f32, 0f32);
    for lag in lmin..=lmax.min(env.len() / 2) {
        let v = ac(lag) / zero;
        // мягко предпочитаем темп около 120: иначе половинный и двойной темп выигрывали бы случайно
        let bpm = 60.0 * fps / lag as f32;
        let wv = v * (-(bpm / 120.0).log2().powi(2) / (2.0 * 0.9f32.powi(2))).exp();
        if wv > best.2 {
            best = (lag, v, wv);
        }
    }
    let pulse = best.1.clamp(0.0, 1.0);
    let bpm = (best.0 > 0 && pulse > 0.08).then(|| {
        // уточняем по параболе через соседние лаги
        let (l, c) = (best.0 as f32, ac(best.0));
        let (a, b) = (ac(best.0 - 1), ac(best.0 + 1));
        let d = a - 2.0 * c + b;
        let off = if d.abs() > 1e-9 { (0.5 * (a - b) / d).clamp(-0.5, 0.5) } else { 0.0 };
        60.0 * fps / (l + off)
    });
    // атаки: локальные максимумы огибающей выше порога
    let thr = mean + percentile(&mut env.clone(), 0.9) * 0.5;
    let onsets = (1..flux.len() - 1).filter(|&i| flux[i] > thr && flux[i] >= flux[i - 1] && flux[i] > flux[i + 1]).count() as f32 / (flux.len() as f32 / fps);

    // ── тональность ──
    let csum: f32 = chroma.iter().sum::<f32>().max(1e-9);
    let chroma = chroma.map(|c| c / csum);
    let mut scores: Vec<(f32, u8, bool)> = (0..12u8)
        .flat_map(|t| {
            let rot = |p: &[f32; 12]| -> Vec<f32> { (0..12).map(|i| p[(i + 12 - t as usize) % 12]).collect() };
            [(pearson(&chroma, &rot(&MAJOR)), t, false), (pearson(&chroma, &rot(&MINOR)), t, true)]
        })
        .collect();
    scores.sort_by(|a, b| b.0.total_cmp(&a.0));
    let key_conf = (scores[0].0 - scores[1].0).clamp(0.0, 1.0) * 4.0;
    let tonal = scores[0].0 > 0.45;

    // ── громкость и энергия ──
    let all_rms = (pcm.iter().map(|x| x * x).sum::<f32>() / pcm.len() as f32).sqrt();
    let win = (fps * 3.0) as usize;
    let mut wins: Vec<f32> = rms.chunks(win.max(1)).map(|c| db((c.iter().map(|x| x * x).sum::<f32>() / c.len() as f32).sqrt())).filter(|d| *d > -60.0).collect();
    let dyn_db = percentile(&mut wins.clone(), 0.95) - percentile(&mut wins, 0.10);
    let rms_db = db(all_rms);
    let energy = (((rms_db + 30.0) / 24.0).clamp(0.0, 1.0) * 0.7 + (onsets / 6.0).clamp(0.0, 1.0) * 0.3).clamp(0.0, 1.0);

    // ── тембр ──
    let n = mfcc_frames.len() as f32;
    let mut mfcc = [0f32; MFCC];
    let mut sd = [0f32; MFCC];
    for c in &mfcc_frames {
        for i in 0..MFCC {
            mfcc[i] += c[i] / n;
        }
    }
    for c in &mfcc_frames {
        for i in 0..MFCC {
            sd[i] += (c[i] - mfcc[i]).powi(2) / n;
        }
    }
    let sd = sd.map(f32::sqrt);

    let btot = bands.iter().sum::<f64>().max(1e-12);
    let w = wsum.max(1e-12);
    Ok(Dna {
        v: VERSION,
        secs: pcm.len() as f32 / SR as f32,
        bpm,
        pulse,
        onsets,
        key: tonal.then_some(scores[0].1),
        minor: tonal && scores[0].2,
        key_conf: if tonal { key_conf.min(1.0) } else { 0.0 },
        rms_db,
        peak_db: db(peak),
        dyn_db,
        energy,
        centroid: (cen_sum / w) as f32,
        rolloff: (roll_sum / w) as f32,
        flatness: (flat_sum / w) as f32,
        bands: [(bands[0] / btot) as f32, (bands[1] / btot) as f32, (bands[2] / btot) as f32],
        mfcc,
        mfcc_sd: sd,
        chroma,
        sections: sections(&mfcc_frames_secs(&mfcc_frames, &rms, fps)),
    })
}

/// MFCC, усреднённые по секундам (тишину пропущенных кадров не считаем — индексы те же, что у rms)
fn mfcc_frames_secs(m: &[[f32; MFCC]], rms: &[f32], fps: f32) -> Vec<[f32; MFCC]> {
    // m содержит только громкие кадры; для границ нужна временная ось — раскладываем по секундам по rms
    let per = fps.round() as usize;
    let mut out = Vec::new();
    let mut it = m.iter();
    for chunk in rms.chunks(per) {
        let loud = chunk.iter().filter(|r| **r >= 1e-3).count();
        let mut acc = [0f32; MFCC];
        let mut k = 0;
        for c in it.by_ref().take(loud) {
            for i in 0..MFCC {
                acc[i] += c[i];
            }
            k += 1;
        }
        out.push(if k > 0 { acc.map(|x| x / k as f32) } else { acc });
    }
    out
}

/// Границы секций: где тембр 8 секунд до и 8 после различается сильнее всего (пики новизны)
fn sections(sec: &[[f32; MFCC]]) -> Vec<f32> {
    const W: usize = 8;
    if sec.len() < W * 3 {
        return Vec::new();
    }
    let mean = |s: &[[f32; MFCC]]| {
        let mut m = [0f32; MFCC];
        for c in s {
            for i in 0..MFCC {
                m[i] += c[i] / s.len() as f32;
            }
        }
        m
    };
    let nov: Vec<f32> = (W..sec.len() - W)
        .map(|t| {
            let (a, b) = (mean(&sec[t - W..t]), mean(&sec[t..t + W]));
            a.iter().zip(&b).map(|(x, y)| (x - y).powi(2)).sum::<f32>().sqrt()
        })
        .collect();
    let mut sorted = nov.clone();
    let thr = percentile(&mut sorted, 0.8);
    let mut out: Vec<f32> = Vec::new();
    for i in 1..nov.len().saturating_sub(1) {
        let t = (i + W) as f32;
        if nov[i] > thr && nov[i] >= nov[i - 1] && nov[i] >= nov[i + 1] && out.last().is_none_or(|&l| t - l >= 12.0) {
            out.push(t);
        }
    }
    out
}

// ── сходство ──

#[derive(Clone, Debug, Default, Serialize)]
pub struct Similarity {
    pub rhythm: f32,
    pub harmony: f32,
    pub timbre: f32,
    pub energy: f32,
    pub spectrum: f32,
    pub total: f32,
}

pub fn compare(a: &Dna, b: &Dna) -> Similarity {
    let tempo = match (a.bpm, b.bpm) {
        // половинный и двойной темп — тот же грув
        (Some(x), Some(y)) => {
            let r = [0.5f32, 1.0, 2.0].iter().map(|k| (x * k / y).ln().abs()).fold(f32::MAX, f32::min);
            (-r / 0.1).exp()
        }
        (None, None) => 1.0,
        _ => 0.2,
    };
    let rhythm = tempo * 0.6 + (1.0 - (a.pulse - b.pulse).abs()) * 0.2 + (1.0 - ((a.onsets - b.onsets).abs() / 4.0).min(1.0)) * 0.2;
    let harmony = (pearson(&a.chroma, &b.chroma) + 1.0) / 2.0;
    let d: f32 = (0..MFCC).map(|i| (a.mfcc[i] - b.mfcc[i]).powi(2) / (a.mfcc_sd[i].powi(2) + b.mfcc_sd[i].powi(2) + 1e-3)).sum::<f32>() / MFCC as f32;
    let timbre = (-d.sqrt() * 1.2).exp();
    let energy = (1.0 - (a.energy - b.energy).abs()) * 0.6 + (1.0 - ((a.rms_db - b.rms_db).abs() / 12.0).min(1.0)) * 0.2 + (1.0 - ((a.dyn_db - b.dyn_db).abs() / 12.0).min(1.0)) * 0.2;
    let bands = 1.0 - a.bands.iter().zip(&b.bands).map(|(x, y)| (x - y).abs()).sum::<f32>() / 2.0;
    let spectrum = (-(a.centroid.max(1.0) / b.centroid.max(1.0)).ln().abs() * 2.0).exp() * 0.5 + bands * 0.3 + (1.0 - (a.flatness - b.flatness).abs()) * 0.2;
    let total = rhythm * 0.25 + harmony * 0.2 + timbre * 0.3 + energy * 0.15 + spectrum * 0.1;
    Similarity { rhythm, harmony, timbre, energy, spectrum, total }
}

/// Отпечаток группы треков (альбом, артист): среднее по признакам. Темп — медиана, тональность — по средней хроме
pub fn mean(v: &[&Dna]) -> Option<Dna> {
    if v.is_empty() {
        return None;
    }
    let n = v.len() as f32;
    let avg = |f: &dyn Fn(&Dna) -> f32| v.iter().map(|d| f(d)).sum::<f32>() / n;
    let mut bpms: Vec<f32> = v.iter().filter_map(|d| d.bpm).collect();
    let mut out = Dna {
        v: VERSION,
        secs: v.iter().map(|d| d.secs).sum(),
        bpm: (bpms.len() * 2 >= v.len()).then(|| percentile(&mut bpms, 0.5)),
        pulse: avg(&|d| d.pulse),
        onsets: avg(&|d| d.onsets),
        rms_db: avg(&|d| d.rms_db),
        peak_db: avg(&|d| d.peak_db),
        dyn_db: avg(&|d| d.dyn_db),
        energy: avg(&|d| d.energy),
        centroid: avg(&|d| d.centroid),
        rolloff: avg(&|d| d.rolloff),
        flatness: avg(&|d| d.flatness),
        ..Default::default()
    };
    for i in 0..3 {
        out.bands[i] = avg(&|d| d.bands[i]);
    }
    for i in 0..MFCC {
        out.mfcc[i] = avg(&|d| d.mfcc[i]);
        out.mfcc_sd[i] = avg(&|d| d.mfcc_sd[i]);
    }
    for i in 0..12 {
        out.chroma[i] = avg(&|d| d.chroma[i]);
    }
    Some(out)
}

// ── хранилище ──

fn file() -> PathBuf {
    crate::model::Store::dir().join("dna.json")
}

fn store() -> &'static Mutex<HashMap<String, Dna>> {
    static S: OnceLock<Mutex<HashMap<String, Dna>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(std::fs::read(file()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()))
}

pub fn get(key: &str) -> Option<Dna> {
    store().lock().unwrap().get(key).filter(|d| d.v == VERSION).cloned()
}

pub fn all() -> HashMap<String, Dna> {
    store().lock().unwrap().iter().filter(|(_, d)| d.v == VERSION).map(|(k, d)| (k.clone(), d.clone())).collect()
}

pub fn put(key: &str, d: Dna) {
    let mut s = store().lock().unwrap();
    s.insert(key.to_string(), d);
    if let Ok(b) = serde_json::to_vec(&*s) {
        let tmp = file().with_extension("tmp");
        if std::fs::write(&tmp, b).is_ok() {
            let _ = std::fs::rename(tmp, file());
        }
    }
}

/// Среднее звучание с весами (время прослушивания): «твой обычный звук»
pub fn weighted_mean(v: &[(Dna, u64)]) -> Option<Dna> {
    if v.len() < 3 {
        return None;
    }
    // повторяем отпечатки пропорционально весу (не больше 20 раз) — mean считает равные доли
    let total: u64 = v.iter().map(|x| x.1).sum::<u64>().max(1);
    let mut refs = Vec::new();
    for (d, w) in v {
        let k = ((*w as f64 / total as f64) * 200.0).round().clamp(1.0, 20.0) as usize;
        for _ in 0..k {
            refs.push(d);
        }
    }
    mean(&refs)
}

// ── обмен между устройствами аккаунта: Realtime Database, узел dna/<uid>/<ключ трека> ──
// ПК разбирает кэш, телефон — скачанное; отпечатки сходятся у всех. Локальные файлы не отправляем

fn remote_key(k: &str) -> Option<&str> {
    (!k.starts_with("lc:") && !k.contains(['.', '$', '#', '[', ']', '/'])).then_some(k)
}

fn dna_base() -> anyhow::Result<(String, String)> {
    let db = crate::connect::db().context("нет базы")?;
    let c = crate::cloud::cfg().context("облако не настроено")?;
    let a = crate::cloud::account();
    if !a.verified || a.uid.is_empty() {
        anyhow::bail!("NOT_VERIFIED");
    }
    Ok((format!("{db}/dna/{}", a.uid), crate::cloud::auth_token(&c)?))
}

/// Свои отпечатки — в облако, чужие (с других устройств) — к себе. Сколько пришло нового
pub fn sync() -> anyhow::Result<usize> {
    let (base, tok) = dna_base()?;
    let remote: HashMap<String, Dna> = crate::util::agent().get(&format!("{base}.json")).query("auth", &tok).call()?.into_json::<Option<HashMap<String, Dna>>>()?.unwrap_or_default();
    let mut got = 0;
    {
        let mut s = store().lock().unwrap();
        for (k, d) in &remote {
            if d.v == VERSION && s.get(k).is_none_or(|x| x.v != VERSION) {
                s.insert(k.clone(), d.clone());
                got += 1;
            }
        }
    }
    if got > 0 {
        let s = store().lock().unwrap();
        if let Ok(b) = serde_json::to_vec(&*s) {
            let _ = std::fs::write(file(), b);
        }
    }
    let mine: serde_json::Map<String, serde_json::Value> = all()
        .into_iter()
        .filter(|(k, _)| remote_key(k).is_some() && !remote.contains_key(k))
        .filter_map(|(k, d)| Some((k, serde_json::to_value(d).ok()?)))
        .collect();
    if !mine.is_empty() {
        crate::util::agent().request("PATCH", &format!("{base}.json")).query("auth", &tok).send_json(serde_json::Value::Object(mine))?;
    }
    Ok(got)
}

/// Разобрать файлы по очереди в фоне, по одному, с низким приоритетом. Уже разобранные пропускаются.
/// done вызывается после каждого трека (обновить экран)
pub fn analyze_files(jobs: Vec<(String, PathBuf)>, done: impl Fn(&str) + Send + 'static) {
    static BUSY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let jobs: Vec<_> = jobs.into_iter().filter(|(k, _)| get(k).is_none()).collect();
    if jobs.is_empty() || BUSY.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        #[cfg(target_os = "linux")]
        unsafe {
            // приоритет только этого потока (и ffmpeg, который он запустит)
            libc::setpriority(libc::PRIO_PROCESS, libc::gettid() as libc::id_t, 19);
        }
        for (key, path) in jobs {
            let t = std::time::Instant::now();
            match decode(&path).and_then(|pcm| analyze(&pcm)) {
                Ok(d) => {
                    crate::logln!("DNA {key}: {:?} BPM, {:?}, {:.0} мс", d.bpm.map(|b| b.round()), d.key_name(), t.elapsed().as_millis());
                    put(&key, d);
                    done(&key);
                }
                Err(e) => crate::logln!("DNA {key}: {e:#}"),
            }
        }
        if let Err(e) = sync() {
            crate::logln!("DNA в облако: {e:#}");
        }
        BUSY.store(false, std::sync::atomic::Ordering::SeqCst);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Метроном: щелчок (затухающий шум) каждые 60/bpm секунд поверх тихого тона
    fn click_track(bpm: f32, secs: f32) -> Vec<f32> {
        let n = (SR as f32 * secs) as usize;
        let period = (SR as f32 * 60.0 / bpm) as usize;
        let mut seed = 12345u32;
        (0..n)
            .map(|i| {
                let ph = i % period;
                seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
                let noise = (seed >> 9) as f32 / (1u32 << 23) as f32 * 2.0 - 1.0;
                let click = if ph < 800 { noise * (-(ph as f32) / 120.0).exp() * 0.8 } else { 0.0 };
                click + 0.05 * (2.0 * PI * 220.0 * i as f32 / SR as f32).sin()
            })
            .collect()
    }

    /// Аккорд из нот (номера полутонов от A4), с гармониками
    fn chord(notes: &[i32], secs: f32) -> Vec<f32> {
        let n = (SR as f32 * secs) as usize;
        (0..n)
            .map(|i| {
                let t = i as f32 / SR as f32;
                notes
                    .iter()
                    .map(|&s| {
                        let f = 440.0 * 2f32.powf(s as f32 / 12.0);
                        (1..=4).map(|h| (2.0 * PI * f * h as f32 * t).sin() / h as f32).sum::<f32>()
                    })
                    .sum::<f32>()
                    * 0.1
            })
            .collect()
    }

    #[test]
    fn finds_tempo() {
        for bpm in [90.0, 120.0, 140.0] {
            let d = analyze(&click_track(bpm, 30.0)).unwrap();
            let got = d.bpm.expect("темп не найден");
            assert!((got - bpm).abs() < 2.0, "ждал {bpm}, вышло {got}");
            assert!(d.pulse > 0.3, "бит должен быть чётким, pulse {}", d.pulse);
        }
    }

    #[test]
    fn finds_key() {
        // до мажор: C E G (от A4: -9, -5, -2), потом F A C и G B D — каденция
        let mut pcm = chord(&[-9, -5, -2], 6.0);
        pcm.extend(chord(&[-4, 0, 3], 3.0));
        pcm.extend(chord(&[-2, 2, 5], 3.0));
        pcm.extend(chord(&[-9, -5, -2], 6.0));
        let d = analyze(&pcm).unwrap();
        assert_eq!(d.key_name().as_deref(), Some("C"), "chroma {:?}", d.chroma);
        // ля минор: A C E
        let d = analyze(&chord(&[0, 3, 7], 12.0)).unwrap();
        assert_eq!(d.key_name().as_deref(), Some("Am"), "chroma {:?}", d.chroma);
    }

    #[test]
    fn similarity_is_reasonable() {
        let a = analyze(&click_track(120.0, 30.0)).unwrap();
        let b = analyze(&click_track(121.0, 30.0)).unwrap();
        let c = analyze(&chord(&[0, 3, 7], 30.0)).unwrap();
        let ab = compare(&a, &b);
        let ac = compare(&a, &c);
        assert!(ab.total > 0.85, "почти одинаковые: {ab:?}");
        assert!(ac.total < ab.total - 0.2, "метроном и аккорд: {ac:?}");
        assert!(ab.rhythm > 0.85 && ac.rhythm < 0.5, "ab {ab:?} ac {ac:?}");
        // симметрия
        assert!((compare(&c, &a).total - ac.total).abs() < 1e-4);
    }

    #[test]
    fn silence_is_an_error() {
        assert!(analyze(&vec![0.0; SR as usize * 10]).is_err());
    }
}
