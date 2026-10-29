//! Эквалайзер: цепочка фильтров ffmpeg для mpv (af). Звук через сам плеер не гоняем — mpv делает всё сам.

pub const BANDS: [u32; 10] = [31, 62, 125, 250, 500, 1000, 2000, 4000, 8000, 16000];
pub const MAX_DB: f32 = 12.0;

/// (название ru, название en, усиления по полосам в дБ)
pub const PRESETS: [(&str, &str, [f32; 10]); 13] = [
    ("Ровно", "Flat", [0.0; 10]),
    // бас — это 30–90 Гц; 125–250 не трогаем, иначе звук «в бочке»
    ("Bass", "Bass", [5.0, 6.0, 3.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
    ("Super Bass", "Super Bass", [8.0, 9.0, 5.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.5, 1.0]),
    ("Treble", "Treble", [0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 3.0, 4.0, 5.0, 6.0]),
    ("Vocal", "Vocal", [-2.0, -2.0, -1.0, 1.0, 3.0, 4.0, 3.0, 1.0, 0.0, -1.0]),
    ("Rock", "Rock", [4.0, 3.0, 2.0, 0.0, -1.0, -1.0, 1.0, 3.0, 4.0, 4.0]),
    ("Pop", "Pop", [-1.0, 1.0, 3.0, 4.0, 3.0, 0.0, -1.0, -1.0, 0.0, 1.0]),
    ("Jazz", "Jazz", [3.0, 2.0, 1.0, 2.0, -1.0, -1.0, 0.0, 1.0, 2.0, 3.0]),
    ("Classical", "Classical", [4.0, 3.0, 2.0, 1.0, -1.0, -1.0, 0.0, 2.0, 3.0, 4.0]),
    ("Hip-Hop", "Hip-Hop", [5.0, 4.0, 1.0, 3.0, -1.0, -1.0, 1.0, 0.0, 2.0, 3.0]),
    ("Electronic", "Electronic", [5.0, 4.0, 1.0, 0.0, -2.0, 0.0, 1.0, 3.0, 4.0, 4.0]),
    ("Acoustic", "Acoustic", [3.0, 3.0, 2.0, 1.0, 1.0, 1.0, 2.0, 3.0, 2.0, 1.0]),
    ("Loudness", "Loudness", [5.0, 3.0, 0.0, 0.0, -1.0, 0.0, -1.0, 0.0, 4.0, 5.0]),
];

/// Строка для `af` в mpv. Пусто — фильтров нет вообще (ровный звук ничего не стоит).
/// Полосы — колокола шириной в октаву и перекрываются: соседние складываются, и бас выходил выше
/// ползунков (Bass +6 давал +7 на 31 Гц и +5.5 на 125). Поэтому усиления подгоняем по настоящей
/// АЧХ, чтобы кривая шла ровно через ползунки, а общий уровень опускаем на её реальный пик — без клипа
pub fn af(gains: &[f32]) -> String {
    if gains.iter().all(|g| g.abs() < 0.05) {
        return String::new();
    }
    let target: Vec<f64> = gains.iter().map(|&g| g as f64).collect();
    let mut set = target.clone();
    for _ in 0..30 {
        for (k, f) in BANDS.iter().enumerate() {
            set[k] = (set[k] + target[k] - response(&set, *f as f64)).clamp(-24.0, 24.0);
        }
    }
    // пик по всему слышимому диапазону с шагом 1/12 октавы
    let peak = (0..=120).map(|i| response(&set, 20.0 * 2f64.powf(i as f64 / 12.0))).fold(0.0, f64::max);
    let mut chain = vec![];
    if peak > 0.05 {
        chain.push(format!("volume=volume={:.1}dB", -peak));
    }
    chain.extend(BANDS.iter().zip(&set).filter(|(_, g)| g.abs() >= 0.05).map(|(f, g)| format!("equalizer=f={f}:t=o:w=1:g={g:.1}")));
    format!("lavfi=[{}]", chain.join(","))
}

/// АЧХ цепочки колоколов (дБ) на частоте f — те же формулы RBJ, что у фильтра equalizer в ffmpeg
fn response(gains: &[f64], f: f64) -> f64 {
    const RATE: f64 = 48000.0;
    let w = 2.0 * std::f64::consts::PI * f / RATE;
    BANDS
        .iter()
        .zip(gains)
        .filter(|(_, g)| g.abs() >= 0.05)
        .map(|(&fc, &g)| {
            let w0 = 2.0 * std::f64::consts::PI * fc as f64 / RATE;
            let a = 10f64.powf(g / 40.0);
            let alpha = w0.sin() * (2f64.ln() / 2.0 * w0 / w0.sin()).sinh();
            let (b, d) = ([1.0 + alpha * a, -2.0 * w0.cos(), 1.0 - alpha * a], [1.0 + alpha / a, -2.0 * w0.cos(), 1.0 - alpha / a]);
            let mag = |c: [f64; 3]| {
                let (re, im) = (c[0] + c[1] * w.cos() + c[2] * (2.0 * w).cos(), -c[1] * w.sin() - c[2] * (2.0 * w).sin());
                re * re + im * im
            };
            10.0 * (mag(b) / mag(d)).log10()
        })
        .sum()
}

/// Какой пресет совпадает с текущими полосами (-1 — свои настройки)
pub fn preset_of(gains: &[f32]) -> i32 {
    PRESETS.iter().position(|(_, _, p)| p.iter().zip(gains).all(|(a, b)| (a - b).abs() < 0.05)).map_or(-1, |i| i as i32)
}

#[cfg(test)]
mod tests {
    #[test]
    fn filter_string() {
        assert_eq!(super::af(&[0.0; 10]), "");
        let s = super::af(&[6.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -3.0]);
        assert!(s.starts_with("lavfi=[volume=volume=-") && s.contains("equalizer=f=31:t=o:w=1:g=") && s.contains("equalizer=f=16000"), "{s}");
        assert_eq!(super::preset_of(&super::PRESETS[1].2), 1);
        assert_eq!(super::preset_of(&[1.0; 10]), -1);
    }

    /// Настоящая кривая идёт через ползунки (а не выше, как у сложенных колоколов)
    #[test]
    fn curve_hits_sliders() {
        for (_, _, g) in super::PRESETS.iter().skip(1) {
            let s = super::af(g);
            let set: Vec<f64> = super::BANDS.iter().map(|f| s.split(&format!("equalizer=f={f}:t=o:w=1:g=")).nth(1).map_or(0.0, |r| r.split([',', ']']).next().unwrap().parse().unwrap())).collect();
            for (k, f) in super::BANDS.iter().enumerate() {
                let got = super::response(&set, *f as f64);
                assert!((got - g[k] as f64).abs() < 0.3, "{f} Гц: {got:.2} вместо {}", g[k]);
            }
        }
    }

    /// mpv сам проверяет, что такая цепочка фильтров валидна (нужен mpv в системе)
    #[test]
    #[ignore]
    fn mpv_accepts_filter() {
        let mpv = crate::util::find_bin("mpv").unwrap();
        for (_, _, g) in super::PRESETS.iter().skip(1) {
            let out = std::process::Command::new(&mpv)
                .args(["--no-config", "--ao=null", "--no-video", "--length=0.3", "av://lavfi:sine=f=440:d=1"])
                .arg(format!("--af={}", super::af(g)))
                .arg("--speed=1.5")
                .output()
                .unwrap();
            let err = String::from_utf8_lossy(&out.stderr);
            assert!(out.status.success() && !err.contains("rror"), "{err}");
        }
    }
}
