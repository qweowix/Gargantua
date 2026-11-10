//! Эквалайзер: цепочка фильтров ffmpeg для mpv (af). Звук через сам плеер не гоняем — mpv делает всё сам.

pub const BANDS: [u32; 10] = [31, 62, 125, 250, 500, 1000, 2000, 4000, 8000, 16000];
pub const MAX_DB: f32 = 12.0;
const STRETCH: &str = "finer";

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
/// АЧХ, чтобы кривая шла ровно через ползунки. Опускать весь звук на пик кривой нельзя: голос и середина
/// тоже становятся тише (Bass: -13.3 LUFS против -9.1 без эквалайзера). Опускаем на четверть пика, а
/// остальные перегрузы ловит лимитер (Bass: -9.4 LUFS, пик в норме)
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
    // подгонка полос даёт и на чистых срезах всплеск на доли дБ — перегружать там нечему
    let boosted = peak > 0.5;
    if boosted {
        chain.push(format!("volume=volume={:.1}dB", -peak / 4.0));
    }
    chain.extend(BANDS.iter().zip(&set).filter(|(_, g)| g.abs() >= 0.05).map(|(f, g)| format!("equalizer=f={f}:t=o:w=1:g={g:.1}")));
    if boosted {
        chain.push("alimiter=level=disabled:limit=0.97:attack=5:release=60:asc=1".into());
    }
    format!("lavfi=[{}]", chain.join(","))
}

/// Все фильтры для mpv: эквалайзер и, если скорость меняют с сохранением тона, растяжение через rubberband.
/// Своё растяжение mpv (scaletempo2) сдвигает атаки на случайные ±13 мс — ритм на замедлении рвётся;
/// rubberband держит их на месте (замер на щелчках: ±0.7 мс, как без растяжения)
pub fn chain(gains: &[f32], speed: f32, keep_pitch: bool, viz: bool) -> String {
    let mut parts = vec![];
    let eq = af(gains);
    if !eq.is_empty() {
        parts.push(eq);
    }
    if viz {
        parts.push(viz_graph());
    }
    if keep_pitch && (speed - 1.0).abs() >= 0.001 {
        parts.push(format!("rubberband=engine={STRETCH}"));
    }
    parts.join(",")
}

/// Сколько полос у спектра (столбиков на экране вдвое больше — между соседними полосами сглаживаем)
pub const VIZ_BANDS: usize = 16;

/// Спектр для панели «Сейчас играет» — силами самого mpv, без внешних программ. Звук идёт дальше нетронутым
/// (замер: бит в бит), а его копия — моно 12 кГц — режется на полосы, и пик каждой ~31 раз в секунду
/// печатается в stdout mpv вместе со временем трека (player.rs читает и отдаёт по позиции).
/// Стоит ~2% одного ядра. Включать и выключать на ходу нельзя: смена фильтров даёт щелчок
fn viz_graph() -> String {
    let (lo, hi) = (50.0f64, 5200.0f64);
    let ins: String = (0..VIZ_BANDS).map(|i| format!("[i{i}]")).collect();
    let bands: String = (0..VIZ_BANDS)
        .map(|i| format!("[i{i}]bandpass=f={:.0}:t=o:w=0.6[b{i}];", lo * (hi / lo).powf(i as f64 / (VIZ_BANDS - 1) as f64)))
        .collect();
    let outs: String = (0..VIZ_BANDS).map(|i| format!("[b{i}]")).collect();
    format!(
        "lavfi=[asplit[m][s];[s]aresample=12000,pan=mono|c0=0.5*c0+0.5*c1,asetnsamples=n=384,asplit={VIZ_BANDS}{ins};{bands}{outs}amerge=inputs={VIZ_BANDS},\
         astats=metadata=1:reset=1:measure_perchannel=Peak_level:measure_overall=none,ametadata=mode=print:file=pipe\\\\:1:direct=1,anullsink;[m]anull]"
    )
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
        assert!(s.starts_with("lavfi=[volume=volume=-") && s.contains("equalizer=f=31:t=o:w=1:g=") && s.contains("equalizer=f=16000") && s.contains(",alimiter="), "{s}");
        // только срезы: перегружать нечему, лимитер и запас не нужны
        let cut = super::af(&[-6.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        assert!(!cut.contains("alimiter") && !cut.contains("volume="), "{cut}");
        assert_eq!(super::preset_of(&super::PRESETS[1].2), 1);
        assert_eq!(super::preset_of(&[1.0; 10]), -1);
    }

    #[test]
    fn stretch_only_when_needed() {
        assert_eq!(super::chain(&[0.0; 10], 1.0, true, false), "");
        assert_eq!(super::chain(&[0.0; 10], 0.85, false, false), "");
        assert!(super::chain(&[0.0; 10], 0.85, true, false).starts_with("rubberband="));
        let both = super::chain(&super::PRESETS[1].2, 0.85, true, false);
        assert!(both.starts_with("lavfi=[") && both.contains("],rubberband="), "{both}");
        // спектр — после эквалайзера (показывает то, что слышно) и до растяжения (время в нём — время трека)
        let all = super::chain(&super::PRESETS[1].2, 0.85, true, true);
        assert!(all.contains("],lavfi=[asplit[m][s]") && all.contains("pipe\\\\:1") && all.ends_with("[m]anull],rubberband=engine=finer"), "{all}");
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
                .arg(format!("--af={}", super::chain(g, 1.5, true, true)))
                .arg("--speed=1.5")
                .output()
                .unwrap();
            let err = String::from_utf8_lossy(&out.stderr);
            assert!(out.status.success() && !err.contains("rror"), "{err}");
        }
    }
}
