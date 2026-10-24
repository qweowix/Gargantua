//! Эквалайзер: цепочка фильтров ffmpeg для mpv (af). Звук через сам плеер не гоняем — mpv делает всё сам.

pub const BANDS: [u32; 10] = [31, 62, 125, 250, 500, 1000, 2000, 4000, 8000, 16000];
pub const MAX_DB: f32 = 12.0;

/// (название ru, название en, усиления по полосам в дБ)
pub const PRESETS: [(&str, &str, [f32; 10]); 13] = [
    ("Ровно", "Flat", [0.0; 10]),
    ("Bass", "Bass", [6.0, 5.0, 4.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
    ("Super Bass", "Super Bass", [10.0, 9.0, 7.0, 4.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
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
/// Если полосы подняты — заранее опускаем общий уровень, иначе громкие места хрипят от перегруза
pub fn af(gains: &[f32]) -> String {
    let mut chain: Vec<String> = vec![];
    let boost = gains.iter().cloned().fold(0.0f32, f32::max);
    let bands = BANDS.iter().zip(gains).filter(|(_, g)| g.abs() >= 0.05);
    if bands.clone().next().is_some() && boost > 0.0 {
        chain.push(format!("volume=volume={:.1}dB", -boost * 0.7));
    }
    chain.extend(bands.map(|(f, g)| format!("equalizer=f={f}:t=o:w=1:g={g:.1}")));
    if chain.is_empty() {
        return String::new();
    }
    format!("lavfi=[{}]", chain.join(","))
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
        assert_eq!(s, "lavfi=[volume=volume=-4.2dB,equalizer=f=31:t=o:w=1:g=6.0,equalizer=f=16000:t=o:w=1:g=-3.0]");
        assert_eq!(super::preset_of(&super::PRESETS[1].2), 1);
        assert_eq!(super::preset_of(&[1.0; 10]), -1);
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
