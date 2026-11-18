//! Экран «Мой вкус» на ПК: отпечаток вкуса (taste.rs) → модели Slint, созвездие жанров и артистов,
//! публичный профиль в Firestore (profiles/<код>) и сравнение с другом по его коду.

use std::collections::HashSet;
use std::rc::Rc;
use std::sync::atomic::Ordering;

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::graph::Graph;
use crate::taste::{self, Fingerprint};
use crate::universe::genre_color;
use crate::{AppWindow, DnaPick, RadarLabel, Taste, TasteBar, TasteCluster, TasteCombo, UNode, LANG};

fn model<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
    ModelRc::from(Rc::new(VecModel::from(v)))
}

fn color(c: [u8; 3]) -> slint::Color {
    slint::Color::from_rgb_u8(c[0], c[1], c[2])
}

fn pct(x: f32) -> String {
    format!("{:.0} %", x * 100.0)
}

fn hours(ms: u64) -> String {
    let h = ms / 3_600_000;
    if h >= 1 {
        tr!("{} ч", "{} h", "{} год", "{} h", h)
    } else {
        tr!("{} мин", "{} min", "{} хв", "{} min", ms / 60_000)
    }
}

fn bar(label: impl Into<SharedString>, value: impl Into<SharedString>, frac: f32, sub: impl Into<SharedString>) -> TasteBar {
    TasteBar { label: label.into(), value: value.into(), frac, sub: sub.into(), frac2: 0.0 }
}

pub fn fill(ui: &AppWindow, fp: &Fingerprint, g: &Graph) {
    let t = ui.global::<Taste>();
    t.set_empty(fp.total_ms == 0);
    if fp.total_ms == 0 {
        return;
    }
    let top_g = fp.genres.iter().take(2).map(|s| format!("{} ({})", s.name, pct(s.w))).collect::<Vec<_>>().join(", ");
    t.set_summary(
        tr!(
            "{} музыки, {} треков, {} артистов{}. Разнообразие артистов {:.0} %.",
            "{} of music, {} tracks, {} artists{}. Artist diversity {:.0} %.",
            "{} музики, {} треків, {} артистів{}. Різноманіття артистів {:.0} %.",
            "{} hudby, {} skladieb, {} interpretov{}. Rozmanitosť interpretov {:.0} %.",
            hours(fp.total_ms),
            fp.tracks,
            fp.artists.len().max(if fp.artists.len() == 30 { 30 } else { 0 }),
            if top_g.is_empty() { String::new() } else { tr!(". Главное: {}", ". Mostly: {}", ". Головне: {}", ". Najmä: {}", top_g) },
            fp.artist_diversity * 100.0
        )
        .into(),
    );
    t.set_coverage(
        if fp.genres.is_empty() {
            tr!(
                "Жанров пока нет: их дают MusicBrainz. Нажми «Подтянуть связи» во Вселенной.",
                "No genres yet: they come from MusicBrainz. Press “Fetch links” in the Universe.",
                "Жанрів поки немає: їх дає MusicBrainz. Натисни «Підтягнути зв'язки» у Всесвіті.",
                "Zatiaľ žiadne žánre: dodáva ich MusicBrainz. Stlač „Doplniť väzby“ vo Vesmíre."
            )
        } else {
            tr!("Жанры известны для {} времени (MusicBrainz)", "Genres known for {} of the time (MusicBrainz)", "Жанри відомі для {} часу (MusicBrainz)", "Žánre známe pre {} času (MusicBrainz)", pct(fp.genre_coverage))
        }
        .into(),
    );
    let gmax = fp.genres.first().map_or(1.0, |s| s.w).max(1e-3);
    t.set_genres(model(fp.genres.iter().take(12).map(|s| bar(s.name.as_str(), pct(s.w), s.w / gmax, hours(s.ms))).collect()));
    t.set_combos(model(
        fp.combos
            .iter()
            .map(|c| {
                let shares: Vec<String> = c.genres.iter().filter_map(|gn| fp.genres.iter().find(|s| &s.name == gn)).map(|s| format!("{} {}", s.name, pct(s.w))).collect();
                TasteCombo {
                    title: c.genres.join(" + ").into(),
                    why: tr!(
                        "{} твоего времени, а вместе их сочетают {} из {} твоих артистов",
                        "{} of your time, yet only {} of your {} artists combine them",
                        "{} твого часу, а разом їх поєднують {} з {} твоїх артистів",
                        "{} tvojho času, a spolu ich spája {} z {} tvojich interpretov",
                        shares.join(", "),
                        c.together,
                        c.of
                    )
                    .into(),
                    strength: c.strength,
                }
            })
            .collect(),
    ));
    t.set_note(
        if fp.genres.is_empty() {
            tr!("Для сочетаний нужны жанры — нажми «Подтянуть связи» во Вселенной.", "Combinations need genres — press “Fetch links” in the Universe.", "Для поєднань потрібні жанри — натисни «Підтягнути зв'язки» у Всесвіті.", "Kombinácie potrebujú žánre — stlač „Doplniť väzby“ vo Vesmíre.")
        } else {
            tr!(
                "Необычных сочетаний не нашлось: твои жанры у твоих же артистов и так звучат вместе.",
                "No unusual combinations: your genres already go together in your artists.",
                "Незвичних поєднань не знайшлося: твої жанри у твоїх же артистів і так звучать разом.",
                "Nezvyčajné kombinácie sa nenašli: tvoje žánre u tvojich interpretov už znejú spolu."
            )
        }
        .into(),
    );
    t.set_clusters(model(fp.clusters.iter().map(|c| TasteCluster { name: c.name.as_str().into(), artists: c.artists.join(", ").into(), frac: c.w, c: color(genre_color(&c.name)) }).collect()));

    // звучание
    let s = &fp.sound;
    let mut sound = Vec::new();
    let cov = tr!("по {} трекам · {} времени", "from {} tracks · {} of the time", "за {} треками · {} часу", "z {} skladieb · {} času", s.tracks, pct(s.coverage));
    if let Some(m) = &s.mean {
        if let Some((a, b, c)) = s.tempo {
            sound.push(bar(tr!("Темп", "Tempo", "Темп", "Tempo"), format!("{b:.0} BPM"), (b - 60.0) / 140.0, format!("{a:.0}–{c:.0} BPM")));
        }
        if let Some(mi) = s.minor {
            sound.push(bar(tr!("Минор", "Minor", "Мінор", "Mol"), pct(mi), mi, s.keys.iter().map(|k| k.name.clone()).collect::<Vec<_>>().join(" ")));
        }
        sound.push(bar(tr!("Энергия", "Energy", "Енергія", "Energia"), pct(s.energy), s.energy, ""));
        sound.push(bar(tr!("Чёткость бита", "Beat clarity", "Чіткість біту", "Zreteľnosť beatu"), pct(s.pulse), s.pulse, ""));
        sound.push(bar(tr!("Яркость", "Brightness", "Яскравість", "Jas"), format!("{:.0} Hz", s.brightness), s.brightness / 4000.0, ""));
        sound.push(bar(tr!("Покрытие", "Coverage", "Покриття", "Pokrytie"), pct(s.coverage), s.coverage, cov));
        let ang = |k: usize| -std::f32::consts::FRAC_PI_2 + k as f32 * std::f32::consts::FRAC_PI_4;
        let v = [m.energy, m.pulse, m.bpm.map_or(0.0, |b| (b - 60.0) / 140.0), m.centroid / 4000.0, m.flatness * 3.0, m.dyn_db / 18.0, m.onsets / 8.0, m.bands[0] * 2.0];
        let mut p = String::new();
        for (k, x) in v.iter().enumerate() {
            let r = 80.0 * x.clamp(0.04, 1.0);
            p += &format!("{}{:.1} {:.1}", if k == 0 { "M" } else { "L" }, 100.0 + r * ang(k).cos(), 100.0 + r * ang(k).sin());
        }
        t.set_radar(model(vec![SharedString::from(p + "Z")]));
        let names = [
            tr!("Энергия", "Energy", "Енергія", "Energia"),
            tr!("Бит", "Beat", "Біт", "Beat"),
            tr!("Темп", "Tempo", "Темп", "Tempo"),
            tr!("Яркость", "Bright", "Яскравість", "Jas"),
            tr!("Шум", "Noise", "Шум", "Šum"),
            tr!("Динамика", "Dynamics", "Динаміка", "Dynamika"),
            tr!("Атаки", "Onsets", "Атаки", "Nástupy"),
            tr!("Низ", "Bass", "Низ", "Basy"),
        ];
        t.set_axes(model(names.iter().enumerate().map(|(k, n)| RadarLabel { text: n.as_str().into(), x: 100.0 + 92.0 * ang(k).cos(), y: 100.0 + 92.0 * ang(k).sin() }).collect()));
    } else {
        t.set_radar(model(Vec::new()));
        t.set_axes(model(Vec::new()));
        sound.push(bar(
            tr!("Нет данных", "No data", "Немає даних", "Žiadne údaje"),
            "",
            0.0,
            tr!("Music DNA ещё не разобрал твои треки: нужен звук на диске (кэш, скачанное)", "Music DNA hasn't analysed your tracks yet: needs audio on disk (cache, downloads)", "Music DNA ще не розібрав твої треки: потрібен звук на диску (кеш, завантажене)", "Music DNA ešte neanalyzovala tvoje skladby: treba zvuk na disku (cache, stiahnuté)"),
        ));
    }
    t.set_sound(model(sound));

    let mut habits = vec![
        bar(tr!("Разнообразие артистов", "Artist diversity", "Різноманіття артистів", "Rozmanitosť interpretov"), pct(fp.artist_diversity), fp.artist_diversity, ""),
        bar(tr!("Повторы", "Repeats", "Повтори", "Opakovania"), format!("{:.1}×", fp.repeat), (fp.repeat / 10.0).min(1.0), tr!("в среднем на трек", "plays per track", "у середньому на трек", "priemerne na skladbu")),
        bar(tr!("Любимое по кругу", "Favourites on repeat", "Улюблене по колу", "Obľúbené dookola"), pct(fp.loyal), fp.loyal, tr!("время на треки, игравшие больше 5 раз", "time on tracks played over 5 times", "час на треки, що грали понад 5 разів", "čas na skladby hrané viac ako 5×")),
    ];
    if !fp.genres.is_empty() {
        habits.insert(1, bar(tr!("Разнообразие жанров", "Genre diversity", "Різноманіття жанрів", "Rozmanitosť žánrov"), pct(fp.genre_diversity), fp.genre_diversity, ""));
    }
    habits.push(match fp.skip_rate {
        Some(r) => bar(tr!("Пропуски", "Skips", "Пропуски", "Preskočenia"), pct(r), r, tr!("ушёл с трека в первые 30 с", "left a track within 30 s", "пішов з треку в перші 30 с", "odišiel zo skladby do 30 s")),
        None => bar(tr!("Пропуски", "Skips", "Пропуски", "Preskočenia"), "", 0.0, tr!("ещё копятся — считаются с этой версии", "still collecting — counted since this version", "ще збираються — рахуються з цієї версії", "ešte sa zbierajú — počítajú sa od tejto verzie")),
    });
    t.set_habits(model(habits));
    t.set_hours(model(fp.hours.clone()));
    let d = &fp.dynamics;
    let dmax = d.rising.iter().chain(&d.falling).map(|s| s.w.abs()).fold(0.0, f32::max).max(1e-3);
    t.set_rising(model(d.rising.iter().map(|s| bar(format!("↑ {}", s.name), format!("+{:.1} %", s.w * 100.0), s.w / dmax, "")).collect()));
    t.set_falling(model(d.falling.iter().map(|s| bar(format!("↓ {}", s.name), format!("{:.1} %", s.w * 100.0), s.w.abs() / dmax, "")).collect()));
    t.set_fresh(d.new_artists.join(", ").into());
    t.set_stable(d.stable.join(", ").into());
    t.set_discovered(d.discovered.join(", ").into());
    t.set_diversity(model(d.diversity.iter().map(|x| x.1).collect()));
    let (stars, paths) = taste::stars(fp, g);
    let nodes: Vec<UNode> = stars.into_iter().map(|s| UNode { x: s.x, y: s.y, r: s.r, c: color(s.c), label: s.label.into(), kind: s.kind as i32, lab: s.lab, id: -1, ..Default::default() }).collect();
    t.set_nodes(model(nodes));
    t.set_paths(model(paths.into_iter().map(SharedString::from).collect()));
}

/// Сравнение с другом → модели
pub fn fill_compare(ui: &AppWindow, me: &Fingerprint, them: &taste::Public, my_lib: &HashSet<String>, g: &Graph) {
    let t = ui.global::<Taste>();
    let c = taste::compare(me, &them.fp);
    t.set_cmp_name(them.name.as_str().into());
    t.set_cmp_numbers(
        tr!(
            "Пересечение жанров {} · артистов {} · общих артистов {}",
            "Genre overlap {} · artist overlap {} · shared artists {}",
            "Перетин жанрів {} · артистів {} · спільних артистів {}",
            "Prienik žánrov {} · interpretov {} · spoločných interpretov {}",
            pct(c.genre_overlap),
            pct(c.artist_overlap),
            c.shared_artists.len()
        )
        .into(),
    );
    let smax = c.shared.iter().map(|x| x.1.max(x.2)).fold(0.0, f32::max).max(1e-3);
    t.set_cmp_shared(model(
        c.shared
            .iter()
            .take(8)
            .map(|(n, a, b)| TasteBar { label: n.as_str().into(), value: format!("{} / {}", pct(*a), pct(*b)).into(), frac: a / smax, frac2: b / smax, sub: SharedString::new() })
            .collect(),
    ));
    let line = |label: String, v: &[String]| if v.is_empty() { SharedString::new() } else { format!("{label}: {}", v.iter().take(8).cloned().collect::<Vec<_>>().join(", ")).into() };
    t.set_cmp_only_me(line(tr!("Только у тебя", "Only you", "Тільки в тебе", "Len ty"), &c.only_me));
    t.set_cmp_only_them(line(tr!("Только у {}", "Only {}", "Тільки в {}", "Len {}", them.name), &c.only_them));
    t.set_cmp_artists(line(tr!("Общие артисты", "Shared artists", "Спільні артисти", "Spoloční interpreti"), &c.shared_artists));
    t.set_cmp_surprises(line(tr!("Неожиданно совпали", "Unexpected overlap", "Несподівано збіглися", "Nečakaná zhoda"), &c.surprises));
    t.set_cmp_sound(model(match &c.sound {
        Some(s) => vec![
            bar(tr!("Ритм", "Rhythm", "Ритм", "Rytmus"), pct(s.rhythm), s.rhythm, tr!("звучание", "sound", "звучання", "zvuk")),
            bar(tr!("Гармония", "Harmony", "Гармонія", "Harmónia"), pct(s.harmony), s.harmony, ""),
            bar(tr!("Тембр", "Timbre", "Тембр", "Farba"), pct(s.timbre), s.timbre, ""),
            bar(tr!("Энергетика", "Energy", "Енергетика", "Energia"), pct(s.energy), s.energy, ""),
        ],
        None => Vec::new(),
    }));
    // что может понравиться обоим: их любимые треки, которых у тебя нет, но которые рядом с твоим вкусом
    let picks: Vec<DnaPick> = taste::discover(me, them, my_lib, g)
        .into_iter()
        .map(|d| DnaPick {
            name: d.track.title.as_str().into(),
            sub: d.track.artist.as_str().into(),
            pct: d.score,
            why: match d.why {
                "artist" => tr!("ты слушаешь {}, а этот трек — нет", "you listen to {}, but not this track", "ти слухаєш {}, а цей трек — ні", "počúvaš {}, ale túto skladbu nie", d.arg),
                "similar" => tr!("YouTube Music считает {} похожим на твоего {}", "YouTube Music finds {} similar to your {}", "YouTube Music вважає {} схожим на твого {}", "YouTube Music považuje {} za podobného tvojmu {}", d.arg, d.arg2),
                _ => tr!("в твоём жанре {}", "in your genre {}", "у твоєму жанрі {}", "v tvojom žánri {}", d.arg),
            }
            .into(),
            id: -1,
        })
        .collect();
    t.set_cmp_discover(model(picks));
    t.set_cmp_error(SharedString::new());
    t.set_cmp_shown(true);
}

