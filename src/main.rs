#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod apple;
mod cache;
mod cloud;
mod connect;
mod covers;
mod discord;
mod download;
mod fx;
mod home;
mod local;
mod lyrics;
mod media;
mod model;
mod player;
mod qr;
#[cfg(test)]
mod shots;
mod soundcloud;
mod spicy;
mod spotify;
mod stats;
mod util;
mod ytw;
mod yandex;
mod ytmusic;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use slint::{ComponentHandle, Image, Model, Timer, TimerMode, VecModel};

use model::{fmt_dur, Playlist, Src, Store, Track};
use player::{Cmd, Ev, Player};

slint::include_modules!();

/// Язык для строк, которые собираются в Rust (тосты, статусы): 0 русский, 1 английский, 2 украинский.
/// Строки UI переводит сам Slint через @tr()
static LANG: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

macro_rules! tr {
    ($ru:literal, $en:literal, $uk:literal, $sk:literal $(, $a:expr)* $(,)?) => {
        match LANG.load(Ordering::Relaxed) {
            1 => format!($en $(, $a)*),
            2 => format!($uk $(, $a)*),
            3 => format!($sk $(, $a)*),
            _ => format!($ru $(, $a)*),
        }
    };
}

mod remote;

fn deps_text() -> String {
    let ok = |n: &str| match (util::find_bin(n).is_some(), LANG.load(Ordering::Relaxed)) {
        (true, 1) => "found ✓",
        (true, 2) => "є ✓",
        (true, 3) => "je ✓",
        (true, _) => "есть ✓",
        (false, 1) => "NOT FOUND ✗",
        (false, 2) => "НЕ ЗНАЙДЕНО ✗",
        (false, 3) => "NENÁJDENÉ ✗",
        (false, _) => "НЕ НАЙДЕН ✗",
    };
    tr!(
        "mpv: {}   ·   yt-dlp: {}\nОба нужны: mpv играет звук, yt-dlp достаёт потоки.",
        "mpv: {}   ·   yt-dlp: {}\nBoth are required: mpv plays the audio, yt-dlp fetches the streams.", "mpv: {}   ·   yt-dlp: {}\nПотрібні обидва: mpv грає звук, yt-dlp дістає потоки.", "mpv: {}   ·   yt-dlp: {}\nPotrebné sú oba: mpv prehráva zvuk, yt-dlp získava streamy.",
        ok("mpv"),
        ok("yt-dlp")
    )
}

fn open_url(url: &str) {
    #[cfg(windows)]
    let r = util::command(std::path::Path::new("cmd")).args(["/C", "start", "", url]).spawn();
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let r = std::process::Command::new("xdg-open").arg(url).spawn();
    if let Err(e) = r {
        eprintln!("не смог открыть {url}: {e}");
    }
}

const SRCS: [Src; 3] = [Src::Yt, Src::Sc, Src::Ya];
const THEMES: i32 = 15; // столько тем в Theme (ui/app.slint); следующий слот — цвета из обложки

// геометрия строк lyrics: должна совпадать с ui/app.slint (LyrRow height / spacing)
const LYR_ROW_H: f32 = 56.0;
const LYR_GAP: f32 = 28.0;
/// перевод под строкой: шрифт 0.55 от строки, межстрочный 1.45 (как в LyricLine)
const TR_SCALE: f32 = 0.55;
const TR_ROW_H: f32 = 36.0 * TR_SCALE * 1.45;
// бэк-вокал мельче: шрифт 22 против 36, ряд и отступ над ним меньше
const BG_SCALE: f32 = 22.0 / 36.0;
const BG_GAP: f32 = 6.0;

/// Синхронный текст текущего трека в виде, удобном для тика караоке
struct LyrData {
    raw: Vec<lyrics::Line>,
    // дальше всё по строкам в том виде, как их показывает экран (с точками в проигрышах)
    // тайминги слов: (начало, конец, доля строки где слово начинается, где кончается)
    words: Vec<Vec<(f64, f64, f32, f32)>>,
    starts: Vec<f64>,
    ends: Vec<f64>,  // когда строка перестаёт быть активной (строки Apple бывают внахлёст)
    fills: Vec<f64>, // докуда заливается строка без таймингов слов
    tops: Vec<f32>,
    bg: Vec<bool>,
    dots: Vec<bool>,
    cur: i32,
    cur_at: Instant, // когда сменилась текущая строка — заготовку букв следующей делаем после прокрутки
    tr: HashMap<String, String>, // перевод: текст строки → перевод на язык интерфейса
}

/// Открытый альбом или "все песни" артиста
#[derive(Clone, Default)]
struct Coll {
    title: String,
    sub: String,
    cover: String,
    tracks: Vec<Track>,
}

/// Куда вернуться кнопкой "назад" со страниц артиста и коллекции
enum Nav {
    Page(String),
    Artist(ytmusic::Artist),
    Coll(Coll),
}

/// Первый артист из "A, B & C feat. D" — по нему ищем страницу
fn first_artist(s: &str) -> &str {
    let cut = [", ", " & ", " feat", " ft.", " x ", " и ", "; "].iter().filter_map(|p| s.find(p)).min();
    cut.map_or(s, |i| &s[..i]).trim()
}

struct App {
    ui: AppWindow,
    store: Store,
    results: [Vec<Track>; 3],
    view: Vec<Track>,
    res_model: Rc<VecModel<TrackItem>>,
    pl_model: Rc<VecModel<TrackItem>>,
    pls_model: Rc<VecModel<PlItem>>,
    open_pl: Option<usize>,
    // очередь: played (история) → current → upnext. В шафле upnext перемешан заранее целиком,
    // поэтому следующие треки всегда известны и успевают предзагрузиться
    source: Vec<Track>, // исходный список (из него строится upnext при шафле/повторе)
    played: Vec<Track>,
    upnext: Vec<Track>,
    hist_model: Rc<VecModel<TrackItem>>,
    next_model: Rc<VecModel<TrackItem>>,
    current: Option<Track>,
    gen: u64,
    searches_left: u32,
    covers: HashMap<String, Image>,
    pending: HashSet<String>,
    loader: covers::Loader,
    player: Option<Player>,
    token: Arc<AtomicU64>,
    pos: f64,
    pos_at: Instant,
    dur: f64,
    lyr: Option<LyrData>,
    lyr_model: Rc<VecModel<LyrLine>>,
    lyr_timer: Timer,
    lyr_idle: Timer, // через 2 с без движения мыши прячет управление на экране текста
    lyr_last: Instant,
    lyr_shown: f64, // позиция, которую уже показал текст: назад на доли секунды не откатываем
    lyr_step: f64,  // за сколько секунд UI доезжает до нового значения заливки
    lyr_still: [(i32, f32, Instant); 3], // по звучащим строкам: строка, заливка и когда та последний раз сдвинулась
    lyr_still_model: Rc<VecModel<f32>>,
    lyr_wrap: usize, // символов в ряду, считается от ширины колонки
    lyr_rank: (u8, u8), // насколько хорош показанный текст, см. Found::rank
    lyr_pending: u8, // сколько источников ещё не ответило
    lyr_gen: u64,     // свой счётчик у текста: токен потока меняется при каждой попытке открыть поток
    lyr_for: String,  // для какого трека загружен текст
    lyr_results: Vec<lyrics::Found>, // все ответившие источники — из них выбираем лучший
    lyr_decided: bool,
    lyr_deadline: Timer,
    was_fullscreen: bool,
    shuffle: bool,
    repeat: u8, // 0 — выкл, 1 — вся очередь, 2 — один трек (как на телефоне)
    stream_cache: HashMap<String, (String, Instant)>,
    inflight: HashSet<String>, // что сейчас предзагружается — чтобы не резолвить дважды
    prefetched: bool,
    from_cache: bool,
    media: Option<media::Media>,
    eq_model: Rc<VecModel<f32>>,
    eq_anim: Timer,
    retries: u8,
    fail_streak: u32, // сколько треков подряд не заиграло — чтобы не листать всю очередь впустую
    qr_imgs: Vec<image::RgbaImage>,
    artist: Option<ytmusic::Artist>,
    coll: Coll,
    back: Vec<Nav>,
    art_gen: u64, // чтобы медленный ответ по старому артисту не перетёр новый
    art_top_model: Rc<VecModel<TrackItem>>,
    art_albums_model: Rc<VecModel<Card>>,
    art_singles_model: Rc<VecModel<Card>>,
    art_similar_model: Rc<VecModel<Card>>,
    s_artists: Vec<ytmusic::Card>, // артисты и альбомы в поиске (YT Music)
    s_albums: Vec<ytmusic::Card>,
    s_artists_model: Rc<VecModel<Card>>,
    s_albums_model: Rc<VecModel<Card>>,
    typing: Timer, // поиск по мере набора: ждём паузу в наборе
    last_q: String,
    coll_model: Rc<VecModel<TrackItem>>,
    qr_page: usize, // YouTube иногда отдаёт непроигрываемый формат ("unrecognized file format") — повтор помогает
    load_started: Instant,
    seek_to: Option<(f64, Instant)>, // после перемотки mpv ещё пару опросов отдаёт старую позицию
    add_target: Option<Track>,
    toast_timer: Timer,
    dl_queue: std::collections::VecDeque<Track>, // ждут своей очереди на скачивание
    dl_busy: HashSet<String>,                    // в очереди или качаются (ключи треков)
    dl_active: usize,
    dl_files: HashSet<String>,                   // имена mp3, которые уже лежат в папке загрузок
    intro: Timer,
    review: Option<Review>,
    confirm: Option<Confirm>,
    restored: bool,          // трек из прошлого запуска показан, но ещё не загружен в mpv
    resume_at: Option<f64>, // докуда перемотать, когда он загрузится
    // кроссфейд: второй mpv. Следующий трек стартует в нём, старый доигрывает и гаснет
    player2: Option<Player>,
    xf: Option<(Player, Instant)>, // кто гаснет и с какого момента
    xf_timer: Timer,
    stats: stats::Stats,
    stats_others: stats::Stats, // доли других устройств аккаунта — показываем сумму
    listen_at: Option<Instant>, // последний опрос, когда звук шёл — время между опросами идёт в статистику
    listened: f64,              // сколько секунд дослушан текущий трек (прослушивание считается с 30 с)
    stats_saved: Instant,
    stat_top: Vec<Track>,
    stat_artists: Vec<String>,
    sleep_mode: i32, // 0 выкл, 1–3 через 15/30/60 мин, 4 после этого трека
    sleep_at: Option<Instant>,
    sleep_timer: Timer,
    discord: Option<discord::Discord>,
    discord_last: (String, i64), // что последним отправили — чтобы не дёргать Discord на каждом опросе
    local: Vec<Track>,
    local_model: Rc<VecModel<TrackItem>>,
    auto_for: String, // по какому треку уже подобрали «дальше похожее»
    cover_pal: Option<[[u8; 3]; 8]>,
    muted: Option<f32>, // громкость до M
    rebinding: i32,     // какую горячую клавишу сейчас переназначают (-1 — никакую)
    // открытый плейлист как его видно: поиск по нему и сортировка. pl_view[строка] = индекс трека
    pl_view: Vec<usize>,
    pl_query: String,
    pl_sort: i32, // 0 как добавлены, 1 сначала новые, 2 название, 3 артист, 4 длительность
    home: home::Home,
    home_busy: bool,
    home_quick: Vec<Track>,
    home_recent: Vec<Track>,
    home_pls: Vec<usize>,
    home_mixes_model: Rc<VecModel<Card>>,
    home_recent_model: Rc<VecModel<Card>>,
    home_pls_model: Rc<VecModel<Card>>,
    home_similar_model: Rc<VecModel<Card>>,
    home_quick_model: Rc<VecModel<TrackItem>>,
    save_timer: Timer,
    cloud_timer: Timer,
    push_timer: Timer,
    syncing: bool,
    cloud_loaded: bool, // плейлисты аккаунта уже пришли с сервера
    // Connect: одна сессия на все устройства аккаунта (см. remote.rs)
    conn_live: bool,
    conn_timer: Timer,
    conn_pub: remote::Published,
    conn_adopted: String,           // какую передачу сессии сюда уже подхватили
    mirror: Option<connect::State>, // звук на другом устройстве — тут его сессия
    conn_held: Option<remote::Held>,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|a| a.borrow_mut().as_mut().map(f))
}

/// Выполнить на UI-потоке из любого потока
fn post(f: impl FnOnce(&mut App) + Send + 'static) {
    let _ = slint::invoke_from_event_loop(move || {
        with(f);
    });
}

/// Замок на файл в папке настроек: держится, пока жив процесс (ядро отпускает его само, даже при падении)
/// Err — уже запущен. Файл замка создать не вышло — запускаемся без защиты, а не отказываемся работать
fn single_instance() -> Result<Option<std::fs::File>, ()> {
    let dir = Store::dir();
    let _ = std::fs::create_dir_all(&dir);
    let Ok(f) = std::fs::File::create(dir.join("lock")) else { return Ok(None) };
    match f.try_lock() {
        Ok(()) => Ok(Some(f)),
        Err(std::fs::TryLockError::WouldBlock) => Err(()),
        Err(_) => Ok(None),
    }
}

/// Спотик и SC-превью полноценно не стримятся — ищем этот же трек на YT Music.
/// Локальный файл, которого тут нет (пришёл по QR от друга), — тоже
fn resolve(t: &Track) -> Result<String> {
    match t.src {
        Src::Yt => Ok(t.url.clone()),
        Src::Sc if !t.snip => Ok(t.url.clone()),
        _ => ytmusic::best_match(&format!("{} {}", t.artist, t.title)),
    }
}

/// Файл на этом компьютере — играем его напрямую, без yt-dlp
fn local_file(t: &Track) -> Option<String> {
    (t.src == Src::Lc && std::path::Path::new(&t.url).is_file()).then(|| t.url.clone())
}

/// Ссылка на картинку, которую поймут панели (MPRIS) и Discord: только настоящий http
fn web_cover(t: &Track) -> Option<&str> {
    t.cover.starts_with("http").then_some(t.cover.as_str())
}

/// Похожие треки для радио: умеет только YT Music, остальные сначала ищем там (блокирующе)
fn radio_for(t: &Track) -> Result<Vec<Track>> {
    let vid = match t.src {
        Src::Yt => Ok(t.id.clone()),
        _ => ytmusic::best_match(&format!("{} {}", t.artist, t.title)).and_then(|u| ytmusic::video_id(&u).context("нет videoId")),
    };
    vid.and_then(|v| ytmusic::radio(&v))
}

fn key(t: &Track) -> String {
    format!("{}:{}", t.src.tag(), t.id)
}

// прямые ссылки googlevideo живут ~6 часов, берём с запасом
const STREAM_TTL: Duration = Duration::from_secs(4 * 3600);
const LOAD_TIMEOUT: Duration = Duration::from_secs(40);
const PREFETCH_AHEAD: usize = 5;
const PREFETCH_BEHIND: usize = 3; // чтобы "назад" тоже было мгновенным

/// Какой сервис у ссылки
fn source_of(url: &str) -> Option<Src> {
    [("spotify", Src::Sp), ("soundcloud", Src::Sc), ("youtu", Src::Yt), ("yandex", Src::Ya)].into_iter().find(|(k, _)| url.contains(k)).map(|(_, s)| s)
}

/// Действие, которое ждёт «да» в окне подтверждения
enum Confirm {
    Download(Vec<Track>),
    Remove(usize),
    Radio(Track),
}

/// Импорт из Spotify / Apple Music, который ждёт ответа юзера
struct Review {
    name: String,
    url: String,                                               // ссылка-источник: по ней QR остаётся одним кодом
    skipped: Vec<String>, // что юзер отверг — в QR едет как "убрано", получатель тоже это выкинет
    picked: Vec<Option<Track>>,                                // по порядку исходного плейлиста
    asks: std::collections::VecDeque<(usize, Track, Track)>, // (место, что искали, что нашлось не наверняка)
    missing: usize,
}

/// Ищет треки на YT Music, потом спрашивает про сомнительные (звать из фонового потока)
fn match_on_yt(name: String, url: String, wants: Vec<Track>) {
    let found = find_all_on_yt(&wants);
    post(move |a| a.matched(name, url, wants, found));
}

/// Spotify/Apple Music по ссылке — сразу в треки YT Music, сомнительные берём без вопросов
/// (для QR: что отправитель отверг, придёт в правках как "убрано")
fn fetch_on_yt(url: &str) -> Result<(String, Vec<Track>)> {
    let (name, wants) = if apple::is_apple(url) { apple::import(url)? } else { spotify::import(url)? };
    Ok((name, find_all_on_yt(&wants).into_iter().flatten().map(|(t, _)| t).collect()))
}

/// Ищет треки на YT Music в 6 потоков (блокирующе), прогресс — в статус импорта
fn find_all_on_yt(wants: &[Track]) -> Vec<Option<(Track, bool)>> {
    let n = wants.len();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let done = std::sync::atomic::AtomicUsize::new(0);
    let found: std::sync::Mutex<Vec<Option<(Track, bool)>>> = std::sync::Mutex::new(vec![None; n]);
    std::thread::scope(|sc| {
        for _ in 0..6 {
            sc.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(w) = wants.get(i) else { break };
                // сеть моргнула — ещё раз, потом сдаёмся (трек попадёт в «не нашлось»)
                let r = ytmusic::find_same(w).or_else(|_| ytmusic::find_same(w));
                found.lock().unwrap()[i] = r.ok().flatten();
                let k = done.fetch_add(1, Ordering::Relaxed) + 1;
                post(move |a| a.ui.set_import_status(tr!("Ищу треки на YT Music: {}/{}", "Finding tracks on YT Music: {}/{}", "Шукаю треки на YT Music: {}/{}", "Hľadám skladby na YT Music: {}/{}", k, n).into()));
            });
        }
    });
    found.into_inner().unwrap()
}

/// Плейлист/альбом по ссылке любого сервиса (блокирующе — звать из фонового потока)
fn fetch_url(url: &str) -> Result<(String, Vec<Track>)> {
    match source_of(url).context("неизвестная ссылка")? {
        Src::Yt => ytmusic::import(url),
        Src::Sc => soundcloud::import(url),
        Src::Sp => spotify::import(url),
        Src::Ya => yandex::import(url),
        Src::Lc => anyhow::bail!("это файл, а не ссылка"),
    }
}

fn rand_below(n: usize) -> usize {
    // xorshift с общим состоянием: раньше сид брался из часов на каждый вызов, и подряд идущие
    // вызовы (перемешивание) давали коррелированные числа
    static STATE: AtomicU64 = AtomicU64::new(0);
    let mut x = STATE.load(Ordering::Relaxed);
    if x == 0 {
        x = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1) | 1;
    }
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    STATE.store(x, Ordering::Relaxed);
    (x % n.max(1) as u64) as usize
}

/// Горячие клавиши: (действие, клавиша по умолчанию). Esc не меняется — им выходят из всего
const KEYS: [(&str, &str); 14] = [
    ("toggle", "Space"),
    ("back", "←"),
    ("fwd", "→"),
    ("prev", "Ctrl+←"),
    ("next", "Ctrl+→"),
    ("vol+", "↑"),
    ("vol-", "↓"),
    ("mute", "M"),
    ("lyrics", "L"),
    ("shuffle", "S"),
    ("repeat", "R"),
    ("fullscreen", "F"),
    ("search", "Ctrl+F"),
    ("sync", "Ctrl+R"),
];

/// Нажатие → имя клавиши вида "Ctrl+Shift+K". Буквы — по месту на клавиатуре, а не по раскладке:
/// «ь» в русской — это та же M. None — сам по себе модификатор или что-то непечатное
fn key_name(text: &str, ctrl: bool, alt: bool, shift: bool, meta: bool) -> Option<String> {
    use slint::platform::Key;
    const RU: &str = "йцукенгшщзхъфывапролджэячсмитьбюёіїєґ";
    const EN: &str = "qwertyuiop[]asdfghjkl;'zxcvbnm,.`s]'`";
    let c = text.chars().next()?;
    let named = [
        (Key::LeftArrow, "←"), (Key::RightArrow, "→"), (Key::UpArrow, "↑"), (Key::DownArrow, "↓"),
        (Key::Home, "Home"), (Key::End, "End"), (Key::PageUp, "PgUp"), (Key::PageDown, "PgDn"),
        (Key::Delete, "Del"), (Key::Insert, "Ins"), (Key::Return, "Enter"), (Key::Tab, "Tab"),
        (Key::Backspace, "Backspace"), (Key::Escape, "Esc"),
        (Key::F1, "F1"), (Key::F2, "F2"), (Key::F3, "F3"), (Key::F4, "F4"), (Key::F5, "F5"), (Key::F6, "F6"),
        (Key::F7, "F7"), (Key::F8, "F8"), (Key::F9, "F9"), (Key::F10, "F10"), (Key::F11, "F11"), (Key::F12, "F12"),
    ];
    let (base, is_named) = if c == ' ' {
        ("Space".to_string(), true)
    } else if let Some((_, n)) = named.iter().find(|(k, _)| char::from(k.clone()) == c) {
        (n.to_string(), true)
    } else if c.is_control() || ('\u{F700}'..='\u{F8FF}').contains(&c) {
        return None; // Shift, Ctrl, Alt и прочие служебные
    } else {
        let low = c.to_lowercase().next()?;
        let pos = RU.chars().position(|r| r == low).and_then(|i| EN.chars().nth(i)).unwrap_or(low);
        (pos.to_uppercase().to_string(), false)
    };
    let mut out = String::new();
    for (on, m) in [(ctrl, "Ctrl+"), (alt, "Alt+"), (meta, "Super+")] {
        if on {
            out += m;
        }
    }
    // Shift у знаков зависит от раскладки ("/" и "?" — одна клавиша), учитываем его только у букв и именованных
    if shift && (is_named || c.is_alphabetic()) {
        out += "Shift+";
    }
    Some(out + &base)
}

/// "2 ч 14 мин" для статистики
fn fmt_long(ms: u64) -> String {
    let m = ms / 60_000;
    match (m / 60, m % 60) {
        (0, m) => tr!("{} мин", "{} min", "{} хв", "{} min", m),
        (h, 0) => tr!("{} ч", "{} h", "{} год", "{} h", h),
        (h, m) => tr!("{} ч {} мин", "{} h {} min", "{} год {} хв", "{} h {} min", h, m),
    }
}

/// Один и тот же трек из разных мест (радио отдаёт свои id) — по названию и артисту
fn same_song(t: &Track) -> String {
    format!("{}|{}", t.title.to_lowercase(), first_artist(&t.artist).to_lowercase())
}

/// Обложка «Любимых треков»: белое сердечко на фиолетово-розовом градиенте
fn liked_art() -> Image {
    const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 240 240"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#3b1d8f"/><stop offset=".5" stop-color="#7c3aed"/><stop offset="1" stop-color="#db2777"/></linearGradient></defs><rect width="240" height="240" fill="url(#g)"/><path transform="translate(70 72) scale(4.2)" fill="#fff" d="M12 21.35l-1.45-1.32C5.4 15.36 2 12.28 2 8.5 2 5.42 4.42 3 7.5 3c1.74 0 3.41.81 4.5 2.09C13.09 3.81 14.76 3 16.5 3 19.58 3 22 5.42 22 8.5c0 3.78-3.4 6.86-8.55 11.54L12 21.35z"/></svg>"##;
    Image::load_from_svg_data(SVG.as_bytes()).unwrap_or_default()
}

/// Имя плейлиста для показа: любимые — «♥ Любимые» на языке интерфейса
fn pl_name(name: &str) -> String {
    if name == model::LIKED {
        tr!("Любимые треки", "Liked Songs", "Улюблені треки", "Obľúbené skladby")
    } else {
        name.to_string()
    }
}

/// Код ошибки Firebase → человеческий текст. Протухшая сессия — выходим из аккаунта (плейлисты остаются)
fn acc_err(e: &anyhow::Error) -> String {
    let s = e.to_string();
    let code = s.split([' ', ':']).next().unwrap_or("");
    match code {
        "EMAIL_EXISTS" => tr!("На эту почту уже есть аккаунт — просто войди", "This email already has an account — just sign in", "На цю пошту вже є акаунт — просто увійди", "Na tento e-mail už účet existuje — stačí sa prihlásiť"),
        "INVALID_LOGIN_CREDENTIALS" | "INVALID_PASSWORD" | "EMAIL_NOT_FOUND" => tr!("Неверная почта или пароль", "Wrong email or password", "Невірна пошта або пароль", "Nesprávny e-mail alebo heslo"),
        "WEAK_PASSWORD" => tr!("Пароль слишком короткий: нужно от 6 символов", "Password is too short: at least 6 characters", "Пароль закороткий: потрібно від 6 символів", "Heslo je príliš krátke: aspoň 6 znakov"),
        "MISSING_EMAIL" => tr!("Введи почту", "Enter your email", "Введи пошту", "Zadaj e-mail"),
        "INVALID_EMAIL" => tr!("Это не похоже на почту", "That doesn't look like an email", "Це не схоже на пошту", "Toto nevyzerá ako e-mail"),
        "MISSING_PASSWORD" => tr!("Введи пароль", "Enter a password", "Введи пароль", "Zadaj heslo"),
        "TOO_MANY_ATTEMPTS_TRY_LATER" => tr!("Слишком много попыток, подожди немного", "Too many attempts, wait a bit", "Забагато спроб, зачекай трохи", "Príliš veľa pokusov, chvíľu počkaj"),
        "NETWORK" => tr!("Нет связи с сервером", "Can't reach the server", "Немає зв'язку з сервером", "Server je nedostupný"),
        "TOKEN_EXPIRED" | "INVALID_REFRESH_TOKEN" | "USER_NOT_FOUND" | "USER_DISABLED" | "NOT_SIGNED_IN" => {
            cloud::sign_out();
            tr!("Сессия кончилась, войди заново", "Session expired, sign in again", "Сесія скінчилася, увійди знову", "Relácia vypršala, prihlás sa znova")
        }
        _ => tr!("Не вышло: {}", "Failed: {}", "Не вийшло: {}", "Nevyšlo to: {}", s),
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

fn shuffled(mut v: Vec<Track>) -> Vec<Track> {
    for i in (1..v.len()).rev() {
        v.swap(i, rand_below(i + 1));
    }
    v
}

impl App {
    fn item(&self, t: &Track) -> TrackItem {
        let cover = self.covers.get(&t.cover).cloned();
        TrackItem {
            title: t.title.as_str().into(),
            artist: t.artist.as_str().into(),
            source: t.src.tag().into(),
            duration: fmt_dur(t.dur).into(),
            has_cover: cover.is_some(),
            cover: cover.unwrap_or_default(),
            cover_url: t.cover.as_str().into(),
            playing: self.current.as_ref().is_some_and(|c| c.same(t)),
            snip: t.snip,
            dl: self.dl_of(t),
            liked: self.is_liked(t),
        }
    }

    fn is_liked(&self, t: &Track) -> bool {
        self.store.playlists.first().is_some_and(|p| p.name == model::LIKED && p.tracks.iter().any(|x| x.same(t)))
    }

    fn want_cover(&mut self, url: &str) {
        if !url.is_empty() && !self.covers.contains_key(url) && self.pending.insert(url.to_string()) {
            self.loader.get(url.to_string());
        }
    }

    fn on_cover(&mut self, url: String, buf: Option<covers::Buf>) {
        let Some(buf) = buf else { return }; // в pending остаётся — повторно не долбим
        self.pending.remove(&url);
        let img = Image::from_rgba8(buf);
        if self.covers.len() > 300 {
            self.covers.clear();
        }
        self.covers.insert(url.clone(), img.clone());
        for m in [&self.res_model, &self.pl_model, &self.hist_model, &self.next_model, &self.art_top_model, &self.coll_model, &self.local_model, &self.home_quick_model] {
            for i in 0..m.row_count() {
                if let Some(mut r) = m.row_data(i) {
                    if r.cover_url == url.as_str() && !r.has_cover {
                        r.cover = img.clone();
                        r.has_cover = true;
                        m.set_row_data(i, r);
                    }
                }
            }
        }
        for m in [
            &self.art_albums_model,
            &self.art_singles_model,
            &self.art_similar_model,
            &self.s_artists_model,
            &self.s_albums_model,
            &self.home_mixes_model,
            &self.home_recent_model,
            &self.home_pls_model,
            &self.home_similar_model,
        ] {
            for i in 0..m.row_count() {
                if let Some(mut c) = m.row_data(i) {
                    if c.cover_url == url.as_str() && !c.has_cover {
                        c.cover = img.clone();
                        c.has_cover = true;
                        m.set_row_data(i, c);
                    }
                }
            }
        }
        if self.coll.cover == url {
            self.ui.set_coll_cover(img.clone());
            self.ui.set_has_coll_cover(true);
        }
        if self.current.as_ref().is_some_and(|t| t.cover == url) {
            self.ui.set_now_cover(img);
            self.ui.set_now_has_cover(true);
        }
    }

    // ── артисты и альбомы ──

    fn card(&self, c: &ytmusic::Card) -> Card {
        let img = self.covers.get(&c.cover).cloned();
        Card { title: c.title.as_str().into(), sub: c.sub.as_str().into(), has_cover: img.is_some(), cover: img.unwrap_or_default(), cover_url: c.cover.as_str().into() }
    }

    /// Запомнить, где мы сейчас, чтобы "назад" туда вернул
    fn push_nav(&mut self) {
        let page = self.ui.get_page().to_string();
        let nav = match page.as_str() {
            "artist" => self.artist.clone().map(Nav::Artist),
            "collection" => Some(Nav::Coll(self.coll.clone())),
            _ => Some(Nav::Page(page)),
        };
        if let Some(n) = nav {
            self.back.push(n);
            if self.back.len() > 30 {
                self.back.remove(0);
            }
        }
    }

    fn nav_back(&mut self) {
        match self.back.pop() {
            Some(Nav::Artist(a)) => self.show_artist(a),
            Some(Nav::Coll(c)) => self.show_coll(c),
            Some(Nav::Page(p)) => self.ui.set_page(p.into()),
            None => self.ui.set_page("search".into()),
        }
    }

    fn open_artist_of(&mut self, t: Track) {
        let name = first_artist(&t.artist).to_string();
        let id = t.artist_id.clone();
        self.load_artist(move || if id.is_empty() { ytmusic::find_artist(&name) } else { Ok(id) });
    }

    fn load_artist(&mut self, id: impl FnOnce() -> Result<String> + Send + 'static) {
        if self.ui.get_lyrics_open() {
            self.set_lyrics_open(false);
        }
        self.ui.set_queue_open(false);
        self.push_nav();
        self.art_gen += 1;
        let gen = self.art_gen;
        self.ui.set_artist_loading(true);
        self.ui.set_artist_error("".into());
        self.ui.set_page("artist".into());
        std::thread::spawn(move || {
            let r = id().and_then(|id| ytmusic::artist(&id));
            post(move |a| {
                if a.art_gen != gen {
                    return;
                }
                match r {
                    Ok(ar) => a.show_artist(ar),
                    Err(e) => {
                        a.ui.set_artist_loading(false);
                        a.ui.set_artist_error(tr!("Не нашёл артиста: {:#}", "Couldn't find the artist: {:#}", "Не знайшов артиста: {:#}", "Interpreta som nenašiel: {:#}", e).into());
                    }
                }
            });
        });
    }

    fn show_artist(&mut self, ar: ytmusic::Artist) {
        let ui = &self.ui;
        ui.set_page("artist".into());
        ui.set_artist_loading(false);
        ui.set_artist_error("".into());
        ui.set_artist_name(ar.name.as_str().into());
        ui.set_artist_stats(ar.stats.as_str().into());
        ui.set_artist_desc(ar.desc.as_str().into());
        ui.set_has_all_songs(ar.songs_id.is_some());
        ui.set_has_artist_photo(false);
        let top: Vec<TrackItem> = ar.top.iter().map(|t| self.item(t)).collect();
        self.art_top_model.set_vec(top);
        for (m, cards) in [(&self.art_albums_model, &ar.albums), (&self.art_singles_model, &ar.singles), (&self.art_similar_model, &ar.similar)] {
            m.set_vec(cards.iter().map(|c| self.card(c)).collect::<Vec<_>>());
        }
        // большое фото и размытый фон под шапку — как на экране текстов
        let (photo, gen) = (ar.photo.clone(), self.art_gen);
        std::thread::spawn(move || {
            let Ok(b) = covers::big(&photo).map_err(|e| eprintln!("фото артиста: {e:#}")) else { return };
            post(move |a| {
                if a.art_gen == gen {
                    a.ui.set_artist_photo(Image::from_rgba8(b.cover));
                    a.ui.set_artist_bg(Image::from_rgba8(b.bg));
                    a.ui.set_has_artist_photo(true);
                }
            });
        });
        self.artist = Some(ar);
        self.ui.set_can_back(!self.back.is_empty());
        self.mark_playing();
    }

    fn show_coll(&mut self, c: Coll) {
        let ui = &self.ui;
        ui.set_page("collection".into());
        ui.set_coll_loading(false);
        ui.set_coll_title(c.title.as_str().into());
        ui.set_coll_sub(c.sub.as_str().into());
        let cover = self.covers.get(&c.cover).cloned();
        ui.set_has_coll_cover(cover.is_some());
        ui.set_coll_cover(cover.unwrap_or_default());
        self.want_cover(&c.cover);
        let items: Vec<TrackItem> = c.tracks.iter().map(|t| self.item(t)).collect();
        self.coll_model.set_vec(items);
        self.coll = c;
        self.ui.set_can_back(!self.back.is_empty());
        self.mark_playing();
    }

    /// Коллекция грузится в фоне: альбом или все песни артиста
    fn load_coll(&mut self, load: impl FnOnce() -> Result<Coll> + Send + 'static) {
        self.push_nav();
        self.art_gen += 1;
        let gen = self.art_gen;
        self.ui.set_coll_loading(true);
        self.ui.set_coll_title("".into());
        self.ui.set_coll_sub("".into());
        self.ui.set_has_coll_cover(false);
        self.coll_model.set_vec(vec![]);
        self.ui.set_page("collection".into());
        std::thread::spawn(move || {
            let r = load();
            post(move |a| {
                if a.art_gen != gen {
                    return;
                }
                match r {
                    Ok(c) => a.show_coll(c),
                    Err(e) => {
                        a.ui.set_coll_loading(false);
                        a.toast(&tr!("Не загрузилось: {:#}", "Couldn't load: {:#}", "Не завантажилось: {:#}", "Nenačítalo sa: {:#}", e));
                    }
                }
            });
        });
    }

    fn artist_all(&mut self) {
        let Some(ar) = self.artist.clone() else { return };
        let Some(id) = ar.songs_id.clone() else { return };
        self.load_coll(move || {
            let tracks = ytmusic::all_songs(&id, &ar.name)?;
            Ok(Coll { title: ar.name.clone(), sub: tr!("Все песни · {} треков", "All songs · {} tracks", "Усі пісні · {} треків", "Všetky skladby · {} skladieb", tracks.len()), cover: ar.photo.clone(), tracks })
        });
    }

    fn open_card(&mut self, which: i32, i: usize) {
        // карточки главной: 10 микс, 11 недавнее, 12 плейлист, 13 похожий артист
        match which {
            10 => {
                let Some(m) = self.home.mixes.get(i).cloned() else { return };
                self.push_nav();
                let sub = tr!("{} · {} треков", "{} · {} tracks", "{} · {} треків", "{} · {} skladieb", m.sub, m.tracks.len());
                return self.show_coll(Coll { title: m.title, sub, cover: m.cover, tracks: m.tracks });
            }
            11 => return self.play_queue(self.home_recent.clone(), i),
            12 => {
                if let Some(&p) = self.home_pls.get(i) {
                    self.open_playlist(p);
                }
                return;
            }
            13 => {
                let Some(c) = self.home.similar.get(i).cloned() else { return };
                return self.load_artist(move || Ok(c.id));
            }
            _ => {}
        }
        let c = match which {
            3 => self.s_artists.get(i).cloned(),
            4 => self.s_albums.get(i).cloned(),
            _ => self.artist.as_ref().and_then(|ar| match which {
                0 => &ar.albums,
                1 => &ar.singles,
                _ => &ar.similar,
            }.get(i).cloned()),
        };
        let Some(c) = c else { return };
        if c.id.starts_with("UC") {
            return self.load_artist(move || Ok(c.id));
        }
        self.load_coll(move || {
            let (title, sub, cover, tracks) = ytmusic::album(&c.id)?;
            Ok(Coll { title, sub, cover, tracks })
        });
    }

    /// "Слушать" на странице артиста: все песни (догружаем), а если их нет — популярное
    fn artist_play(&mut self, shuffle: bool) {
        let Some(ar) = self.artist.clone() else { return };
        if shuffle {
            self.set_shuffle(true);
        }
        let Some(id) = ar.songs_id.clone() else {
            let i = if shuffle { rand_below(ar.top.len()) } else { 0 };
            return self.play_queue(ar.top, i);
        };
        self.toast(&tr!("Загружаю песни {}…", "Loading {} songs…", "Завантажую пісні {}…", "Načítavam skladby {}…", ar.name));
        std::thread::spawn(move || {
            let r = ytmusic::all_songs(&id, &ar.name);
            post(move |a| match r {
                Ok(t) if !t.is_empty() => {
                    let i = if shuffle { rand_below(t.len()) } else { 0 };
                    a.play_queue(t, i);
                }
                _ => a.play_queue(ar.top, 0),
            });
        });
    }

    fn set_lang(&mut self, lang: &str) {
        let code = match lang {
            "en" => 1,
            "uk" => 2,
            "sk" => 3,
            _ => 0,
        };
        LANG.store(code, Ordering::Relaxed);
        // исходный язык UI — русский (пустая строка), остальные — из вшитых lang/en и lang/uk
        if let Err(e) = slint::select_bundled_translation(match code {
            1 => "en",
            2 => "uk",
            3 => "sk",
            _ => "",
        }) {
            eprintln!("перевод: {e}");
        }
        self.ui.set_lang(lang.into());
        // перевод текста был на прошлый язык — переводим заново
        if let Some(d) = &mut self.lyr {
            if !d.tr.is_empty() {
                d.tr.clear();
                self.layout_lyrics();
                self.lyr_translate(false);
            }
        }
        self.ui.set_deps(deps_text().into());
        let presets: Vec<slint::SharedString> = fx::PRESETS
            .iter()
            .map(|(ru, en, _)| match code {
                1 => *en,
                2 if *ru == "Ровно" => "Рівно",
                _ => *ru,
            })
            .map(Into::into)
            .collect();
        // в два ряда, чтобы влезали в панель
        let (a, b) = presets.split_at(7);
        self.ui.set_eq_presets(Rc::new(VecModel::from(a.to_vec())).into());
        self.ui.set_eq_presets2(Rc::new(VecModel::from(b.to_vec())).into());
        self.refresh_keys();
    }

    // ── эквалайзер и скорость ──

    fn apply_fx(&mut self) {
        let st = &mut self.store.settings;
        st.eq.resize(fx::BANDS.len(), 0.0);
        for p in self.player.iter().chain(&self.player2).chain(self.xf.as_ref().map(|x| &x.0)) {
            p.send(Cmd::Af(fx::af(&st.eq)));
            p.send(Cmd::Speed(st.speed as f64));
            p.send(Cmd::KeepPitch(st.keep_pitch));
        }
        let target = st.eq.clone();
        if self.eq_model.row_count() != target.len() {
            self.eq_model.set_vec(target.clone());
        }
        self.ui.set_eq_preset(fx::preset_of(&target));
        self.ui.set_speed(st.speed);
        self.ui.set_keep_pitch(st.keep_pitch);
        self.ui.set_fx_default(target.iter().all(|g| g.abs() < 0.05) && (st.speed - 1.0).abs() < 0.001 && st.keep_pitch);
        self.save();
        if self.eq_anim.running() {
            return; // ползунки уже едут к новым значениям
        }
        for (i, g) in target.into_iter().enumerate() {
            self.eq_model.set_row_data(i, g);
        }
    }

    /// Пресет/сброс: звук меняется сразу, а ползунки плавно доезжают до новых значений (~0.45 с)
    fn animate_eq(&mut self, to: Vec<f32>) {
        let from: Vec<f32> = (0..self.eq_model.row_count()).filter_map(|i| self.eq_model.row_data(i)).collect();
        self.store.settings.eq = to.clone();
        let t0 = Instant::now();
        // таймер стартует до apply_fx: тот видит, что ползунки едут, и не ставит их сразу в конец
        self.eq_anim.start(TimerMode::Repeated, Duration::from_millis(16), move || {
            with(|a| {
                let t = (t0.elapsed().as_secs_f32() / 0.45).min(1.0);
                let e = 1.0 - (1.0 - t).powi(3); // ease-out
                for (i, (f, g)) in from.iter().zip(&to).enumerate() {
                    a.eq_model.set_row_data(i, f + (g - f) * e);
                }
                if t >= 1.0 {
                    a.eq_anim.stop();
                }
            });
        });
        self.apply_fx();
    }

    fn toast(&mut self, s: &str) {
        self.ui.set_toast(s.into());
        self.ui.set_toast_on(true);
        self.toast_timer.start(TimerMode::SingleShot, Duration::from_millis(2600), || {
            with(|a| a.ui.set_toast_on(false));
        });
    }

    fn save(&mut self) {
        for p in self.store.playlists.iter().filter(|p| !p.cover.is_empty()) {
            self.store.pl_covers.insert(p.name.clone(), p.cover.clone());
        }
        self.store.save();
        // плейлисты на диск не пишутся — правку через пару секунд отправляем на сервер
        if self.cloud_loaded && cloud::dirty(&self.store.playlists) {
            self.push_timer.start(TimerMode::SingleShot, Duration::from_secs(2), || {
                with(|a| a.cloud_sync(false));
            });
        }
    }

    // ── поиск ──

    fn set_status(&self, i: usize, s: &str) {
        match i {
            0 => self.ui.set_st_yt(s.into()),
            1 => self.ui.set_st_sc(s.into()),
            _ => self.ui.set_st_ya(s.into()),
        }
    }

    /// Набор в поле поиска: ищем, когда пользователь притормозил на 350 мс
    fn search_typed(&mut self, q: String) {
        if q.trim().chars().count() < 2 {
            self.typing.stop();
            if q.trim().is_empty() {
                self.gen += 1; // ответы старого запроса больше не нужны
                self.last_q.clear();
                self.results = Default::default();
                self.rebuild_view();
                self.set_cards(vec![], vec![]);
                self.ui.set_searching(false);
                (0..SRCS.len()).for_each(|i| self.set_status(i, ""));
            }
            return;
        }
        self.typing.start(TimerMode::SingleShot, Duration::from_millis(350), move || {
            with(|a| a.search(q.clone(), false));
        });
    }

    fn set_cards(&mut self, artists: Vec<ytmusic::Card>, albums: Vec<ytmusic::Card>) {
        self.s_artists_model.set_vec(artists.iter().map(|c| self.card(c)).collect::<Vec<_>>());
        self.s_albums_model.set_vec(albums.iter().map(|c| self.card(c)).collect::<Vec<_>>());
        (self.s_artists, self.s_albums) = (artists, albums);
    }

    /// force — Enter: повторяем даже тот же запрос (вдруг сеть моргнула)
    fn search(&mut self, q: String, force: bool) {
        self.typing.stop();
        let q = q.trim().to_string();
        if q.is_empty() || (!force && q == self.last_q) {
            return;
        }
        self.last_q = q.clone();
        self.gen += 1;
        let gen = self.gen;
        // старые результаты не стираем: при наборе они висят, пока каждый сервис не ответит новыми — без мигания
        self.ui.set_searching(true);
        self.searches_left = SRCS.len() as u32;
        {
            let q = q.clone();
            std::thread::spawn(move || {
                // артисты и альбомы — два запроса параллельно
                let q2 = q.clone();
                let albums = std::thread::spawn(move || ytmusic::search_cards(&q2, ytmusic::ALBUMS, 12));
                let artists = ytmusic::search_cards(&q, ytmusic::ARTISTS, 8).unwrap_or_else(|e| {
                    eprintln!("поиск артистов: {e:#}");
                    vec![]
                });
                let albums = albums.join().ok().and_then(|r| r.map_err(|e| eprintln!("поиск альбомов: {e:#}")).ok()).unwrap_or_default();
                post(move |a| {
                    if a.gen == gen {
                        a.set_cards(artists, albums);
                    }
                });
            });
        }
        for (i, src) in SRCS.into_iter().enumerate() {
            self.set_status(i, "…");
            let q = q.clone();
            std::thread::spawn(move || {
                let r = match src {
                    Src::Sc => soundcloud::search(&q, 20),
                    Src::Ya => yandex::search(&q, 20),
                    _ => ytmusic::search(&q, 20),
                };
                post(move |a| {
                    if a.gen != gen {
                        return;
                    }
                    a.searches_left -= 1;
                    if a.searches_left == 0 {
                        a.ui.set_searching(false);
                    }
                    match r {
                        Ok(v) => {
                            a.set_status(i, &v.len().to_string());
                            a.results[i] = v;
                            a.rebuild_view();
                        }
                        Err(e) => {
                            eprintln!("{src:?}: {e:#}");
                            a.results[i].clear(); // висели от прошлого запроса
                            a.rebuild_view();
                            // Яндекс за пределами РФ/СНГ отвечает 451 — это не ошибка плеера
                            let region = e.to_string().contains("регионе");
                            a.set_status(i, &if region { tr!("недоступна в регионе", "unavailable in your region", "недоступна в регіоні", "nedostupné vo tvojom regióne") } else { tr!("ошибка", "error", "помилка", "chyba") });
                        }
                    }
                });
            });
        }
    }

    fn rebuild_view(&mut self) {
        let f = self.ui.get_filter() as usize;
        self.view = if f == 0 {
            // вперемешку по кругу, чтобы сверху были все сервисы
            let n = self.results.iter().map(Vec::len).max().unwrap_or(0);
            (0..n).flat_map(|i| self.results.iter().filter_map(move |r| r.get(i).cloned())).collect()
        } else {
            self.results[f - 1].clone()
        };
        let items: Vec<TrackItem> = self.view.iter().map(|t| self.item(t)).collect();
        self.res_model.set_vec(items);
    }

    // ── плейлисты ──

    fn refresh_playlists(&self) {
        let items: Vec<PlItem> =
            self.store.playlists.iter().map(|p| PlItem { name: pl_name(&p.name).into(), count: p.tracks.len() as i32, liked: p.name == model::LIKED }).collect();
        self.pls_model.set_vec(items);
        self.ui.set_open_pl(self.open_pl.map_or(-1, |i| i as i32));
        self.update_liked();
    }

    /// «Любимые треки» есть всегда и стоят первыми (открытый плейлист при этом не теряем)
    fn ensure_liked(&mut self) {
        let open = self.open_pl.and_then(|i| self.store.playlists.get(i)).map(|p| p.name.clone());
        match self.store.playlists.iter().position(|p| p.name == model::LIKED) {
            Some(0) => return,
            Some(i) => {
                let p = self.store.playlists.remove(i);
                self.store.playlists.insert(0, p);
            }
            None => self.store.playlists.insert(0, Playlist { name: model::LIKED.to_string(), ..Default::default() }),
        }
        self.open_pl = open.and_then(|n| self.store.playlists.iter().position(|p| p.name == n));
    }

    /// ♥ в панели плеера — в любимых ли играющий трек
    fn update_liked(&self) {
        let on = self.current.as_ref().is_some_and(|c| self.store.playlists.iter().any(|p| p.name == model::LIKED && p.tracks.iter().any(|t| t.same(c))));
        self.ui.set_cur_liked(on);
    }

    /// ♥: играющий трек в любимые (свежие сверху) или обратно. Плейлист любимых создаётся при первом сердечке
    fn toggle_like(&mut self) {
        let Some(t) = self.current.clone() else { return };
        self.like_track(t);
    }

    /// ♥ у строки трека — не запуская его
    fn like_at(&mut self, idx: usize, kind: i32) {
        if let Some(t) = self.track_at(kind, idx) {
            self.like_track(t);
        }
    }

    /// ♥ всем трекам открытого плейлиста (кто уже в любимых — остаётся на своём месте)
    fn like_all(&mut self) {
        let Some(pl) = self.open_pl.and_then(|i| self.store.playlists.get(i)).filter(|p| p.name != model::LIKED) else { return };
        let new: Vec<Track> = pl.tracks.iter().filter(|t| !self.is_liked(t)).cloned().collect();
        if new.is_empty() {
            return self.toast(&tr!("Тут всё уже в любимых", "Everything here is already liked", "Тут усе вже в улюблених", "Všetko tu už je v obľúbených"));
        }
        self.ensure_liked();
        let n = new.len();
        let liked = &mut self.store.playlists[0].tracks;
        liked.splice(0..0, new);
        self.liked_changed();
        self.toast(&tr!("В любимые: {} треков ♥", "Liked {} tracks ♥", "В улюблені: {} треків ♥", "Do obľúbených: {} skladieb ♥", n));
    }

    fn like_track(&mut self, t: Track) {
        self.ensure_liked();
        let i = 0;
        let p = &mut self.store.playlists[i];
        let msg = if let Some(j) = p.tracks.iter().position(|x| x.same(&t)) {
            p.tracks.remove(j);
            tr!("Убрал из любимых", "Removed from Liked", "Прибрав з улюблених", "Odobrané z obľúbených")
        } else {
            p.tracks.insert(0, t);
            tr!("В любимых ♥", "Added to Liked ♥", "В улюблених ♥", "V obľúbených ♥")
        };
        self.liked_changed();
        self.toast(&msg);
    }

    /// любимые поменялись: сохранить, обновить список плейлистов, сердечки в строках и открытый плейлист
    fn liked_changed(&mut self) {
        self.save();
        self.refresh_playlists();
        self.mark_playing();
        if self.open_pl == Some(0) {
            self.open_playlist(0);
        }
    }

    fn open_playlist(&mut self, i: usize) {
        let Some(pl) = self.store.playlists.get(i) else { return };
        self.ui.set_pl_title(pl_name(&pl.name).into());
        self.ui.set_pl_desc(pl.desc.as_str().into());
        let cover = if pl.name == model::LIKED { Some(liked_art()) } else { (!pl.cover.is_empty()).then(|| Image::load_from_path(std::path::Path::new(&pl.cover)).ok()).flatten() };
        self.ui.set_pl_has_cover(cover.is_some());
        self.ui.set_pl_cover(cover.unwrap_or_default());
        if self.open_pl != Some(i) {
            // другой плейлист — поиск и сортировка с чистого листа
            self.pl_query.clear();
            self.pl_sort = 0;
            self.ui.set_pl_query("".into());
            self.ui.set_pl_sort(0);
        }
        self.open_pl = Some(i);
        self.pl_rebuild();
        self.ui.set_page("playlist".into());
        self.refresh_playlists();
    }

    /// Строки открытого плейлиста по поиску и сортировке. Поиск понимает и транслит: «skryptonit» найдёт «Скриптонит»
    fn pl_rebuild(&mut self) {
        let Some(pl) = self.open_pl.and_then(|i| self.store.playlists.get(i)) else { return };
        let view = self.pl_order(&self.pl_query);
        let tr = &pl.tracks;
        let items: Vec<TrackItem> = view.iter().map(|&j| self.item(&tr[j])).collect();
        self.ui.set_pl_total(tr.len() as i32);
        self.pl_view = view;
        self.pl_model.set_vec(items);
    }

    /// Номера треков открытого плейлиста: найденные по запросу (пустой — все) в выбранной сортировке
    fn pl_order(&self, query: &str) -> Vec<usize> {
        let Some(pl) = self.open_pl.and_then(|i| self.store.playlists.get(i)) else { return vec![] };
        let q = query.trim().to_lowercase();
        let ql = ytmusic::latin(&q);
        let mut view: Vec<usize> = (0..pl.tracks.len())
            .filter(|&j| {
                let t = &pl.tracks[j];
                let hay = format!("{} {}", t.title, t.artist).to_lowercase();
                q.is_empty() || hay.contains(&q) || (!ql.is_empty() && ytmusic::latin(&hay).contains(&ql))
            })
            .collect();
        let tr = &pl.tracks;
        match self.pl_sort {
            1 => view.reverse(),
            2 => view.sort_by_cached_key(|&j| tr[j].title.to_lowercase()),
            3 => view.sort_by_cached_key(|&j| (tr[j].artist.to_lowercase(), tr[j].title.to_lowercase())),
            4 => view.sort_by_key(|&j| tr[j].dur),
            _ => {}
        }
        view
    }

    /// Трек из плейлиста: поиск только помогает его найти — в очередь идёт весь плейлист в его сортировке
    fn play_pl(&mut self, i: usize) {
        let Some(&j) = self.pl_view.get(i) else { return };
        let Some(pl) = self.open_pl.and_then(|p| self.store.playlists.get(p)) else { return };
        let all = self.pl_order("");
        let at = all.iter().position(|&k| k == j).unwrap_or(0);
        let q = all.iter().map(|&k| pl.tracks[k].clone()).collect();
        self.play_queue(q, at);
    }

    /// Треки открытого плейлиста в том порядке, как их видно
    fn pl_shown(&self) -> Vec<Track> {
        let Some(pl) = self.open_pl.and_then(|i| self.store.playlists.get(i)) else { return vec![] };
        self.pl_view.iter().filter_map(|&j| pl.tracks.get(j).cloned()).collect()
    }

    fn edit_playlist(&mut self) {
        let Some(pl) = self.open_pl.and_then(|i| self.store.playlists.get(i)) else { return };
        if pl.name == model::LIKED {
            return self.toast(&tr!("Любимые не переименовать — по имени их узнаёт и телефон", "Liked can't be renamed — your phone finds it by name", "Улюблені не перейменувати — за назвою їх знаходить і телефон", "Obľúbené sa nedajú premenovať — telefón ich spoznáva podľa názvu"));
        }
        self.ui.set_edit_name(pl.name.as_str().into());
        self.ui.set_edit_desc(pl.desc.as_str().into());
        self.ui.set_modal(7);
    }

    /// Новое имя (пустое — оставляем старое, занятое другим плейлистом — с номером) и описание
    fn save_playlist(&mut self, name: &str, desc: &str) {
        let Some(i) = self.open_pl.filter(|&i| i < self.store.playlists.len()) else { return };
        let name = name.trim();
        if !name.is_empty() && name != self.store.playlists[i].name {
            self.store.playlists[i].name.clear(); // своё старое имя не считается занятым
            self.store.playlists[i].name = self.unique_name(name);
        }
        self.store.playlists[i].desc = desc.trim().to_string();
        self.save();
        self.ui.set_modal(0);
        self.open_playlist(i);
    }

    fn unique_name(&self, base: &str) -> String {
        let base = if base.trim().is_empty() { tr!("Плейлист {}", "Playlist {}", "Плейлист {}", "Playlist {}", self.store.playlists.len() + 1) } else { base.trim().to_string() };
        let mut name = base.clone();
        let mut n = 2;
        while self.store.playlists.iter().any(|p| p.name == name) {
            name = format!("{base} ({n})");
            n += 1;
        }
        name
    }

    /// Трек по месту в интерфейсе: 0 поиск, 1 открытый плейлист, 2 играющий, 4 артист (популярное), 5 коллекция,
    /// 6 локальные файлы, 7 топ в статистике
    fn track_at(&self, kind: i32, idx: usize) -> Option<Track> {
        match kind {
            0 => self.view.get(idx).cloned(),
            1 => self.pl_shown().get(idx).cloned(),
            4 => self.artist.as_ref().and_then(|a| a.top.get(idx).cloned()),
            5 => self.coll.tracks.get(idx).cloned(),
            6 => self.local.get(idx).cloned(),
            7 => self.stat_top.get(idx).cloned(),
            8 => self.home_quick.get(idx).cloned(),
            _ => self.current.clone(),
        }
    }

    // ── скачивание ──

    fn dl_dir(&self) -> std::path::PathBuf {
        match self.store.settings.dl_dir.as_str() {
            "" => download::default_dir(),
            d => d.into(),
        }
    }

    /// 0 — не скачан, 1 — в очереди/качается, 2 — уже лежит в папке
    fn dl_of(&self, t: &Track) -> i32 {
        if t.src == Src::Lc {
            2 // и так на диске
        } else if self.dl_busy.contains(&key(t)) {
            1
        } else if self.dl_files.contains(&download::file_name(t)) {
            2
        } else {
            0
        }
    }

    /// Какие mp3 уже есть в папке — чтобы показать галочку и не качать второй раз
    fn scan_dl_dir(&mut self) {
        self.dl_files = std::fs::read_dir(self.dl_dir())
            .map(|rd| rd.flatten().filter_map(|e| e.file_name().into_string().ok()).filter(|n| n.ends_with(".mp3")).collect())
            .unwrap_or_default();
        self.ui.set_dl_dir(self.dl_dir().display().to_string().into());
        self.mark_playing();
    }

    /// Спросить, прежде чем качать, убирать из плейлиста или менять очередь на радио
    fn ask(&mut self, c: Confirm) {
        let (title, text, ok) = match &c {
            Confirm::Download(v) => {
                let fresh: Vec<&Track> = v.iter().filter(|t| self.dl_of(t) == 0).collect();
                let dir = self.dl_dir().display().to_string();
                match fresh.as_slice() {
                    [] => return self.toast(&tr!("Уже скачано", "Already downloaded", "Вже завантажено", "Už stiahnuté")),
                    [t] => (
                        tr!("Скачать в mp3?", "Download as mp3?", "Завантажити в mp3?", "Stiahnuť ako mp3?"),
                        tr!("«{} — {}» сохранится в {}", "“{} — {}” will be saved to {}", "«{} — {}» збережеться в {}", "„{} — {}“ sa uloží do {}", t.artist, t.title, dir),
                        tr!("Скачать", "Download", "Завантажити", "Stiahnuť"),
                    ),
                    all => (
                        tr!("Скачать {} треков?", "Download {} tracks?", "Завантажити {} треків?", "Stiahnuť {} skladieb?", all.len()),
                        tr!("Они сохранятся в {}. Уже скачанные пропущу.", "They'll be saved to {}. Already downloaded ones are skipped.", "Вони збережуться в {}. Вже завантажені пропущу.", "Uložia sa do {}. Už stiahnuté preskočím.", dir),
                        tr!("Скачать", "Download", "Завантажити", "Stiahnuť"),
                    ),
                }
            }
            Confirm::Remove(i) => {
                let Some(pl) = self.open_pl.and_then(|p| self.store.playlists.get(p)) else { return };
                let Some(t) = pl.tracks.get(*i) else { return };
                (
                    tr!("Убрать из плейлиста?", "Remove from the playlist?", "Прибрати з плейлиста?", "Odstrániť z playlistu?"),
                    tr!("«{}» пропадёт из «{}».", "“{}” will be removed from “{}”.", "«{}» зникне з «{}».", "„{}“ zmizne z „{}“.", t.title, pl.name),
                    tr!("Убрать", "Remove", "Прибрати", "Odstrániť"),
                )
            }
            Confirm::Radio(t) => (
                tr!("Включить радио?", "Start a radio?", "Увімкнути радіо?", "Zapnúť rádio?"),
                tr!("Очередь заменится треками, похожими на «{}».", "The queue will be replaced with tracks like “{}”.", "Черга заміниться треками, схожими на «{}».", "Front sa nahradí skladbami podobnými „{}“.", t.title),
                tr!("Включить", "Start", "Увімкнути", "Zapnúť"),
            ),
        };
        self.ui.set_confirm_title(title.into());
        self.ui.set_confirm_text(text.into());
        self.ui.set_confirm_ok(ok.into());
        self.ui.set_confirm_danger(matches!(c, Confirm::Remove(_)));
        self.confirm = Some(c);
        self.ui.set_modal(5);
    }

    fn confirmed(&mut self) {
        self.ui.set_modal(0);
        match self.confirm.take() {
            Some(Confirm::Download(v)) => self.download(v),
            Some(Confirm::Remove(i)) => self.remove_track(i),
            Some(Confirm::Radio(t)) => self.start_radio(t),
            None => {}
        }
    }

    fn download(&mut self, tracks: Vec<Track>) {
        let fresh: Vec<Track> = tracks.into_iter().filter(|t| self.dl_of(t) == 0).collect();
        if fresh.is_empty() {
            self.toast(&tr!("Уже скачано", "Already downloaded", "Вже завантажено", "Už stiahnuté"));
            return;
        }
        let msg = match fresh.len() {
            1 => tr!("Качаю «{}»", "Downloading “{}”", "Завантажую «{}»", "Sťahujem „{}“", fresh[0].title),
            n => tr!("Качаю {} треков", "Downloading {} tracks", "Завантажую {} треків", "Sťahujem {} skladieb", n),
        };
        for t in fresh {
            self.dl_busy.insert(key(&t));
            self.dl_queue.push_back(t);
        }
        self.toast(&msg);
        self.mark_playing();
        self.dl_pump();
    }

    /// Не больше двух загрузок разом: yt-dlp + ffmpeg на десяток треков положили бы и сеть, и проц
    fn dl_pump(&mut self) {
        self.ui.set_dl_count((self.dl_queue.len() + self.dl_active) as i32);
        while self.dl_active < 2 {
            let Some(t) = self.dl_queue.pop_front() else { break };
            self.dl_active += 1;
            let dir = self.dl_dir();
            std::thread::spawn(move || {
                let r = resolve(&t).and_then(|url| download::download(&t, &url, &dir));
                post(move |a| {
                    a.dl_active -= 1;
                    a.dl_busy.remove(&key(&t));
                    match r {
                        Ok(p) => {
                            if let Some(n) = p.file_name().and_then(|n| n.to_str()) {
                                a.dl_files.insert(n.to_string());
                            }
                            // скачанное сразу видно в «Моей музыке», без пересканирования всех папок
                            if let Some(t) = local::read(&p).filter(|t| !a.local.iter().any(|l| l.same(t))) {
                                a.local.push(t);
                                a.show_local();
                            }
                            if a.dl_queue.is_empty() && a.dl_active == 0 {
                                a.toast(&tr!("Скачано: «{}»", "Downloaded “{}”", "Завантажено: «{}»", "Stiahnuté: „{}“", t.title));
                            }
                        }
                        Err(e) => {
                            eprintln!("скачивание {}: {e:#}", t.title);
                            a.toast(&tr!("Не скачалось «{}»: {}", "Couldn't download “{}”: {}", "Не завантажилось «{}»: {}", "Nestiahlo sa „{}“: {}", t.title, e));
                        }
                    }
                    a.mark_playing();
                    a.dl_pump();
                });
            });
        }
        self.ui.set_dl_count((self.dl_queue.len() + self.dl_active) as i32);
    }

    fn pick_dl_dir(&mut self) {
        let title = tr!("Куда качать музыку", "Where to save music", "Куди завантажувати музику", "Kam sťahovať hudbu");
        let start = self.dl_dir();
        std::thread::spawn(move || {
            let d = rfd::FileDialog::new().set_title(&title).set_directory(&start).pick_folder();
            post(move |a| {
                if let Some(d) = d {
                    a.store.settings.dl_dir = d.display().to_string();
                    a.save();
                    a.scan_dl_dir();
                }
            });
        });
    }

    fn ask_add(&mut self, idx: usize, kind: i32) {
        self.add_target = self.track_at(kind, idx);
        if self.add_target.is_some() {
            self.ui.set_new_name("".into());
            self.ui.set_modal(1);
        }
    }

    fn add_to(&mut self, pl: usize) {
        self.ui.set_modal(0);
        let Some(t) = self.add_target.take() else { return };
        let Some(p) = self.store.playlists.get_mut(pl).filter(|p| p.name != model::LIKED) else { return }; // в любимые — только сердечком
        let msg = if p.tracks.iter().any(|x| x.same(&t)) {
            tr!("Уже есть в «{}»", "Already in “{}”", "Вже є в «{}»", "Už je v „{}“", p.name)
        } else {
            p.tracks.push(t);
            tr!("Закинул в «{}»", "Added to “{}”", "Закинув у «{}»", "Pridané do „{}“", p.name)
        };
        self.save();
        self.refresh_playlists();
        if self.open_pl == Some(pl) {
            self.open_playlist(pl);
        }
        self.toast(&msg);
    }

    fn create_playlist(&mut self, name: &str) {
        let adding = self.ui.get_modal() == 1;
        let name = self.unique_name(name);
        self.store.playlists.push(Playlist { name, ..Default::default() });
        let i = self.store.playlists.len() - 1;
        if adding {
            self.add_to(i);
        } else {
            self.ui.set_modal(0);
            self.save();
            self.open_playlist(i);
        }
    }

    /// Своя обложка плейлиста: картинку режем в квадрат 512px и кладём в папку настроек —
    /// исходный файл потом можно удалить или переместить
    fn pick_pl_cover(&mut self) {
        let Some(i) = self.open_pl else { return };
        let title = tr!("Обложка плейлиста", "Playlist cover", "Обкладинка плейлиста", "Obal playlistu");
        std::thread::spawn(move || {
            let Some(file) = rfd::FileDialog::new().set_title(&title).add_filter("Images", &["png", "jpg", "jpeg", "webp"]).pick_file() else { return };
            let r = (|| -> Result<std::path::PathBuf> {
                let img = image::open(&file)?;
                let side = img.width().min(img.height());
                let sq = img.crop_imm((img.width() - side) / 2, (img.height() - side) / 2, side, side);
                let sq = sq.resize_exact(512, 512, image::imageops::FilterType::Lanczos3);
                let dir = model::Store::dir().join("covers");
                std::fs::create_dir_all(&dir)?;
                let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
                let path = dir.join(format!("{stamp}.png"));
                sq.to_rgb8().save(&path)?;
                Ok(path)
            })();
            post(move |a| match r {
                Ok(path) => {
                    let Some(pl) = a.store.playlists.get_mut(i) else { return };
                    let old = std::mem::replace(&mut pl.cover, path.display().to_string());
                    if !old.is_empty() {
                        let _ = std::fs::remove_file(old);
                    }
                    a.save();
                    if a.open_pl == Some(i) {
                        a.open_playlist(i);
                    }
                }
                Err(e) => a.toast(&tr!("Не вышло: {:#}", "Failed: {:#}", "Не вийшло: {:#}", "Nevyšlo to: {:#}", e)),
            });
        });
    }

    // ── аккаунт и плейлисты в облаке ──

    fn acc_refresh_ui(&mut self) {
        let a = cloud::account();
        if self.cloud_loaded && !(a.signed_in() && a.verified) {
            // вышли (или сессия кончилась) — плейлисты этого аккаунта на экране больше не показываем
            self.cloud_loaded = false;
            self.store.playlists.clear();
            self.open_pl = None;
            self.refresh_playlists();
            self.refresh_home();
            if self.ui.get_page() == "playlist" {
                self.ui.set_page("search".into());
            }
        }
        self.ui.set_acc_on(cloud::cfg().is_some());
        let state = if !a.signed_in() { 0 } else if !a.verified { 1 } else { 2 };
        self.ui.set_acc_state(state);
        if state != 0 {
            self.ui.set_acc_reg(false); // вышел потом — окно снова начинается со входа
        }
        self.ui.set_acc_letter(a.email.chars().next().map_or("?".into(), |c| c.to_uppercase().to_string()).into());
        self.ui.set_acc_email(a.email.into());
        if state != 2 && self.ui.get_page() == "account" {
            self.ui.set_page("search".into());
        }
    }

    /// Запрос к аккаунту в фоне: Ok — что написать под кнопками
    fn acc_job(&mut self, f: impl FnOnce(&cloud::Cfg, &str) -> anyhow::Result<String> + Send + 'static) {
        let Some(c) = cloud::cfg() else { return };
        let lang = if self.store.settings.lang.is_empty() { "en".to_string() } else { self.store.settings.lang.clone() };
        self.ui.set_acc_busy(true);
        self.ui.set_acc_msg("".into());
        std::thread::spawn(move || {
            let r = f(&c, &lang);
            post(move |a| {
                a.ui.set_acc_busy(false);
                a.ui.set_acc_msg(r.unwrap_or_else(|e| acc_err(&e)).into());
                a.acc_refresh_ui();
                if cloud::account().verified {
                    a.cloud_sync(false);
                }
            });
        });
    }

    /// Слить плейлисты с облаком: при запуске, раз в минуту и по кнопке (manual — с тостом)
    fn cloud_sync(&mut self, manual: bool) {
        let Some(c) = cloud::cfg() else { return };
        let acc = cloud::account();
        if !acc.signed_in() {
            if manual {
                self.toast(&tr!("Сначала войди в аккаунт в настройках", "Sign in to your account in settings first", "Спершу увійди в акаунт у налаштуваннях", "Najprv sa v nastaveniach prihlás do účtu"));
            }
            return;
        }
        if self.syncing || self.ui.get_acc_busy() {
            return;
        }
        if !acc.verified {
            // ждём, пока нажмут ссылку в письме: проверяем тихо, подтвердили — тут же синхронизируемся
            self.acc_job(move |c, l| {
                Ok(if cloud::check_verified(c, l)? || !manual {
                    String::new()
                } else {
                    tr!("Почта ещё не подтверждена", "Email isn't confirmed yet", "Пошту ще не підтверджено", "E-mail ešte nie je potvrdený")
                })
            });
            return;
        }
        self.syncing = true;
        self.ui.set_acc_syncing(true);
        let local = self.store.playlists.clone();
        let snap = cloud::hashes(&local);
        let first = !self.cloud_loaded;
        let own = self.stats.clone();
        std::thread::spawn(move || {
            let r = if first { cloud::load(&c) } else { cloud::sync(&c, &local) };
            // статистика аккаунта: своя доля — в облако, чужие — к себе. Ошибка тут плейлистам не мешает
            let others = cloud::sync_stats(&c, &own).map_err(|e| eprintln!("stats sync: {e:#}")).ok();
            post(move |a| {
                if r.is_ok() {
                    a.conn_start(); // аккаунт живой — подключаемся к общей сессии устройств
                }
                if let Some(o) = others {
                    a.stats_others = o;
                    if a.ui.get_page() == "stats" {
                        a.open_stats();
                    }
                }
                a.syncing = false;
                a.ui.set_acc_syncing(false);
                match r {
                    // за время запроса плейлисты поправили — результат устарел, сливаем заново
                    Ok(_) if !first && cloud::hashes(&a.store.playlists) != snap => a.cloud_sync(manual),
                    Ok(s) if first => {
                        a.cloud_loaded = true;
                        cloud::update(|acc| acc.base = s.base);
                        a.apply_cloud(s.playlists);
                        a.ui.set_acc_msg("".into());
                    }
                    Ok(s) => {
                        cloud::update(|acc| acc.base = s.base);
                        if cloud::hashes(&s.playlists) != snap {
                            a.apply_cloud(s.playlists);
                        }
                        a.ui.set_acc_msg(tr!("Синхронизировано в {}", "Synced at {}", "Синхронізовано о {}", "Synchronizované o {}", stats::hm()).into());
                        if manual {
                            a.toast(&tr!("Плейлисты синхронизированы", "Playlists synced", "Плейлисти синхронізовано", "Playlisty synchronizované"));
                        }
                    }
                    Err(e) => {
                        let m = acc_err(&e);
                        if manual {
                            a.toast(&m);
                        }
                        a.ui.set_acc_msg(m.into());
                        a.acc_refresh_ui(); // сессия могла протухнуть — тогда снова форма входа
                    }
                }
            });
        });
    }

    /// Плейлисты пришли из облака: заменить свои, открытый плейлист остаётся открытым
    fn apply_cloud(&mut self, mut pls: Vec<Playlist>) {
        for p in pls.iter_mut().filter(|p| p.cover.is_empty()) {
            p.cover = self.store.pl_covers.get(&p.name).filter(|c| std::path::Path::new(c).exists()).cloned().unwrap_or_default();
        }
        let open = self.open_pl.and_then(|i| self.store.playlists.get(i)).map(|p| p.name.clone());
        for p in self.store.playlists.iter().filter(|p| !p.cover.is_empty() && !pls.iter().any(|n| n.cover == p.cover)) {
            let _ = std::fs::remove_file(&p.cover); // плейлист удалили на другом компьютере — его обложка тут больше не нужна
        }
        self.store.playlists = pls;
        self.open_pl = open.and_then(|n| self.store.playlists.iter().position(|p| p.name == n));
        self.ensure_liked();
        self.save();
        self.refresh_playlists();
        self.refresh_home();
        if self.ui.get_page() == "playlist" {
            match self.open_pl {
                Some(i) => self.open_playlist(i),
                None => self.ui.set_page("search".into()),
            }
        }
    }

    fn delete_playlist(&mut self) {
        if self.open_pl.and_then(|i| self.store.playlists.get(i)).is_some_and(|p| p.name == model::LIKED) {
            return self.toast(&tr!("Любимые не удалить — сними сердечки с треков", "Liked can't be deleted — unheart the tracks instead", "Улюблені не видалити — зніми сердечка з треків", "Obľúbené sa nedajú zmazať — zruš srdiečka pri skladbách"));
        }
        let Some(i) = self.open_pl.take() else { return };
        let p = self.store.playlists.remove(i);
        if !p.cover.is_empty() {
            let _ = std::fs::remove_file(&p.cover);
            self.store.pl_covers.remove(&p.name);
        }
        self.save();
        self.refresh_playlists();
        self.ui.set_page("search".into());
        self.toast(&tr!("«{}» улетел за горизонт событий", "“{}” fell past the event horizon", "«{}» полетів за горизонт подій", "„{}“ odletel za horizont udalostí", p.name));
    }

    /// idx — настоящий индекс в плейлисте (не строка на экране: там поиск и сортировка)
    fn remove_track(&mut self, idx: usize) {
        let Some(i) = self.open_pl else { return };
        if idx < self.store.playlists[i].tracks.len() {
            self.store.playlists[i].tracks.remove(idx);
            self.save();
            self.pl_rebuild();
            self.mark_playing();
            self.refresh_playlists();
        }
    }

    fn import(&mut self, url: String) {
        let url = url.trim().to_string();
        let apple = apple::is_apple(&url);
        let src = source_of(&url);
        if src.is_none() && !apple {
            self.ui.set_import_status(
                tr!(
                    "Это не похоже на ссылку Spotify / Apple Music / YouTube Music / SoundCloud / Яндекс Музыки",
                    "That doesn't look like a Spotify / Apple Music / YouTube Music / SoundCloud / Yandex Music link",
                    "Це не схоже на посилання Spotify / Apple Music / YouTube Music / SoundCloud / Яндекс Музики", "Toto nevyzerá ako odkaz na Spotify / Apple Music / YouTube Music / SoundCloud / Yandex Music"
                )
                .into(),
            );
            return;
        }
        self.ui.set_importing(true);
        self.ui.set_import_status("".into());
        std::thread::spawn(move || {
            if apple || src == Some(Src::Sp) {
                // Spotify и Apple Music сами не играются — сразу ищем каждый трек на YT Music
                let r = if apple { apple::import(&url) } else { spotify::import(&url) };
                match r {
                    Ok((name, tracks)) if !tracks.is_empty() => match_on_yt(name, url, tracks),
                    r => post(move |a| a.imported(r.map(|(n, t)| (n, t, None)))),
                }
                return;
            }
            let r = fetch_url(&url).map(|(name, tracks)| (name, tracks, Some(url)));
            post(move |a| a.imported(r));
        });
    }

    /// Все треки сопоставлены: уверенные сразу в плейлист, сомнительные — по одному спрашиваем юзера
    fn matched(&mut self, name: String, url: String, wants: Vec<Track>, found: Vec<Option<(Track, bool)>>) {
        let mut rv = Review { name, url, skipped: vec![], picked: vec![None; wants.len()], asks: Default::default(), missing: 0 };
        for (i, (want, f)) in wants.into_iter().zip(found).enumerate() {
            match f {
                Some((t, true)) => rv.picked[i] = Some(t),
                Some((t, false)) => rv.asks.push_back((i, want, t)),
                None => rv.missing += 1,
            }
        }
        self.review = Some(rv);
        self.next_review();
    }

    fn next_review(&mut self) {
        let Some(rv) = &self.review else { return };
        let line = |t: &Track| format!("{} — {}  ·  {}", t.artist, t.title, fmt_dur(t.dur));
        if let Some((_, want, got)) = rv.asks.front() {
            self.ui.set_ask_want(line(want).into());
            self.ui.set_ask_got(line(got).into());
            self.ui.set_ask_left(rv.asks.len() as i32);
            self.ui.set_modal(6);
            return;
        }
        let rv = self.review.take().unwrap();
        self.ui.set_modal(0);
        let missing = rv.missing;
        let before = self.store.playlists.len();
        self.imported(Ok((rv.name, rv.picked.into_iter().flatten().collect(), Some(rv.url))));
        // отвергнутые считаем пришедшими по ссылке и удалёнными — тогда QR их у получателя тоже уберёт
        if self.store.playlists.len() > before {
            if let Some(src) = self.store.playlists.last_mut().and_then(|p| p.source.as_mut()) {
                src.keys.extend(rv.skipped);
                self.save();
            }
        }
        if missing > 0 {
            let s = self.ui.get_import_status().to_string();
            self.ui.set_import_status(
                tr!("{}. Не нашлось на YT Music: {}", "{}. Not found on YT Music: {}", "{}. Не знайшлося на YT Music: {}", "{}. Na YT Music sa nenašlo: {}", s, missing).into(),
            );
        }
    }

    /// 0 пропустить, 1 оставить, 2 оставить все оставшиеся, 3 пропустить все оставшиеся
    fn review_answer(&mut self, k: i32) {
        let Some(rv) = &mut self.review else { return };
        let take = if k >= 2 { rv.asks.len() } else { 1 };
        for (i, _, got) in rv.asks.drain(..take) {
            if k == 1 || k == 2 {
                rv.picked[i] = Some(got);
            } else {
                rv.skipped.push(got.key());
            }
        }
        self.next_review();
    }

    /// Результат импорта (по ссылке или по QR) → новый плейлист. Ссылку-источник запоминаем:
    /// тогда поделиться плейлистом можно одним QR (ссылка + правки)
    fn imported(&mut self, r: Result<(String, Vec<Track>, Option<String>)>) {
        self.ui.set_importing(false);
        match r {
            Ok((name, tracks, url)) if !tracks.is_empty() => {
                let n = tracks.len();
                let name = self.unique_name(&name);
                let source = url.map(|url| model::Source { url, keys: tracks.iter().map(Track::key).collect() });
                self.store.playlists.push(Playlist { name: name.clone(), tracks, source, ..Default::default() });
                self.save();
                self.ui.set_import_url("".into());
                self.ui.set_import_status(tr!("Готово: «{}», {} треков", "Done: “{}”, {} tracks", "Готово: «{}», {} треків", "Hotovo: „{}“, {} skladieb", name, n).into());
                self.toast(&tr!("Засосал «{}» — {} треков", "Sucked in “{}” — {} tracks", "Засмоктав «{}» — {} треків", "Vcucnuté „{}“ — {} skladieb", name, n));
                self.open_playlist(self.store.playlists.len() - 1);
            }
            Ok(_) => self.ui.set_import_status(tr!("Плейлист пустой или приватный", "The playlist is empty or private", "Плейлист порожній або приватний", "Playlist je prázdny alebo súkromný").into()),
            Err(e) => self.ui.set_import_status(tr!("Не вышло: {:#}", "Failed: {:#}", "Не вийшло: {:#}", "Nevyšlo to: {:#}", e).into()),
        }
    }

    /// Выбор картинок с QR в системном диалоге (можно сразу все части большого плейлиста)
    fn import_qr(&mut self) {
        self.ui.set_importing(true);
        self.ui.set_import_status("".into());
        let title = tr!("Картинки с QR-кодом плейлиста", "Images with the playlist QR code", "Картинки з QR-кодом плейлиста", "Obrázky s QR kódom playlistu");
        std::thread::spawn(move || {
            let files = rfd::FileDialog::new().set_title(&title).add_filter("Images", &["png", "jpg", "jpeg", "webp", "bmp"]).pick_files();
            post(move |a| match files {
                None => a.ui.set_importing(false), // передумал
                Some(files) => {
                    std::thread::spawn(move || {
                        let r = qr::decode_files(&files).and_then(|shared| match shared {
                            qr::Shared::Full { name, tracks } => Ok((name, tracks, None)),
                            qr::Shared::Cloud { .. } => anyhow::bail!("QR повреждён"),
                            // по ссылке подтягиваем свежий плейлист и накатываем правки отправителя
                            qr::Shared::Linked { name, url, removed, added } => {
                                let (_, mut tracks) = match source_of(&url) {
                                    _ if apple::is_apple(&url) => fetch_on_yt(&url)?,
                                    Some(Src::Sp) => fetch_on_yt(&url)?,
                                    _ => fetch_url(&url)?,
                                };
                                let src_keys: Vec<String> = tracks.iter().map(Track::key).collect();
                                tracks.retain(|t| !removed.contains(&t.key()));
                                tracks.extend(added);
                                Ok((name, tracks, Some(url))).map(|(n, t, u)| (n, t, u.filter(|_| !src_keys.is_empty())))
                            }
                        });
                        post(move |a| a.imported(r));
                    });
                }
            });
        });
    }

    // ── QR: поделиться плейлистом ──

    fn share_playlist(&mut self) {
        let Some(pl) = self.open_pl.and_then(|i| self.store.playlists.get(i)) else { return };
        if pl.tracks.is_empty() {
            return self.toast(&tr!("Пустой плейлист нечем делиться", "Nothing to share in an empty playlist", "Порожнім плейлистом нема чим ділитися", "Prázdnym playlistom sa nedá zdieľať"));
        }
        // большой плейлист уходит в облако — это сеть, поэтому в фоне
        let pl = pl.clone();
        std::thread::spawn(move || {
            let imgs: Vec<_> = qr::codes(&pl).iter().filter_map(|c| qr::render(c, 6).map_err(|e| eprintln!("qr: {e:#}")).ok()).collect();
            post(move |a| {
                a.qr_imgs = imgs;
                a.qr_page = 0;
                a.show_qr_page();
                a.ui.set_modal(4);
            });
        });
    }

    fn show_qr_page(&mut self) {
        let Some(img) = self.qr_imgs.get(self.qr_page) else { return };
        self.ui.set_qr_img(Image::from_rgba8(slint::SharedPixelBuffer::clone_from_slice(img.as_raw(), img.width(), img.height())));
        self.ui.set_qr_part((self.qr_page + 1) as i32);
        self.ui.set_qr_total(self.qr_imgs.len() as i32);
    }

    fn save_qr(&mut self) {
        let Some(pl) = self.open_pl.and_then(|i| self.store.playlists.get(i)) else { return };
        let dir = dirs::picture_dir().or_else(dirs::home_dir).unwrap_or_else(|| ".".into()).join("BlackHole");
        let safe: String = pl.name.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' }).collect();
        let n = self.qr_imgs.len();
        let saved = std::fs::create_dir_all(&dir).is_ok()
            && self.qr_imgs.iter().enumerate().all(|(i, img)| {
                let name = if n == 1 { format!("{safe} QR.png") } else { format!("{safe} QR {}-{n}.png", i + 1) };
                img.save(dir.join(name)).is_ok()
            });
        if saved {
            self.toast(&tr!("Сохранил в {}", "Saved to {}", "Зберіг у {}", "Uložené do {}", dir.display()));
            open_url(&dir.display().to_string());
        } else {
            self.toast(&tr!("Не смог сохранить QR", "Couldn't save the QR code", "Не зміг зберегти QR", "QR kód sa nepodarilo uložiť"));
        }
    }

    // ── радио по треку ──

    fn start_radio(&mut self, t: Track) {
        self.toast(&tr!("Собираю радио по «{}»…", "Building a radio from “{}”…", "Збираю радіо за «{}»…", "Skladám rádio podľa „{}“…", t.title));
        std::thread::spawn(move || {
            let r = radio_for(&t);
            post(move |a| match r {
                Ok(mut tracks) => {
                    // первым — сам трек в исходном виде (с его сервисом и обложкой)
                    tracks.retain(|x| !(x.title == t.title && x.artist == t.artist));
                    tracks.insert(0, t.clone());
                    let name = a.unique_name(&format!("📻 {}", t.title));
                    a.store.playlists.push(Playlist { name, tracks: tracks.clone(), source: None, ..Default::default() });
                    a.save();
                    a.open_playlist(a.store.playlists.len() - 1);
                    if a.current.as_ref().is_some_and(|c| c.same(&t)) {
                        // трек уже играет — не перезапускаем, просто подставляем радио следующим
                        a.source = tracks.clone();
                        a.upnext = if a.shuffle { shuffled(tracks[1..].to_vec()) } else { tracks[1..].to_vec() };
                        a.queue_changed();
                    } else {
                        a.play_queue(tracks, 0);
                    }
                }
                Err(e) => a.toast(&tr!("Радио не собралось: {:#}", "Couldn't build a radio: {:#}", "Радіо не зібралося: {:#}", "Rádio sa nepodarilo poskladať: {:#}", e)),
            });
        });
    }

    // ── воспроизведение ──

    fn mark_playing(&self) {
        let cur = self.current.as_ref();
        let is = |t: &Track| cur.is_some_and(|c| c.same(t));
        let shown = self.pl_shown();
        let pl = &shown[..];
        let top = self.artist.as_ref().map(|a| &a.top[..]).unwrap_or(&[]);
        for (m, tracks) in [(&self.res_model, &self.view[..]), (&self.pl_model, pl), (&self.art_top_model, top), (&self.coll_model, &self.coll.tracks[..]), (&self.local_model, &self.local[..])] {
            for (i, t) in tracks.iter().enumerate() {
                if let Some(mut r) = m.row_data(i) {
                    let dl = self.dl_of(t);
                    let liked = self.is_liked(t);
                    if r.playing != is(t) || r.dl != dl || r.liked != liked {
                        r.playing = is(t);
                        r.dl = dl;
                        r.liked = liked;
                        m.set_row_data(i, r);
                    }
                }
            }
        }
    }

    fn play_queue(&mut self, q: Vec<Track>, idx: usize) {
        let Some(t) = q.get(idx).cloned() else { return };
        self.source = q;
        // без шафла "назад" ведёт к предыдущему по списку, в шафле истории ещё нет
        self.played = if self.shuffle { vec![] } else { self.source[..idx].to_vec() };
        self.upnext = self.build_upnext(&t);
        self.play_now(t);
    }

    /// Что будет после текущего: в шафле — весь список вперемешку, иначе — хвост списка после него
    fn build_upnext(&self, cur: &Track) -> Vec<Track> {
        if self.shuffle {
            shuffled(self.source.iter().filter(|t| !t.same(cur)).cloned().collect())
        } else {
            let at = self.source.iter().position(|t| t.same(cur)).map_or(0, |i| i + 1);
            self.source[at..].to_vec()
        }
    }

    fn play_now(&mut self, t: Track) {
        self.end_xfade();
        self.current = Some(t);
        self.update_liked();
        self.retries = 0;
        self.resume_at = None;
        if self.played.len() > 500 {
            self.played.drain(..100);
        }
        self.start();
        self.refresh_queue();
    }

    fn refresh_queue(&self) {
        // история — сверху свежие
        let hist: Vec<TrackItem> = self.played.iter().rev().take(100).map(|t| self.item(t)).collect();
        let next: Vec<TrackItem> = self.upnext.iter().map(|t| self.item(t)).collect();
        self.hist_model.set_vec(hist);
        self.next_model.set_vec(next);
    }

    /// После правки очереди следующий трек мог смениться — перезапускаем предзагрузку
    fn queue_changed(&mut self) {
        self.refresh_queue();
        if self.conn_queue(false) {
            return; // очередь живёт на устройстве со звуком
        }
        self.prefetched = false;
        if self.pos > 1.0 {
            self.prefetch();
        }
    }

    fn start(&mut self) {
        let Some(t) = self.current.clone() else { return };
        // звук на другом устройстве — включаем там, с этой очередью
        if self.conn_queue(true) {
            return;
        }
        self.restored = false;
        let Some(player) = self.player.clone() else {
            self.toast(&tr!("mpv не найден — играть нечем", "mpv not found — nothing to play with", "mpv не знайдено — нема чим грати", "mpv sa nenašlo — nie je čím hrať"));
            return;
        };
        let ui = &self.ui;
        ui.set_now_title(t.title.as_str().into());
        ui.set_now_artist(t.artist.as_str().into());
        ui.set_now_source(t.src.tag().into());
        let cover = self.covers.get(&t.cover).cloned();
        ui.set_now_has_cover(cover.is_some());
        ui.set_now_cover(cover.unwrap_or_default());
        ui.set_has_track(true);
        ui.set_loading(true);
        ui.set_playing(true);
        ui.set_position(0.0);
        ui.set_pos_text("0:00".into());
        ui.set_dur_text(fmt_dur(t.dur).into());
        self.want_cover(&t.cover);
        self.current = Some(t.clone());
        self.update_liked();
        self.pos = 0.0;
        self.lyr_shown = 0.0;
        self.prefetched = false;
        self.load_started = Instant::now();
        self.listened = 0.0;
        self.listen_at = None;
        let file = local_file(&t);
        let disk = file.is_none().then(|| self.offline_file(&t)).flatten();
        // битый локальный файл перезапрашивать бессмысленно — это не протухшая ссылка. А вот файл из
        // офлайн-кэша или протухшую ссылку — да: при ошибке их выкинем и пойдём в сеть
        self.from_cache = file.is_none() && (disk.is_some() || self.stream_cache.get(&key(&t)).is_some_and(|(_, at)| at.elapsed() < STREAM_TTL));
        let cached = file.or(disk).or_else(|| self.stream_cache.get(&key(&t)).filter(|(_, at)| at.elapsed() < STREAM_TTL).map(|(u, _)| u.clone()));
        self.mark_playing();
        self.update_lyr_timer();

        if let Some(m) = &mut self.media {
            // http-обложку панель покажет сразу; файл подменит её, когда скачается большая
            m.metadata(&t.title, &t.artist, web_cover(&t), t.dur as f64);
            m.playback(true, 0.0, true);
        }
        let tok = self.token.fetch_add(1, Ordering::SeqCst) + 1;
        // текст грузим один раз на трек: повторные попытки открыть поток его не трогают
        if self.lyr_for != key(&t) {
            self.load_lyrics(&t);
        }
        let token = self.token.clone();
        std::thread::spawn(move || {
            // прямую ссылку достаём сами, а не хуком mpv: так видна настоящая причина, если YouTube упёрся
            let r = match cached {
                Some(u) => Ok(u),
                None => resolve(&t).and_then(|u| ytmusic::stream_url(&u)),
            };
            if token.load(Ordering::SeqCst) != tok {
                return; // юзер уже ткнул другой трек
            }
            match r {
                Ok(url) => {
                    // запоминаем и текущий: потом на него вернутся кнопкой "назад"
                    let (k, u) = (key(&t), url.clone());
                    post(move |a| a.cache_stream(k, u));
                    player.send(Cmd::Load(url, tok))
                }
                Err(e) => post(move |a| {
                    if a.token.load(Ordering::SeqCst) == tok {
                        eprintln!("поток {}: {e:#}", t.title);
                        a.track_failed(&e.to_string());
                    }
                }),
            }
        });
    }

    /// Трек не заиграл: пропускаем, но если подряд не играет уже третий — дальше листать бессмысленно
    /// (обычно YouTube упёрся в проверку на бота), останавливаемся и говорим, что делать
    fn track_failed(&mut self, e: &str) {
        self.fail_streak += 1;
        let bot = e.contains(util::BOT);
        let msg = if bot {
            tr!("YouTube просит доказать, что ты не бот. Включи «Куки YouTube» в настройках", "YouTube wants proof you're not a bot. Turn on “YouTube cookies” in settings", "YouTube просить довести, що ти не бот. Увімкни «Куки YouTube» у налаштуваннях", "YouTube chce dôkaz, že nie si bot. Zapni „Cookies YouTube“ v nastaveniach")
        } else {
            tr!("Не играет: {}", "Can't play: {}", "Не грає: {}", "Nehrá: {}", e)
        };
        if self.fail_streak >= 3 || (bot && self.store.settings.cookies.is_empty()) {
            self.fail_streak = 0;
            self.stop();
            self.toast(&msg);
            return;
        }
        self.toast(&msg);
        self.advance();
    }

    fn advance(&mut self) {
        if self.conn_cmd("next", serde_json::Value::Null) {
            return;
        }
        if self.upnext.is_empty() && self.repeat > 0 && !self.source.is_empty() {
            self.upnext = if self.shuffle { shuffled(self.source.clone()) } else { self.source.clone() };
        }
        if self.upnext.is_empty() {
            return self.stop();
        }
        let next = self.upnext.remove(0);
        if let Some(c) = self.current.take() {
            self.played.push(c);
        }
        self.play_now(next);
    }

    fn cache_stream(&mut self, k: String, u: String) {
        self.stream_cache.retain(|_, (_, at)| at.elapsed() < STREAM_TTL);
        self.stream_cache.insert(k, (u, Instant::now()));
    }

    /// Пока текущий играет — заранее достаём прямые ссылки на 5 следующих и 3 прошлых: можно скипать
    /// в обе стороны и не ждать. Один фоновый поток с nice 19: сначала ближайший следующий отдельно
    /// (чтобы был готов побыстрее), остальные одной пачкой — yt-dlp на трек это секунды CPU
    fn prefetch(&mut self) {
        if self.prefetched {
            return;
        }
        self.prefetched = true;
        let todo: Vec<Track> = self
            .upnext
            .iter()
            .take(PREFETCH_AHEAD)
            .chain(self.played.iter().rev().take(PREFETCH_BEHIND))
            .filter(|t| !self.stream_cache.get(&key(t)).is_some_and(|(_, at)| at.elapsed() < STREAM_TTL))
            .filter(|t| !self.inflight.contains(&key(t)) && local_file(t).is_none() && !cache::has(&key(t)))
            .cloned()
            .collect();
        if todo.is_empty() {
            return;
        }
        for t in &todo {
            self.inflight.insert(key(t));
        }
        std::thread::spawn(move || {
            // приоритет потока наследуют yt-dlp и node — музыка и интерфейс их не замечают
            #[cfg(target_os = "linux")]
            unsafe {
                libc::setpriority(libc::PRIO_PROCESS, 0, 19)
            };
            let (first, rest) = todo.split_at(1);
            for batch in [first, rest] {
                // спотик/саундклауд сначала ищем на YT Music, это обычные http-запросы
                let urls: Vec<(Track, Option<String>)> = batch.iter().map(|t| (t.clone(), resolve(t).ok())).collect();
                let want: Vec<String> = urls.iter().filter_map(|(_, u)| u.clone()).collect();
                let got = if want.is_empty() { Default::default() } else { ytmusic::stream_urls(&want).unwrap_or_else(|e| {
                    eprintln!("prefetch: {e:#}");
                    Default::default()
                }) };
                post(move |a| {
                    for (t, u) in urls {
                        a.inflight.remove(&key(&t));
                        if let Some(s) = u.and_then(|u| got.get(&u).cloned()) {
                            a.cache_stream(key(&t), s);
                        }
                    }
                });
            }
        });
    }

    fn prev(&mut self) {
        if self.conn_cmd("prev", serde_json::Value::Null) {
            return;
        }
        // как везде: если трек уже играет больше 3 с — "назад" перематывает его в начало
        if self.pos > 3.0 {
            return self.seek_abs(0.0);
        }
        match self.played.pop() {
            Some(p) => {
                if let Some(c) = self.current.take() {
                    self.upnext.insert(0, c);
                }
                self.play_now(p);
            }
            None => self.seek_abs(0.0),
        }
    }

    // ── правка очереди из панели ──

    fn queue_jump(&mut self, i: usize) {
        if i >= self.upnext.len() {
            return;
        }
        let t = self.upnext.remove(i);
        if let Some(c) = self.current.take() {
            self.played.push(c);
        }
        self.play_now(t);
    }

    /// i — индекс в показанной истории (0 = самый свежий)
    fn history_jump(&mut self, i: usize) {
        let Some(at) = self.played.len().checked_sub(i + 1) else { return };
        // всё, что играло после выбранного, и текущий возвращаются в начало "далее"
        let mut tail = self.played.split_off(at);
        let t = tail.remove(0);
        if let Some(c) = self.current.take() {
            tail.push(c);
        }
        tail.append(&mut self.upnext);
        self.upnext = tail;
        self.play_now(t);
    }

    fn queue_move(&mut self, i: usize, delta: i32) {
        let j = i as i64 + delta as i64;
        if i < self.upnext.len() && j >= 0 && (j as usize) < self.upnext.len() {
            self.upnext.swap(i, j as usize);
            self.queue_changed();
        }
    }

    fn queue_remove(&mut self, i: usize) {
        if i < self.upnext.len() {
            self.upnext.remove(i);
            self.queue_changed();
        }
    }

    fn stop(&mut self) {
        self.end_xfade();
        self.token.fetch_add(1, Ordering::SeqCst);
        if let Some(p) = &self.player {
            p.send(Cmd::Stop);
        }
        // mpv пустой: play снова загрузит этот трек с начала
        self.restored = self.current.is_some();
        self.pos = 0.0;
        self.listen_at = None;
        self.ui.set_playing(false);
        self.ui.set_loading(false);
        self.ui.set_position(0.0);
        self.ui.set_pos_text("0:00".into());
        self.update_lyr_timer();
        if let Some(m) = &mut self.media {
            m.stopped();
        }
        self.discord_update();
    }

    /// Что играло при закрытии — в плеер на паузе. Поток не грузим, пока юзер не нажмёт play
    fn restore_last(&mut self) {
        let Some(l) = self.store.last.take() else { return };
        (self.played, self.upnext, self.source) = (l.played, l.upnext, l.source);
        self.show_paused(l.track, l.pos);
        self.refresh_queue();
    }

    /// Трек в плеере на паузе, но в mpv не загружен: play загрузит и продолжит с pos
    fn show_paused(&mut self, t: Track, pos: f64) {
        self.pos = pos;
        self.dur = t.dur as f64;
        let ui = &self.ui;
        ui.set_now_title(t.title.as_str().into());
        ui.set_now_artist(t.artist.as_str().into());
        ui.set_now_source(t.src.tag().into());
        ui.set_has_track(true);
        ui.set_playing(false);
        ui.set_position(if t.dur > 0 { (pos / t.dur as f64) as f32 } else { 0.0 });
        ui.set_pos_text(fmt_dur(pos as u64).into());
        ui.set_dur_text(fmt_dur(t.dur).into());
        let cover = self.covers.get(&t.cover).cloned();
        ui.set_now_has_cover(cover.is_some());
        ui.set_now_cover(cover.unwrap_or_default());
        self.want_cover(&t.cover);
        if let Some(m) = &mut self.media {
            m.metadata(&t.title, &t.artist, web_cover(&t), t.dur as f64);
            m.playback(false, pos, true);
        }
        self.current = Some(t);
        self.update_liked();
        self.restored = true;
        self.mark_playing();
    }

    fn save_last(&mut self) {
        self.store.last = self.current.clone().map(|track| model::Last {
            track,
            pos: if self.pos.is_finite() { self.pos } else { 0.0 }, // NaN в json — null, и файл бы не прочитался
            played: self.played.iter().rev().take(100).rev().cloned().collect(),
            upnext: self.upnext.clone(),
            source: self.source.clone(),
        });
        self.save();
    }

    fn toggle(&mut self) {
        if self.conn_cmd("toggle", serde_json::Value::Null) {
            return;
        }
        if self.restored {
            // трек из прошлого запуска: грузим и продолжаем с того же места
            self.resume_at = Some(self.pos).filter(|p| *p > 1.0);
            return self.start();
        }
        if self.current.is_none() {
            if !self.view.is_empty() {
                self.play_queue(self.view.clone(), 0);
            }
            return;
        }
        self.end_xfade();
        if let Some(p) = &self.player {
            p.send(Cmd::Toggle);
        }
        self.ui.set_playing(!self.ui.get_playing());
        self.update_lyr_timer();
        let (playing, pos) = (self.ui.get_playing(), self.pos);
        if let Some(m) = &mut self.media {
            m.playback(playing, pos, true);
        }
        self.discord_update();
    }

    fn seek_abs(&mut self, s: f64) {
        if self.conn_cmd("seek", serde_json::json!(s)) {
            self.pos = s;
            self.pos_at = Instant::now();
            return;
        }
        self.end_xfade();
        if let Some(p) = &self.player {
            p.send(Cmd::Seek(s));
        }
        self.pos = s;
        self.pos_at = Instant::now();
        self.lyr_shown = s;
        self.seek_to = Some((s, Instant::now()));
        let playing = self.ui.get_playing();
        if let Some(m) = &mut self.media {
            m.playback(playing, s, true);
        }
        self.lyr_tick();
    }

    fn on_ev(&mut self, ev: Ev) {
        let cur = self.token.load(Ordering::SeqCst);
        match ev {
            Ev::Status { tok, pos, dur, paused, loading } if tok == cur && tok != 0 => {
                // старая позиция сразу после перемотки — игнорим, иначе текст прыгает туда-обратно
                if let Some((target, at)) = self.seek_to {
                    if at.elapsed() < Duration::from_millis(1500) && (pos - target).abs() > 1.5 {
                        return;
                    }
                    self.seek_to = None;
                }
                self.pos = pos;
                self.pos_at = Instant::now();
                if dur > 0.0 {
                    self.dur = dur;
                    self.ui.set_position((pos / dur) as f32);
                    self.ui.set_dur_text(fmt_dur(dur as u64).into());
                }
                let p = fmt_dur(pos as u64);
                self.ui.set_pos_text(if p.is_empty() { "0:00".into() } else { p.into() });
                self.ui.set_loading(loading);
                self.count_listen(!loading && !paused);
                if loading && self.load_started.elapsed() > LOAD_TIMEOUT {
                    self.toast(&tr!("Поток не грузится — пропускаю", "The stream won't load — skipping", "Потік не вантажиться — пропускаю", "Stream sa nenačítava — preskakujem"));
                    self.advance();
                    return;
                }
                if !loading {
                    if let Some(p) = self.resume_at.take() {
                        self.seek_abs(p);
                    }
                }
                if !loading && pos > 1.0 {
                    self.fail_streak = 0;
                    self.prefetch();
                    self.maybe_autoplay();
                }
                if !loading && self.ui.get_playing() == paused {
                    self.ui.set_playing(!paused);
                    self.update_lyr_timer();
                }
                if !loading {
                    if let Some(m) = &mut self.media {
                        m.playback(!paused, pos, false);
                    }
                }
                self.discord_update();
                self.maybe_xfade();
            }
            // таймер сна «после этого трека»: следующий встаёт в плеер на паузе
            Ev::Ended(tok) if tok == cur && self.sleep_mode == 4 => {
                self.set_sleep(0);
                self.advance_paused();
            }
            // повтор трека: тот же трек заново (ссылка в кэше — без ожидания)
            Ev::Ended(tok) if tok == cur && self.repeat == 2 => {
                if let Some(t) = self.current.clone() {
                    self.play_now(t);
                }
            }
            Ev::Ended(tok) if tok == cur => self.advance(),
            // закэшированная ссылка протухла — молча резолвим заново
            // протухшая ссылка из кэша или кривой формат от YouTube — молча перезапрашиваем поток
            Ev::Failed(tok, e) if tok == cur && (self.from_cache || self.retries < 2) => {
                eprintln!("поток не открылся ({e}), пробую ещё раз");
                if let Some(t) = &self.current {
                    self.stream_cache.remove(&key(t));
                    cache::forget(&key(t));
                }
                if !self.from_cache {
                    self.retries += 1;
                }
                self.start();
            }
            Ev::Failed(tok, e) if tok == cur && tok != 0 => self.track_failed(&e),
            Ev::Failed(0, e) => self.toast(&tr!("Не играет: {}", "Can't play: {}", "Не грає: {}", "Nehrá: {}", e)),
            _ => {}
        }
    }

    fn advance_paused(&mut self) {
        if self.upnext.is_empty() {
            return self.stop();
        }
        let next = self.upnext.remove(0);
        if let Some(c) = self.current.take() {
            self.played.push(c);
        }
        self.stop();
        self.show_paused(next, 0.0);
        self.refresh_queue();
    }

    // ── кроссфейд ──

    fn spawn_player(&self, volume: f64) -> Option<Player> {
        let p = Player::spawn(volume, |ev| post(move |a| a.on_ev(ev))).map_err(|e| eprintln!("{e:#}")).ok()?;
        let st = &self.store.settings;
        p.send(Cmd::Af(fx::af(&st.eq)));
        p.send(Cmd::Speed(st.speed as f64));
        p.send(Cmd::KeepPitch(st.keep_pitch));
        Some(p)
    }

    /// Громкость mpv кубическая: громкость v даёт амплитуду (v/100)³, поэтому доля k амплитуды — это v·∛k
    fn vol_at(&self, k: f64) -> f64 {
        self.store.settings.volume as f64 * 100.0 * k.clamp(0.0, 1.0).cbrt()
    }

    /// Скоро конец, а следующий готов (файл или ссылка в кэше) — запускаем его во втором mpv с нуля громкости.
    /// Не готов — обычный переход: ждать yt-dlp посреди кроссфейда хуже, чем его не делать
    fn maybe_xfade(&mut self) {
        let xf = self.store.settings.xfade as f64;
        if xf <= 0.0 || self.repeat == 2 || self.xf.is_some() || self.sleep_mode == 4 || self.dur < xf * 3.0 || self.pos < self.dur - xf || !self.ui.get_playing() {
            return;
        }
        let Some(next) = self.upnext.first() else { return };
        if local_file(next).is_none() && !cache::has(&key(next)) && !self.stream_cache.get(&key(next)).is_some_and(|(_, at)| at.elapsed() < STREAM_TTL) {
            return;
        }
        if self.player2.is_none() {
            self.player2 = self.spawn_player(0.0);
        }
        let Some(fresh) = self.player2.take() else { return };
        fresh.send(Cmd::Volume(0.0));
        let Some(old) = self.player.replace(fresh) else { return };
        self.advance();
        self.xf = Some((old, Instant::now()));
        self.xf_timer.start(TimerMode::Repeated, Duration::from_millis(50), || {
            with(|a| a.xf_step());
        });
    }

    fn xf_step(&mut self) {
        let Some((old, t0)) = &self.xf else { return self.xf_timer.stop() };
        let k = (t0.elapsed().as_secs_f64() / self.store.settings.xfade.max(0.1) as f64).min(1.0);
        // равная мощность: sin² + cos² = 1 — в середине перехода громкость не проваливается
        let a = k * std::f64::consts::FRAC_PI_2;
        old.send(Cmd::Volume(self.vol_at(a.cos())));
        if let Some(p) = &self.player {
            p.send(Cmd::Volume(self.vol_at(a.sin())));
        }
        if k >= 1.0 {
            self.end_xfade();
        }
    }

    /// Переход доиграл или его прервали (пауза, перемотка, другой трек): старый молчит, новый в полный голос
    fn end_xfade(&mut self) {
        let Some((old, _)) = self.xf.take() else { return };
        self.xf_timer.stop();
        old.send(Cmd::Stop);
        self.player2 = Some(old);
        if let Some(p) = &self.player {
            p.send(Cmd::Volume(self.vol_at(1.0)));
        }
    }

    // ── офлайн-кэш ──

    fn offline_file(&self, t: &Track) -> Option<String> {
        self.store.settings.offline.then(|| cache::get(&key(t))).flatten()
    }

    /// Трек слушают — докачиваем его на диск по прямой ссылке, по которой он сейчас играет
    fn cache_track(&self, t: &Track) {
        let k = key(t);
        if !self.store.settings.offline || t.src == Src::Lc || cache::has(&k) {
            return;
        }
        let Some((url, _)) = self.stream_cache.get(&k) else { return };
        let limit = (self.store.settings.cache_gb.max(0.25) as f64 * 1024.0 * 1024.0 * 1024.0) as u64;
        cache::fetch(k, url.clone(), limit, |size| post(move |a| a.show_cache_size(size)));
    }

    fn show_cache_size(&self, bytes: u64) {
        let mb = bytes / (1024 * 1024);
        let s = if mb >= 1024 { format!("{:.1} ГБ", mb as f64 / 1024.0) } else { format!("{mb} МБ") };
        self.ui.set_cache_used(if matches!(LANG.load(Ordering::Relaxed), 1 | 3) { s.replace("ГБ", "GB").replace("МБ", "MB") } else { s }.into());
    }

    // ── главная ──

    fn card_of(&self, title: &str, sub: &str, cover: &str) -> Card {
        let img = self.covers.get(cover).cloned();
        Card { title: title.into(), sub: sub.into(), has_cover: img.is_some(), cover: img.unwrap_or_default(), cover_url: cover.into() }
    }

    /// Главная из того, что уже есть: миксы (home.json), недавнее (статистика), плейлисты
    fn refresh_home(&mut self) {
        let ui = &self.ui;
        ui.set_home_hello(
            match stats::hour() {
                5..=11 => tr!("Доброе утро", "Good morning", "Доброго ранку", "Dobré ráno"),
                12..=17 => tr!("Добрый день", "Good afternoon", "Добрий день", "Dobrý deň"),
                18..=23 => tr!("Добрый вечер", "Good evening", "Добрий вечір", "Dobrý večer"),
                _ => tr!("Доброй ночи", "Good night", "Доброї ночі", "Dobrú noc"),
            }
            .into(),
        );
        let mixes: Vec<Card> = self.home.mixes.iter().map(|m| self.card_of(&m.title, &m.sub, &m.cover)).collect();
        self.home_mixes_model.set_vec(mixes);
        // быстрый подбор: из открытий (новое), а если их нет — из главного микса
        let src = self.home.mixes.iter().rev().find(|m| m.tracks.len() > 5).map(|m| m.tracks.clone()).unwrap_or_default();
        self.home_quick = src.into_iter().take(8).collect();
        let quick: Vec<TrackItem> = self.home_quick.iter().map(|t| self.item(t)).collect();
        self.home_quick_model.set_vec(quick);
        self.home_recent = self.stats.with(&self.stats_others).recent(12).into_iter().map(|t| t.track.clone()).collect();
        let recent: Vec<Card> = self.home_recent.iter().map(|t| self.card_of(&t.title, &t.artist, &t.cover)).collect();
        self.home_recent_model.set_vec(recent);
        let pls: Vec<(usize, Card)> = self
            .store
            .playlists
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let sub = tr!("{} треков", "{} tracks", "{} треків", "{} skladieb", p.tracks.len());
                let own = if p.name == model::LIKED { Some(liked_art()) } else { (!p.cover.is_empty()).then(|| Image::load_from_path(std::path::Path::new(&p.cover)).ok()).flatten() };
                let card = match own {
                    Some(img) => Card { title: pl_name(&p.name).into(), sub: sub.into(), has_cover: true, cover: img, cover_url: "".into() },
                    None => self.card_of(&p.name, &sub, p.tracks.first().map_or("", |t| t.cover.as_str())),
                };
                (i, card)
            })
            .collect();
        self.home_pls = pls.iter().map(|p| p.0).collect();
        self.home_pls_model.set_vec(pls.into_iter().map(|p| p.1).collect::<Vec<_>>());
        let sim: Vec<Card> = self.home.similar.iter().map(|c| self.card(c)).collect();
        self.home_similar_model.set_vec(sim);
        self.ui.set_home_similar_to(self.home.similar_to.as_str().into());
        self.ui.set_home_empty(self.stats.tracks.is_empty() && self.stats_others.tracks.is_empty());
    }

    /// Миксы пересобираем раз в день (или по кнопке): пара запросов к YT Music в фоне
    fn build_home(&mut self, force: bool) {
        let st = self.stats.with(&self.stats_others);
        if self.home_busy || st.top_tracks(1).is_empty() || (!force && self.home.day == stats::today() && !self.home.mixes.is_empty()) {
            return;
        }
        self.home_busy = true;
        self.ui.set_home_loading(true);
        let names = (tr!("Твой микс", "Your mix", "Твій мікс", "Tvoj mix"), tr!("Открытия", "Discover", "Відкриття", "Objavy"), tr!("Микс: {}", "{} mix", "Мікс: {}", "Mix: {}", "{}"));
        std::thread::spawn(move || {
            let h = home::build(&st, radio_for, |n| ytmusic::find_artist(n).and_then(|id| ytmusic::artist(&id)).map(|a| a.similar).unwrap_or_default(), names);
            post(move |a| {
                a.home_busy = false;
                a.ui.set_home_loading(false);
                if !h.mixes.is_empty() {
                    home::save(&h);
                    a.home = h;
                }
                a.refresh_home();
            });
        });
    }

    // ── статистика ──

    /// Время между опросами mpv, пока звук шёл, — в статистику. Прослушивание засчитывается с 30 с
    /// (или с половины, если трек короче минуты)
    fn count_listen(&mut self, sounding: bool) {
        let now = Instant::now();
        let prev = std::mem::replace(&mut self.listen_at, sounding.then_some(now));
        let (Some(prev), true, Some(t)) = (prev, sounding, self.current.as_ref()) else { return };
        let dt = now.duration_since(prev).as_secs_f64();
        if dt > 2.0 {
            return; // висели или спали — это не слушание
        }
        let need = if self.dur > 0.0 { (self.dur * 0.5).min(30.0) } else { 30.0 };
        if self.listened < need && self.listened + dt >= need {
            self.stats.play(t);
            let t = t.clone();
            self.cache_track(&t);
        }
        self.listened += dt;
        self.stats.add(t, dt, stats::today());
        if self.stats_saved.elapsed() > Duration::from_secs(60) {
            self.stats.save();
            self.stats_saved = now;
        }
    }

    fn open_stats(&mut self) {
        let today = stats::today();
        let s = &self.stats.with(&self.stats_others);
        let ui = &self.ui;
        ui.set_st_today(fmt_long(s.last(today, 1)).into());
        ui.set_st_week(fmt_long(s.last(today, 7)).into());
        ui.set_st_month(fmt_long(s.last(today, 30)).into());
        ui.set_st_total(fmt_long(s.days.values().sum()).into());
        // год клетками, как на GitHub: колонка — неделя с понедельника, последняя — текущая
        let start = today - stats::weekday(today) - 52 * 7;
        let cells: Vec<HeatCell> = (start..=today)
            .map(|d| {
                let ms = s.days.get(&d).copied().unwrap_or(0);
                let (y, m, dd) = stats::ymd(d);
                let what = if ms == 0 { tr!("ничего не играло", "nothing played", "нічого не грало", "nič nehralo") } else { fmt_long(ms) };
                HeatCell { level: stats::level(ms), tip: format!("{dd:02}.{m:02}.{y} — {what}").into() }
            })
            .collect();
        let names = [
            tr!("янв", "Jan", "січ", "jan"), tr!("фев", "Feb", "лют", "feb"), tr!("мар", "Mar", "бер", "mar"), tr!("апр", "Apr", "кві", "apr"),
            tr!("май", "May", "тра", "máj"), tr!("июн", "Jun", "чер", "jún"), tr!("июл", "Jul", "лип", "júl"), tr!("авг", "Aug", "сер", "aug"),
            tr!("сен", "Sep", "вер", "sep"), tr!("окт", "Oct", "жов", "okt"), tr!("ноя", "Nov", "лис", "nov"), tr!("дек", "Dec", "гру", "dec"),
        ];
        // подпись месяца — над неделей, в которой он начинается
        let months: Vec<MonthMark> = (0..53)
            .filter_map(|c| {
                let (_, m, d) = stats::ymd(start + c * 7);
                (d <= 7 && c > 0).then(|| MonthMark { col: c as i32, text: names[m as usize - 1].as_str().into() })
            })
            .collect();
        ui.set_st_heat(Rc::new(VecModel::from(cells)).into());
        ui.set_st_months(Rc::new(VecModel::from(months)).into());
        let top: Vec<&stats::TrackStat> = s.top_tracks(5);
        self.stat_top = top.iter().map(|t| t.track.clone()).collect();
        let items: Vec<TrackItem> = top.iter().map(|t| TrackItem { duration: format!("{}×", t.plays).into(), ..self.item(&t.track) }).collect();
        let artists: Vec<StatArtist> = s.top_artists(5).into_iter().map(|(n, ms)| StatArtist { name: n.into(), time: fmt_long(ms).into() }).collect();
        self.stat_artists = artists.iter().map(|a| a.name.to_string()).collect();
        ui.set_st_tracks(Rc::new(VecModel::from(items)).into());
        ui.set_st_more(Rc::new(VecModel::from(artists.iter().skip(1).cloned().collect::<Vec<_>>())).into());
        ui.set_st_artists(Rc::new(VecModel::from(artists)).into());
        ui.set_page("stats".into());
    }

    // ── дальше похожее ──

    /// Очередь кончается (остался последний трек) — подбираем похожие по играющему и докидываем в конец
    fn maybe_autoplay(&mut self) {
        if !self.store.settings.autoplay || self.repeat > 0 || self.upnext.len() >= 2 {
            return;
        }
        let Some(seed) = self.current.clone() else { return };
        if self.auto_for == key(&seed) {
            return;
        }
        self.auto_for = key(&seed);
        std::thread::spawn(move || {
            let r = radio_for(&seed);
            post(move |a| {
                let Ok(tracks) = r.map_err(|e| eprintln!("дальше похожее: {e:#}")) else { return };
                if a.repeat > 0 || a.upnext.len() >= 2 {
                    return; // пока искали, юзер сам набил очередь
                }
                let seen: HashSet<String> = a.played.iter().chain(&a.upnext).chain(&a.current).chain(&a.source).map(same_song).collect();
                let fresh: Vec<Track> = tracks.into_iter().filter(|t| !seen.contains(&same_song(t))).take(25).collect();
                if fresh.is_empty() {
                    return;
                }
                a.upnext.extend(fresh);
                a.queue_changed();
                a.toast(&tr!("Дальше — похожее на «{}»", "Up next: tracks like “{}”", "Далі — схоже на «{}»", "Ďalej — podobné ako „{}“", seed.title));
            });
        });
    }

    // ── таймер сна ──

    /// 0 выкл, 1–3 через 15/30/60 мин, 4 после этого трека
    fn set_sleep(&mut self, mode: i32) {
        let mins = [0, 15, 30, 60, 0][mode.clamp(0, 4) as usize];
        self.sleep_mode = mode.clamp(0, 4);
        self.sleep_at = (mins > 0).then(|| Instant::now() + Duration::from_secs(mins * 60));
        self.ui.set_sleep_mode(self.sleep_mode);
        if mins > 0 {
            // 4 раза в секунду: подпись меняется раз в минуту, а последние 10 с громкость плавно уходит
            self.sleep_timer.start(TimerMode::Repeated, Duration::from_millis(250), || {
                with(|a| a.sleep_tick());
            });
        } else {
            self.sleep_timer.stop();
        }
        self.sleep_tick();
    }

    fn sleep_tick(&mut self) {
        let Some(at) = self.sleep_at else {
            self.ui.set_sleep_left("".into());
            return;
        };
        let left = at.saturating_duration_since(Instant::now());
        let label: slint::SharedString = (left.as_secs() / 60 + 1).to_string().into();
        if self.ui.get_sleep_left() != label {
            self.ui.set_sleep_left(label);
        }
        let fade = left.as_secs_f64() / 10.0;
        if fade < 1.0 && self.xf.is_none() {
            if let Some(p) = &self.player {
                p.send(Cmd::Volume(self.vol_at(fade * fade)));
            }
        }
        if left.is_zero() {
            if self.ui.get_playing() {
                self.toggle();
            }
            if let Some(p) = &self.player {
                p.send(Cmd::Volume(self.vol_at(1.0)));
            }
            self.set_sleep(0);
            self.toast(&tr!("Спокойной ночи 🌙", "Good night 🌙", "На добраніч 🌙", "Dobrú noc 🌙"));
        }
    }

    // ── горячие клавиши ──

    fn set_volume(&mut self, v: f32) {
        let v = v.clamp(0.0, 1.0);
        self.store.settings.volume = v;
        self.ui.set_volume(v);
        if let Some(p) = &self.player {
            p.send(Cmd::Volume(v as f64 * 100.0));
        }
        // клавишами громкость крутят пачкой — data.json пишем один раз, когда отпустили
        self.save_timer.start(TimerMode::SingleShot, Duration::from_millis(800), || {
            with(|a| a.save());
        });
    }

    /// Перемешка запоминается в настройках: после перезапуска остаётся, пока её не выключат
    fn set_shuffle(&mut self, on: bool) {
        self.shuffle = on;
        self.ui.set_shuffle(on);
        if self.store.settings.shuffle != on {
            self.store.settings.shuffle = on;
            self.save();
        }
    }

    fn set_repeat(&mut self, mode: u8) {
        if self.conn_cmd("repeat", serde_json::json!(mode)) {
            return;
        }
        self.repeat = mode.min(2);
        self.ui.set_repeat(self.repeat as i32);
    }

    /// выкл → вся очередь → один трек → выкл
    fn cycle_repeat(&mut self) {
        self.set_repeat((self.repeat + 1) % 3);
    }

    fn toggle_shuffle(&mut self) {
        self.set_shuffle(!self.shuffle);
        // очередь сразу перестраивается: перемешанная целиком или снова по порядку
        if let Some(c) = self.current.clone() {
            self.upnext = self.build_upnext(&c);
            self.queue_changed();
        }
    }

    fn binding(&self, act: &str) -> String {
        self.store.settings.keys.get(act).cloned().unwrap_or_else(|| KEYS.iter().find(|(a, _)| *a == act).map_or("", |k| k.1).to_string())
    }

    fn refresh_keys(&self) {
        let names = [
            tr!("Пауза / играть", "Pause / play", "Пауза / грати", "Pauza / hrať"),
            tr!("Назад на 5 с", "Back 5 s", "Назад на 5 с", "Späť o 5 s"),
            tr!("Вперёд на 5 с", "Forward 5 s", "Вперед на 5 с", "Dopredu o 5 s"),
            tr!("Предыдущий трек", "Previous track", "Попередній трек", "Predchádzajúca skladba"),
            tr!("Следующий трек", "Next track", "Наступний трек", "Ďalšia skladba"),
            tr!("Громче", "Volume up", "Гучніше", "Hlasnejšie"),
            tr!("Тише", "Volume down", "Тихіше", "Tichšie"),
            tr!("Без звука", "Mute", "Без звуку", "Stlmiť"),
            tr!("Текст песни", "Lyrics", "Текст пісні", "Text piesne"),
            tr!("Перемешка", "Shuffle", "Перемішування", "Náhodne"),
            tr!("Повтор", "Repeat", "Повтор", "Opakovanie"),
            tr!("Полный экран", "Fullscreen", "Повний екран", "Celá obrazovka"),
            tr!("Поиск", "Search", "Пошук", "Hľadať"),
            tr!("Синхронизировать плейлисты", "Sync playlists", "Синхронізувати плейлисти", "Synchronizovať playlisty"),
        ];
        let space = tr!("Пробел", "Space", "Пробіл", "Medzerník");
        let binds: Vec<KeyBind> = KEYS
            .iter()
            .zip(names)
            .map(|((act, _), name)| {
                let k = self.binding(act);
                KeyBind { name: name.into(), key: if k.is_empty() { "—".into() } else { k.replace("Space", &space).into() } }
            })
            .collect();
        self.ui.set_key_binds(Rc::new(VecModel::from(binds)).into());
    }

    /// Нажатие из окна. Ждём новую клавишу для действия — запоминаем её; иначе ищем, чьё это сочетание.
    /// Возвращает, что сделали: "" — клавиша ничья (пусть обработает поле ввода), "search"/"fullscreen" доделает UI
    fn key_action(&mut self, text: &str, ctrl: bool, alt: bool, shift: bool, meta: bool) -> String {
        let name = key_name(text, ctrl, alt, shift, meta);
        if self.rebinding >= 0 {
            let Some(name) = name else { return "wait".into() }; // нажали только Shift/Ctrl — ждём саму клавишу
            let i = self.rebinding as usize;
            self.rebinding = -1;
            self.ui.set_rebinding(-1);
            if name != "Esc" {
                let act = KEYS[i].0;
                let old = self.binding(act);
                // клавиша была у другого действия — меняем их местами, чтобы одна клавиша не делала два дела
                for (other, _) in KEYS.iter().filter(|(a, _)| *a != act) {
                    if self.binding(other) == name {
                        self.store.settings.keys.insert(other.to_string(), old.clone());
                    }
                }
                self.store.settings.keys.insert(act.to_string(), name);
                self.save();
            }
            self.refresh_keys();
            return "rebound".into();
        }
        let Some(name) = name else { return String::new() };
        let Some(act) = KEYS.iter().map(|(a, _)| *a).find(|a| self.binding(a) == name) else { return String::new() };
        if act != "search" && act != "fullscreen" {
            self.hotkey(act);
        }
        act.into()
    }

    fn hotkey(&mut self, k: &str) {
        let live = self.current.is_some() && !self.restored && self.dur > 0.0;
        match k {
            "toggle" => self.toggle(),
            "next" => self.advance(),
            "prev" => self.prev(),
            "fwd" if live => self.seek_abs((self.pos + 5.0).min(self.dur - 1.0)),
            "back" if live => self.seek_abs((self.pos - 5.0).max(0.0)),
            "vol+" | "vol-" => {
                self.muted = None;
                self.set_volume(self.store.settings.volume + if k == "vol+" { 0.05 } else { -0.05 });
            }
            "mute" => match self.muted.take() {
                Some(v) => self.set_volume(v),
                None => {
                    self.muted = Some(self.store.settings.volume);
                    self.set_volume(0.0);
                }
            },
            "shuffle" => self.toggle_shuffle(),
            "repeat" => self.cycle_repeat(),
            "lyrics" => self.set_lyrics_open(!self.ui.get_lyrics_open()),
            "sync" => self.cloud_sync(true),
            _ => {}
        }
    }

    // ── Discord ──

    fn discord_update(&mut self) {
        let Some(d) = &self.discord else { return };
        let playing = self.ui.get_playing() && !self.ui.get_loading() && self.store.settings.discord;
        let now = self.current.as_ref().filter(|_| playing).map(|t| {
            let speed = self.store.settings.speed.max(0.1) as f64;
            let start_ms = now_ms() - (self.pos / speed * 1000.0) as i64;
            discord::Now {
                title: t.title.clone(),
                artist: t.artist.clone(),
                cover: web_cover(t).unwrap_or_default().to_string(),
                url: t.url.clone(),
                start_ms,
                end_ms: (self.dur > 0.0).then(|| start_ms + (self.dur / speed * 1000.0) as i64),
            }
        });
        // опрос mpv 3 раза в секунду, а Discord — только когда что-то правда поменялось (трек, пауза, перемотка)
        let (title, start) = now.as_ref().map_or((String::new(), 0), |n| (format!("{}|{:?}", n.title, n.end_ms.is_some()), n.start_ms));
        if title == self.discord_last.0 && (start - self.discord_last.1).abs() < 2000 {
            return;
        }
        self.discord_last = (title, start);
        d.set(now);
    }

    // ── тема из обложки ──

    fn apply_theme(&self) {
        let th = self.ui.global::<Theme>();
        match self.cover_pal.filter(|_| self.store.settings.cover_theme) {
            Some(p) => {
                let c = |i: usize| slint::Color::from_rgb_u8(p[i][0], p[i][1], p[i][2]);
                th.set_cov_c1(c(0));
                th.set_cov_c2(c(1));
                th.set_cov_soft1(c(2));
                th.set_cov_pale1(c(3));
                th.set_cov_soft2(c(4));
                th.set_cov_deep1(c(5));
                th.set_cov_deep2(c(6));
                th.set_cov_glow(c(7));
                th.set_id(THEMES); // слот «из обложки» — сразу за обычными темами
            }
            None => th.set_id(self.store.settings.theme.clamp(0, THEMES - 1)),
        }
    }

    // ── локальные файлы ──

    fn music_dirs(&self) -> Vec<std::path::PathBuf> {
        let mut v: Vec<std::path::PathBuf> = self.store.settings.music_dirs.iter().map(Into::into).collect();
        v.push(self.dl_dir());
        v
    }

    /// Сканируем в фоне с nice 19: теги читаются быстро, но на тысячах файлов это секунды CPU
    fn scan_local(&mut self) {
        self.ui.set_local_scanning(true);
        let dirs = self.music_dirs();
        let shown: Vec<slint::SharedString> = self.store.settings.music_dirs.iter().map(|d| d.as_str().into()).collect();
        self.ui.set_music_dirs(Rc::new(VecModel::from(shown)).into());
        std::thread::spawn(move || {
            #[cfg(target_os = "linux")]
            unsafe {
                libc::setpriority(libc::PRIO_PROCESS, 0, 19)
            };
            let tracks = local::scan(&dirs);
            post(move |a| {
                a.local = tracks;
                a.show_local();
                a.ui.set_local_scanning(false);
            });
        });
    }

    fn show_local(&mut self) {
        let items: Vec<TrackItem> = self.local.iter().map(|t| self.item(t)).collect();
        self.local_model.set_vec(items);
    }

    fn add_music_dir(&mut self) {
        let title = tr!("Папка с музыкой", "Music folder", "Тека з музикою", "Priečinok s hudbou");
        let start = dirs::audio_dir().or_else(dirs::home_dir).unwrap_or_default();
        std::thread::spawn(move || {
            let d = rfd::FileDialog::new().set_title(&title).set_directory(&start).pick_folder();
            post(move |a| {
                let Some(d) = d.map(|d| d.display().to_string()) else { return };
                if !a.store.settings.music_dirs.contains(&d) {
                    a.store.settings.music_dirs.push(d);
                    a.save();
                    a.scan_local();
                }
            });
        });
    }

    fn remove_music_dir(&mut self, i: usize) {
        if i < self.store.settings.music_dirs.len() {
            self.store.settings.music_dirs.remove(i);
            self.save();
            self.scan_local();
        }
    }

    // ── lyrics ──

    fn load_lyrics(&mut self, t: &Track) {
        self.lyr_gen += 1;
        let tok = self.lyr_gen;
        self.lyr_for = key(t);
        self.lyr = None;
        self.lyr_model.set_vec(vec![]);
        self.ui.set_lyr_dots_open(-1);
        self.ui.set_lyr_state(0);
        self.ui.set_lyr_cur(-1);
        self.lyr_rank = (0, 0);
        let spicy_key = spicy::key(&self.store.settings.spicy_key);
        self.lyr_pending = if spicy_key.is_some() { 3 } else { 2 };
        self.lyr_results.clear();
        self.lyr_decided = false;
        self.ui.set_lyr_credit(Default::default());
        self.ui.set_lyr_people(Rc::new(VecModel::<Credit>::default()).into());
        // ждём все источники, но Liriqo бывает думает долго — больше 10 с не ждём (поздний ответ см. late_lyrics)
        self.lyr_deadline.start(TimerMode::SingleShot, Duration::from_secs(10), move || {
            with(|a| {
                if a.lyr_gen == tok && !a.lyr_decided {
                    a.decide_lyrics();
                }
            });
        });
        let (artist, title, dur, cover) = (t.artist.clone(), t.title.clone(), t.dur, t.cover.clone());
        // lrclib — быстрый и подогнан по длительности нашего трека; Liriqo медленнее (2–7 с), но даёт
        // тайминги по словам; Spicy Lyrics (если есть ключ) — community-синки и Apple. Все параллельно,
        // текст выбираем один раз, когда ответят все
        let q = match t.src {
            Src::Yt => lyrics::Query::Video(t.id.clone()),
            Src::Sp => lyrics::Query::Url(t.url.clone()),
            Src::Sc | Src::Ya | Src::Lc => lyrics::Query::Meta { artist: artist.clone(), title: title.clone(), dur },
        };
        if let Some(k) = spicy_key {
            let t = t.clone();
            std::thread::spawn(move || {
                let r = spicy::fetch(&t, &k);
                post(move |a| a.lyrics_arrived(tok, "spicy", r));
            });
        }
        std::thread::spawn(move || {
            let lrclib = lyrics::Credit { provider: "LRCLIB".into(), ..Default::default() };
            let r = lyrics::fetch(&artist, &title, dur).map(|l| l.map(|lyr| lyrics::Found { lyr, credit: lrclib }));
            post(move |a| a.lyrics_arrived(tok, "lrclib", r));
        });
        std::thread::spawn(move || {
            let r = lyrics::fetch_liriqo(&q);
            post(move |a| a.lyrics_arrived(tok, "liriqo", r));
        });
        // большая обложка и размытый фон для экрана lyrics
        std::thread::spawn(move || {
            // нет обложки (локальный файл без картинки) — убираем фон и цвета прошлого трека
            let Ok(b) = covers::big(&cover).map_err(|e| eprintln!("big cover: {e:#}")) else {
                post(move |a| {
                    if a.lyr_gen == tok {
                        a.ui.set_has_big(false);
                        a.cover_pal = None;
                        a.apply_theme();
                    }
                });
                return;
            };
            post(move |a| {
                if a.lyr_gen == tok {
                    a.cover_pal = b.palette;
                    a.apply_theme();
                    a.ui.set_big_cover(Image::from_rgba8(b.cover));
                    a.ui.set_bg_img(Image::from_rgba8(b.bg));
                    a.ui.set_blob_img(Image::from_rgba8(b.blob));
                    a.ui.set_has_big(true);
                    if let (Some(m), Some(art), Some(t)) = (&mut a.media, &b.art, &a.current) {
                        m.metadata(&t.title, &t.artist, Some(art), a.dur.max(t.dur as f64));
                    }
                }
            });
        });
    }

    fn lyrics_arrived(&mut self, tok: u64, from: &'static str, r: Result<Option<lyrics::Found>>) {
        if self.lyr_gen != tok {
            return;
        }
        let found = r.unwrap_or_else(|e| {
            eprintln!("lyrics {from}: {e:#}");
            None
        });
        self.lyr_results.extend(found);
        if self.lyr_decided {
            self.late_lyrics();
            return;
        }
        self.lyr_pending = self.lyr_pending.saturating_sub(1);
        if self.lyr_pending == 0 {
            self.decide_lyrics();
        }
    }

    fn decide_lyrics(&mut self) {
        self.lyr_decided = true;
        self.lyr_deadline.stop();
        let chosen = lyrics::choose(&self.lyr_results);
        self.lyr_rank = chosen.as_ref().map_or((0, 0), lyrics::Found::rank);
        self.apply_lyrics(chosen);
    }

    /// Источник ответил уже после дедлайна. Если его текст лучше — подменяем, но только пока это не видно:
    /// экран текста закрыт или строки ещё не начались. Иначе текст бы прыгал посреди песни
    fn late_lyrics(&mut self) {
        let Some(chosen) = lyrics::choose(&self.lyr_results) else { return };
        let first = match &self.lyr {
            Some(d) => d.raw.first().map_or(0.0, |l| l.start),
            None => f64::MAX,
        };
        if chosen.rank() > self.lyr_rank && (!self.ui.get_lyrics_open() || self.pos < first - 1.0) {
            eprintln!("lyrics: {} опоздал, но текст лучше — подменяю", chosen.credit.provider);
            self.lyr_rank = chosen.rank();
            self.apply_lyrics(Some(chosen));
        }
    }

    fn apply_lyrics(&mut self, f: Option<lyrics::Found>) {
        self.show_credit(f.as_ref().map(|f| &f.credit));
        match f.map(|f| f.lyr) {
            Some(lyrics::Lyrics::Synced(lines)) => {
                self.lyr = Some(LyrData { raw: lines, words: vec![], starts: vec![], ends: vec![], fills: vec![], tops: vec![], bg: vec![], dots: vec![], cur: -2, cur_at: Instant::now(), tr: HashMap::new() });
                self.layout_lyrics();
                self.ui.set_lyr_state(1);
                self.lyr_translate(false);
            }
            Some(lyrics::Lyrics::Plain(lines)) => {
                let v: Vec<slint::SharedString> = lines.iter().map(|l| l.as_str().into()).collect();
                self.ui.set_lyr_plain(Rc::new(VecModel::from(v)).into());
                self.ui.set_lyr_state(2);
            }
            None => self.ui.set_lyr_state(3),
        }
        self.update_lyr_timer();
    }

    /// Подпись источника под текстом (Spicy Lyrics требует её всегда, а людей — для community-синков)
    fn show_credit(&self, c: Option<&lyrics::Credit>) {
        let Some(c) = c else {
            self.ui.set_lyr_credit(Default::default());
            self.ui.set_lyr_people(Rc::new(VecModel::<Credit>::default()).into());
            return;
        };
        let text = if c.via.is_empty() { tr!("Текст: {}", "Lyrics: {}", "Текст: {}", "Text: {}", c.provider) } else { tr!("Текст: {} · через {}", "Lyrics: {} via {}", "Текст: {} · через {}", "Text: {} · cez {}", c.provider, c.via) };
        let people: Vec<Credit> = c
            .people
            .iter()
            .map(|(role, name, url)| {
                let role = if *role == "maker" { tr!("синк", "synced by", "синк", "synch.") } else { tr!("загрузил", "uploaded by", "завантажив", "nahral") };
                Credit { label: format!("{role} {name}").into(), url: url.as_str().into() }
            })
            .collect();
        self.ui.set_lyr_credit(text.into());
        self.ui.set_lyr_people(Rc::new(VecModel::from(people)).into());
    }

    /// Перевод текста на язык интерфейса, если он включён: одним запросом в фоне, пришёл — раскладываем
    /// заново (переводы встают мелко под строками). manual — нажали кнопку: тогда сообщаем, если не вышло
    fn lyr_translate(&mut self, manual: bool) {
        let Some(d) = &self.lyr else { return };
        if !self.store.settings.lyr_tr || !d.tr.is_empty() {
            return;
        }
        let mut texts: Vec<String> = d.raw.iter().map(|l| l.text.trim().to_string()).filter(|t| !t.is_empty()).collect();
        texts.dedup();
        if texts.is_empty() {
            return;
        }
        let lang = if self.store.settings.lang.is_empty() { "en".to_string() } else { self.store.settings.lang.clone() };
        let tok = self.lyr_gen;
        std::thread::spawn(move || {
            let r = lyrics::translate(&texts, &lang);
            post(move |a| {
                if a.lyr_gen != tok || !a.store.settings.lyr_tr {
                    return;
                }
                match r {
                    Ok(Some(out)) => {
                        if let Some(d) = &mut a.lyr {
                            d.tr = texts.into_iter().zip(out).filter(|(s, t)| !t.is_empty() && s != t).collect();
                        }
                        a.layout_lyrics();
                    }
                    Ok(None) if manual => a.toast(&tr!("Текст и так на твоём языке", "Lyrics are already in your language", "Текст і так твоєю мовою", "Text už je v tvojom jazyku")),
                    Err(e) if manual => {
                        eprintln!("{e:#}");
                        a.toast(&tr!("Не удалось перевести текст", "Couldn't translate the lyrics", "Не вдалося перекласти текст", "Text sa nepodarilo preložiť"));
                    }
                    _ => {}
                }
            });
        });
    }

    /// Раскладывает строки по рядам под текущую ширину колонки
    fn layout_lyrics(&mut self) {
        let wrap = self.lyr_wrap;
        let show_tr = self.store.settings.lyr_tr;
        let Some(d) = &mut self.lyr else { return };
        let disp = lyrics::display_lines(&d.raw);
        let (mut starts, mut ends, mut fills, mut tops, mut items, mut words) = (vec![], vec![], vec![], vec![], vec![], vec![]);
        let mut top = 0.0;
        for l in &disp {
            if l.bg {
                top -= LYR_GAP - BG_GAP; // прижимаем к основной строке
            }
            let wrap = if l.bg { (wrap as f32 / BG_SCALE) as usize } else { wrap };
            let text = l.text.clone();
            // все доли — по символам исходной строки: и ряды, и слова меряются одной линейкой
            let total = text.chars().count().max(1) as f32;
            let frac = |byte: usize| text[..byte].chars().count() as f32 / total;
            let mut cursor = 0;
            let rows: Vec<LyrRow> = if l.dots {
                vec![LyrRow { a: 0.0, b: 1.0, chars: Rc::new(VecModel::default()).into(), pre: Rc::new(VecModel::default()).into(), text: "".into() }]
            } else {
                lyrics::wrap(&text, wrap)
                    .into_iter()
                    .map(|r| {
                        let at = text[cursor..].find(&r).map_or(cursor, |x| cursor + x);
                        cursor = at + r.len();
                        // буквы по отдельности — для анимаций, где ряд рассыпается
                        let chars: Vec<slint::SharedString> = r.chars().map(|c| c.to_string().into()).collect();
                        // ряд до k-й буквы включительно: по ширине этих кусков буквы встают ровно туда же,
                        // где они в цельном тексте, с кернингом — переход между режимами не виден
                        let pre: Vec<slint::SharedString> = r.char_indices().map(|(i, c)| r[..i + c.len_utf8()].into()).collect();
                        LyrRow { a: frac(at), b: frac(cursor), chars: Rc::new(VecModel::from(chars)).into(), pre: Rc::new(VecModel::from(pre)).into(), text: r.into() }
                    })
                    .collect()
            };
            let mut cursor = 0;
            let ws: Vec<(f64, f64, f32, f32)> = l
                .words
                .iter()
                .filter_map(|w| {
                    let t = w.text.trim();
                    let at = cursor + text[cursor..].find(t)?;
                    cursor = at + t.len();
                    Some((w.start, w.end.max(w.start + 0.01), frac(at), frac(cursor)))
                })
                .collect();
            // перевод — своими рядами мелким шрифтом, переносы под ту же ширину
            let tr_rows: Vec<slint::SharedString> = match d.tr.get(l.text.trim()).filter(|_| show_tr && !l.dots) {
                Some(t) => lyrics::wrap(t, (wrap as f32 / TR_SCALE) as usize).into_iter().map(Into::into).collect(),
                None => vec![],
            };
            let scale = if l.bg { BG_SCALE } else { 1.0 };
            starts.push(l.start);
            ends.push(l.end);
            fills.push(l.fill_end);
            tops.push(top);
            words.push(ws);
            items.push(LyrLine {
                rows: Rc::new(VecModel::from(rows.clone())).into(),
                top,
                empty: false,
                right: l.right,
                dots: l.dots,
                bg: l.bg,
                tr: Rc::new(VecModel::from(tr_rows.clone())).into(),
            });
            // точки проигрыша места не занимают: пока проигрыш идёт, UI раздвигает строки под ними
            // (lyr-dots-open), а в остальное время дырки в тексте нет — как в Apple Music
            if !l.dots {
                top += rows.len() as f32 * LYR_ROW_H * scale + tr_rows.len() as f32 * TR_ROW_H * scale + LYR_GAP;
            }
        }
        d.words = words;
        d.bg = disp.iter().map(|l| l.bg).collect();
        d.dots = disp.iter().map(|l| l.dots).collect();
        (d.starts, d.ends, d.fills, d.tops, d.cur) = (starts, ends, fills, tops, -2);
        self.lyr_model.set_vec(items);
        self.lyr_tick();
    }

    /// Позиция между опросами mpv досчитывается по часам — иначе караоке дёргалось бы 3 раза в секунду.
    /// Активных строк может быть несколько (у Apple бэк-вокал и дуэты идут внахлёст) — у каждой своя заливка
    fn lyr_tick(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.lyr_last).as_secs_f32().min(0.1);
        self.lyr_last = now;
        if let Some(d) = &mut self.lyr {
            let playing = self.ui.get_playing() && !self.ui.get_loading();
            // между опросами mpv время досчитываем по часам с учётом скорости воспроизведения
            let speed = self.store.settings.speed as f64;
            let mpv = self.pos + if playing { (self.pos_at.elapsed().as_secs_f64() * speed).min(1.5) } else { 0.0 };
            // свои часы идут ровно, а к mpv подтягиваются мягко (~1/3 ошибки в секунду): свежая позиция
            // от mpv приходит 3 раза в секунду и чуть отличается от досчитанной — раньше на каждом
            // опросе заливка и волна дёргались. Назад не ходим; перемотку (ошибка > 1 с) берём сразу
            let ahead = if playing { self.lyr_shown + dt as f64 * speed } else { self.lyr_shown };
            let err = mpv - ahead;
            let pos = if err.abs() > 1.0 { mpv } else { (ahead + err * (dt as f64 * 3.0).min(1.0)).max(self.lyr_shown) };
            self.lyr_shown = pos;
            let (mut cur, act) = lyrics::active(&d.starts, &d.ends, pos);
            // две строки поются внахлёст (дуэт) — прокрутка сразу к последней начавшейся, а не к первой
            // и потом второй рывок к следующей
            if let Some(&i) = act.iter().rev().find(|&&i| !d.bg[i]) {
                cur = i as i32;
            }
            // прокрутка держится за основную строку, а не за её бэк-вокал
            while cur > 0 && d.bg[cur as usize] {
                cur -= 1;
            }
            // текст прыгнул (перемотка, текст пришёл посреди песни): пропущенные строки разом стали бы
            // спетыми и рассыпались на буквы все сразу — сотни букв и лаги. При прыжке все строки до
            // нового места просто гаснут, без эффектов
            if cur != d.cur && (err.abs() > 1.0 || d.cur == -2) {
                self.ui.set_lyr_jump(cur);
            }
            if cur != d.cur {
                d.cur = cur;
                d.cur_at = now;
                self.ui.set_lyr_cur(cur);
                // до первой строки держим текст чуть ниже якоря
                self.ui.set_lyr_cur_top(if cur >= 0 { d.tops[cur as usize] } else { -LYR_ROW_H * 1.5 });
            }
            // буквы следующей строки создаются и размечаются заранее, когда прокрутка уже доехала (750 мс):
            // в кадр смены строки это стоило в 2–3 раза дороже обычного — как раз на старте прокрутки
            let prep = if now.duration_since(d.cur_at) > Duration::from_millis(800) { cur + 1 } else { -1 };
            if self.ui.get_lyr_prep() != prep {
                self.ui.set_lyr_prep(prep);
            }
            let progress = |i: usize| -> f32 {
                let pos = pos + self.lyr_step; // UI доезжает до этого значения как раз за step
                if let Some(ws) = d.words.get(i).filter(|w| !w.is_empty()) {
                    // реальные тайминги: слово заливается, пока его поют; в паузе между словами стоим
                    match ws.iter().rposition(|w| w.0 <= pos) {
                        None => 0.0,
                        Some(k) => {
                            let (s, e, c0, c1) = ws[k];
                            c0 + (c1 - c0) * ((pos - s) / (e - s)).clamp(0.0, 1.0) as f32
                        }
                    }
                } else {
                    ((pos - d.starts[i]) / (d.fills[i] - d.starts[i]).max(0.01)).clamp(0.0, 1.0) as f32
                }
            };
            // короткая пауза между строками: последняя спетая остаётся подсвеченной
            let slots: Vec<(i32, f32)> = match (act.is_empty(), cur >= 0) {
                (true, true) => vec![(cur, 1.0)],
                (true, false) => vec![],
                _ => act.iter().take(3).map(|&i| (i as i32, progress(i))).collect(),
            };
            let dots_open = act.iter().find(|&&i| d.dots[i]).map_or(-1, |&i| i as i32);
            if self.ui.get_lyr_dots_open() != dots_open {
                self.ui.set_lyr_dots_open(dots_open);
            }
            // бэк-вокал идёт сразу за своей основной строкой; закончился раньше неё — считаем спетым
            let bg_done = (cur.max(0) as usize + 1..d.bg.len())
                .take_while(|&i| d.bg[i])
                .filter(|&i| d.ends[i] <= pos && !act.contains(&i))
                .last()
                .map_or(-1, |i| i as i32);
            if self.ui.get_lyr_bg_done() != bg_done {
                self.ui.set_lyr_bg_done(bg_done);
            }
            let slot = |k: usize| slots.get(k).copied().unwrap_or((-1, 0.0));
            let ((a0, p0), (a1, p1), (a2, p2)) = (slot(0), slot(1), slot(2));
            self.ui.set_act0(a0);
            self.ui.set_act1(a1);
            self.ui.set_act2(a2);
            self.ui.set_p0(p0);
            self.ui.set_p1(p1);
            self.ui.set_p2(p2);
            // как долго заливка каждой звучащей строки не двигается
            let still: Vec<f32> = [(a0, p0), (a1, p1), (a2, p2)]
                .iter()
                .zip(&mut self.lyr_still)
                .map(|(&(a, p), st)| {
                    if a != st.0 || p > st.1 + 1e-4 {
                        *st = (a, p, now);
                    }
                    now.duration_since(st.2).as_secs_f32()
                })
                .collect();
            for (i, v) in still.into_iter().enumerate() {
                self.lyr_still_model.set_row_data(i, v);
            }
            self.ui.set_act_max(slots.last().map_or(cur, |s| s.0));
        }
        if !self.store.settings.lite && self.ui.get_playing() {
            self.ui.set_bg_t(self.ui.get_bg_t() + dt * 1.5);
        }
    }

    /// Тикаем только пока экран lyrics открыт и что-то играет: 20 fps, в лайт-режиме ~12
    fn update_lyr_timer(&mut self) {
        let on = self.ui.get_lyrics_open() && self.ui.get_playing();
        if on && !self.lyr_timer.running() {
            let ms = if self.store.settings.lite { 80 } else { 50 };
            // UI интерполирует до следующего значения чуть дольше периода, чтобы опоздавший тик не давал остановку
            self.lyr_step = (ms + 16) as f64 / 1000.0;
            self.ui.set_lyr_step((ms + 16) as i64);
            self.lyr_last = Instant::now();
            self.lyr_timer.start(TimerMode::Repeated, Duration::from_millis(ms), || {
                with(|a| a.lyr_tick());
            });
        } else if !on {
            self.lyr_timer.stop();
        }
    }

    /// Мышь шевельнулась: показываем управление и заново ждём 2 с тишины
    fn lyr_activity(&mut self) {
        self.ui.set_lyr_idle(false);
        if !self.ui.get_lyrics_open() {
            return self.lyr_idle.stop();
        }
        self.lyr_idle.start(TimerMode::SingleShot, Duration::from_secs(2), || {
            with(|a| a.ui.set_lyr_idle(a.ui.get_lyrics_open()));
        });
    }

    /// Настройки → интерфейс, mpv и yt-dlp. При запуске и после сброса настроек
    fn apply_settings(&mut self) {
        let st = self.store.settings.clone();
        let ui = &self.ui;
        ui.set_volume(st.volume);
        if let Some(p) = &self.player {
            p.send(Cmd::Volume(st.volume as f64 * 100.0));
        }
        ui.set_karaoke(st.karaoke);
        ui.set_lyr_style(st.lyr_anim.clamp(0, 2));
        // 1 был "распад" — его больше нет
        ui.set_lyr_exit(match st.lyr_exit {
            e @ (0 | 2..=7) => e,
            _ => 3,
        });
        ui.set_fall_stay(st.fall_stay.clamp(1.0, 10.0));
        ui.set_lyr_glow(st.glow.clamp(0.0, 2.0));
        ui.set_lyr_tr(st.lyr_tr);
        ui.set_lyr_edge(st.lyr_edge.clamp(0.0, 160.0));
        ui.set_shuffle(st.shuffle);
        self.shuffle = st.shuffle;
        ui.set_lite(st.lite);
        ui.set_hole_always(st.hole_always);
        ui.set_spicy_key(st.spicy_key.clone().into());
        ui.set_cookies(st.cookies.clone().into());
        util::set_cookies_browser(&st.cookies);
        ui.set_xfade(st.xfade.clamp(0.0, 12.0));
        ui.set_autoplay(st.autoplay);
        ui.set_cover_theme(st.cover_theme);
        ui.set_discord_on(st.discord);
        ui.set_discord_ready(!discord::APP_ID.is_empty());
        ui.set_offline(st.offline);
        ui.set_cache_gb(st.cache_gb);
        ui.set_setup_open(!st.setup_done);
        // язык: из настроек, а при первом запуске — английский (выбирается в первом же окне настройки)
        let lang = if st.lang.is_empty() { "en".to_string() } else { st.lang.clone() };
        self.set_lang(&lang);
        self.update_lyr_timer();
        self.apply_fx();
        if st.discord && self.discord.is_none() {
            self.discord = discord::Discord::spawn();
        }
        self.discord_last = (String::from("?"), 0); // настройка могла поменяться — отправим заново
        self.discord_update();
        self.apply_theme();
    }

    fn set_lyrics_open(&mut self, open: bool) {
        let w = self.ui.window();
        if open {
            self.was_fullscreen = w.is_fullscreen();
            w.set_fullscreen(true);
        } else {
            w.set_fullscreen(self.was_fullscreen);
        }
        self.ui.set_lyrics_open(open);
        self.lyr_activity();
        self.lyr_tick();
        self.update_lyr_timer();
    }
}

#[cfg(windows)]
fn win_hwnd(ui: &AppWindow) -> Option<*mut std::ffi::c_void> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match ui.window().window_handle().window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(h) => Some(h.hwnd.get() as *mut std::ffi::c_void),
        _ => None,
    }
}

/// На винде консоли нет: паника молча закрывала окно. Пишем её в crash.log рядом с data.json
fn crash_log() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let dir = Store::dir();
        let _ = std::fs::create_dir_all(&dir);
        let msg = format!("{info}\n{}\n\n", std::backtrace::Backtrace::force_capture());
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("crash.log")) {
            use std::io::Write;
            let _ = f.write_all(msg.as_bytes());
        }
        prev(info);
    }));
}

fn main() -> Result<(), slint::PlatformError> {
    crash_log();
    // второй экземпляр держал бы в памяти старый список и при закрытии затёр бы им свежий data.json
    // Skia: текст со сглаживанием и дробными позициями — буквы в lyrics двигаются и растут плавно.
    // BLACKHOLE_RENDER=femtovg — старый лёгкий рендер (для сравнения)
    let render = std::env::var("BLACKHOLE_RENDER").unwrap_or_else(|_| "skia".into());
    if let Err(e) = slint::BackendSelector::new().renderer_name(render.clone()).select() {
        eprintln!("рендер {render} не завёлся ({e}), беру по умолчанию");
    }
    let Ok(_lock) = single_instance() else {
        eprintln!("Gargantua уже запущен");
        return Ok(());
    };
    let ui = AppWindow::new()?;
    let mut store = Store::load();
    if cloud::cfg().is_some() {
        // плейлисты теперь в аккаунте: старые с диска откладываем до первого входа и в data.json больше не пишем
        model::CLOUD.store(true, std::sync::atomic::Ordering::Relaxed);
        cloud::stash_local(&store.playlists);
        store.playlists.clear();
    }

    let player = Player::spawn(store.settings.volume as f64 * 100.0, |ev| post(move |a| a.on_ev(ev)))
        .map_err(|e| eprintln!("{e:#}"))
        .ok();

    let res_model = Rc::new(VecModel::default());
    let pl_model = Rc::new(VecModel::default());
    let pls_model = Rc::new(VecModel::default());
    let lyr_model = Rc::new(VecModel::default());
    let hist_model = Rc::new(VecModel::default());
    let eq_model: Rc<VecModel<f32>> = Rc::new(VecModel::default());
    ui.set_eq_gains(eq_model.clone().into());
    let art_top_model = Rc::new(VecModel::default());
    let art_albums_model = Rc::new(VecModel::default());
    let art_singles_model = Rc::new(VecModel::default());
    let art_similar_model = Rc::new(VecModel::default());
    let coll_model = Rc::new(VecModel::default());
    let local_model = Rc::new(VecModel::default());
    ui.set_local_tracks(local_model.clone().into());
    let (home_mixes_model, home_recent_model, home_pls_model, home_similar_model) =
        (Rc::new(VecModel::default()), Rc::new(VecModel::default()), Rc::new(VecModel::default()), Rc::new(VecModel::default()));
    let home_quick_model = Rc::new(VecModel::default());
    ui.set_home_mixes(home_mixes_model.clone().into());
    ui.set_home_recent(home_recent_model.clone().into());
    ui.set_home_pls(home_pls_model.clone().into());
    ui.set_home_similar(home_similar_model.clone().into());
    ui.set_home_quick(home_quick_model.clone().into());
    ui.set_art_top(art_top_model.clone().into());
    ui.set_art_albums(art_albums_model.clone().into());
    ui.set_art_singles(art_singles_model.clone().into());
    ui.set_art_similar(art_similar_model.clone().into());
    let s_artists_model = Rc::new(VecModel::default());
    let s_albums_model = Rc::new(VecModel::default());
    ui.set_s_artists(s_artists_model.clone().into());
    ui.set_s_albums(s_albums_model.clone().into());
    ui.set_coll_tracks(coll_model.clone().into());
    let next_model = Rc::new(VecModel::default());
    ui.set_q_hist(hist_model.clone().into());
    ui.set_q_next(next_model.clone().into());
    ui.set_lyr_lines(lyr_model.clone().into());
    let lyr_still_model = Rc::new(VecModel::from(vec![0.0f32; 3]));
    ui.set_still(lyr_still_model.clone().into());
    ui.set_results(res_model.clone().into());
    ui.set_pl_tracks(pl_model.clone().into());
    ui.set_playlists(pls_model.clone().into());
    ui.set_version(env!("CARGO_PKG_VERSION").into());

    let app = App {
        ui: ui.clone_strong(),
        store,
        results: Default::default(),
        view: vec![],
        res_model,
        pl_model,
        pls_model,
        open_pl: None,
        source: vec![],
        played: vec![],
        upnext: vec![],
        hist_model,
        next_model,
        current: None,
        gen: 0,
        searches_left: 0,
        covers: HashMap::new(),
        pending: HashSet::new(),
        loader: covers::Loader::new(|url, buf| post(move |a| a.on_cover(url, buf))),
        player,
        token: Arc::new(AtomicU64::new(0)),
        pos: 0.0,
        pos_at: Instant::now(),
        dur: 0.0,
        lyr: None,
        lyr_model,
        lyr_timer: Timer::default(),
        lyr_idle: Timer::default(),
        lyr_last: Instant::now(),
        lyr_shown: 0.0,
        lyr_step: 0.066,
        lyr_still: [(-1, 0.0, Instant::now()); 3],
        lyr_still_model: lyr_still_model.clone(),
        lyr_wrap: 30,
        lyr_rank: (0, 0),
        lyr_pending: 0,
        lyr_gen: 0,
        lyr_for: String::new(),
        lyr_results: vec![],
        lyr_decided: false,
        lyr_deadline: Timer::default(),
        was_fullscreen: false,
        shuffle: false,
        repeat: 0,
        stream_cache: HashMap::new(),
        inflight: HashSet::new(),
        prefetched: false,
        from_cache: false,
        retries: 0,
        fail_streak: 0,
        qr_imgs: vec![],
        artist: None,
        coll: Coll::default(),
        back: vec![],
        art_gen: 0,
        art_top_model,
        art_albums_model,
        art_singles_model,
        art_similar_model,
        s_artists: vec![],
        s_albums: vec![],
        s_artists_model,
        s_albums_model,
        typing: Timer::default(),
        last_q: String::new(),
        coll_model,
        qr_page: 0,
        media: None,
        eq_model: eq_model.clone(),
        eq_anim: Timer::default(),
        load_started: Instant::now(),
        seek_to: None,
        add_target: None,
        toast_timer: Timer::default(),
        dl_queue: Default::default(),
        dl_busy: HashSet::new(),
        dl_active: 0,
        dl_files: HashSet::new(),
        intro: Timer::default(),
        review: None,
        confirm: None,
        restored: false,
        resume_at: None,
        player2: None,
        xf: None,
        xf_timer: Timer::default(),
        stats: stats::Stats::load(),
        stats_others: stats::Stats::default(),
        listen_at: None,
        listened: 0.0,
        stats_saved: Instant::now(),
        stat_top: vec![],
        stat_artists: vec![],
        sleep_mode: 0,
        sleep_at: None,
        sleep_timer: Timer::default(),
        discord: None,
        discord_last: (String::new(), 0),
        local: vec![],
        local_model: local_model.clone(),
        auto_for: String::new(),
        cover_pal: None,
        muted: None,
        rebinding: -1,
        pl_view: vec![],
        pl_query: String::new(),
        pl_sort: 0,
        home: home::load(),
        home_busy: false,
        home_quick: vec![],
        home_recent: vec![],
        home_pls: vec![],
        home_mixes_model: home_mixes_model.clone(),
        home_recent_model: home_recent_model.clone(),
        home_pls_model: home_pls_model.clone(),
        home_similar_model: home_similar_model.clone(),
        home_quick_model: home_quick_model.clone(),
        save_timer: Timer::default(),
        cloud_timer: Timer::default(),
        push_timer: Timer::default(),
        syncing: false,
        cloud_loaded: false,
        conn_live: false,
        conn_timer: Timer::default(),
        conn_pub: Default::default(),
        conn_adopted: String::new(),
        mirror: None,
        conn_held: None,
    };
    APP.with(|a| *a.borrow_mut() = Some(app));
    with(|a| a.apply_settings());
    // MPRIS: панель, playerctl и медиа-клавиши видят и управляют плеером
    #[cfg(windows)]
    let hwnd = {
        ui.show()?;
        win_hwnd(&ui)
    };
    #[cfg(not(windows))]
    let hwnd = None;
    let media = media::Media::new(hwnd, |act| {
        post(move |a| match act {
            media::Action::Play if !a.ui.get_playing() => a.toggle(),
            media::Action::Pause if a.ui.get_playing() => a.toggle(),
            media::Action::Toggle => a.toggle(),
            media::Action::Next => a.advance(),
            media::Action::Prev => a.prev(),
            media::Action::Stop => a.stop(),
            media::Action::SeekTo(s) => a.seek_abs(s),
            media::Action::SeekBy(d) => a.seek_abs((a.pos + d).max(0.0)),
            _ => {}
        })
    });
    with(|a| a.media = media);
    // эквалайзер и скорость из настроек — сразу в mpv (команды ждут в очереди, пока он поднимется)
    ui.set_eq_names(Rc::new(VecModel::from(fx::BANDS.iter().map(|f| if *f >= 1000 { format!("{}k", f / 1000) } else { f.to_string() }.into()).collect::<Vec<slint::SharedString>>())).into());
    with(|a| a.refresh_playlists());

    ui.on_search(|q| {
        with(|a| a.search(q.to_string(), true));
    });
    ui.on_search_typed(|q| {
        with(|a| a.search_typed(q.to_string()));
    });
    ui.on_set_filter(|f| {
        with(|a| {
            a.ui.set_filter(f);
            a.rebuild_view();
        });
    });
    ui.on_play_result(|i| {
        with(|a| a.play_queue(a.view.clone(), i as usize));
    });
    ui.on_play_pl(|i| {
        with(|a| a.play_pl(i as usize));
    });
    ui.on_pl_filter(|q, sort| {
        with(|a| {
            a.pl_query = q.to_string();
            a.pl_sort = sort;
            a.ui.set_pl_sort(sort);
            a.pl_rebuild();
            a.mark_playing();
        });
    });
    ui.on_play_all(|shuffle| {
        with(|a| {
            let Some(p) = a.open_pl else { return };
            let q = a.store.playlists[p].tracks.clone();
            if shuffle {
                a.set_shuffle(true);
            }
            let i = if shuffle { rand_below(q.len()) } else { 0 };
            a.play_queue(q, i);
        });
    });
    ui.on_download(|i, kind| {
        with(|a| {
            if let Some(t) = a.track_at(kind, i as usize) {
                a.ask(Confirm::Download(vec![t]));
            }
        });
    });
    ui.on_download_all(|kind| {
        with(|a| {
            let all = match kind {
                1 => a.open_pl.and_then(|p| a.store.playlists.get(p)).map(|p| p.tracks.clone()).unwrap_or_default(),
                _ => a.coll.tracks.clone(),
            };
            a.ask(Confirm::Download(all));
        });
    });
    ui.on_pick_dl_dir(|| {
        with(|a| a.pick_dl_dir());
    });
    ui.on_open_dl_dir(|| {
        with(|a| {
            let d = a.dl_dir();
            let _ = std::fs::create_dir_all(&d);
            open_url(&d.display().to_string());
        });
    });
    with(|a| a.scan_dl_dir());
    with(|a| a.scan_local());
    // живой yt-dlp поднимаем сразу: первый трек не будет ждать запуска Python и загрузки кук
    util::warm_ytdlp();
    with(|a| {
        a.refresh_home();
        a.build_home(false);
        a.show_cache_size(cache::size());
    });
    ui.on_ask_add(|i, kind| {
        with(|a| a.ask_add(i as usize, kind));
    });
    ui.on_add_to(|i| {
        with(|a| a.add_to(i as usize));
    });
    ui.on_create_playlist(|n| {
        with(|a| a.create_playlist(&n));
    });
    ui.on_open_playlist(|i| {
        with(|a| a.open_playlist(i as usize));
    });
    ui.on_edit_playlist(|| {
        with(|a| a.edit_playlist());
    });
    ui.on_save_playlist(|n, d| {
        with(|a| a.save_playlist(&n, &d));
    });
    ui.on_pick_pl_cover(|| {
        with(|a| a.pick_pl_cover());
    });
    ui.on_toggle_like(|| {
        with(|a| a.toggle_like());
    });
    ui.on_like_track(|i, kind| {
        with(|a| a.like_at(i as usize, kind));
    });
    ui.on_like_all_pl(|| {
        with(|a| a.like_all());
    });
    ui.on_delete_playlist(|| {
        with(|a| a.delete_playlist());
    });
    ui.on_remove_track(|i| {
        with(|a| {
            if let Some(&j) = a.pl_view.get(i as usize) {
                a.ask(Confirm::Remove(j));
            }
        });
    });
    ui.on_confirm_yes(|| {
        with(|a| a.confirmed());
    });
    ui.on_review(|k| {
        with(|a| a.review_answer(k));
    });
    ui.on_toggle_play(|| {
        with(|a| a.toggle());
    });
    ui.on_next(|| {
        with(|a| a.advance());
    });
    ui.on_prev(|| {
        with(|a| a.prev());
    });
    ui.on_seek(|f| {
        with(|a| {
            if a.dur > 0.0 {
                a.ui.set_position(f);
                a.seek_abs(f as f64 * a.dur);
            }
        });
    });
    ui.on_set_volume(|v| {
        with(|a| {
            if let Some(p) = &a.player {
                p.send(Cmd::Volume(v as f64 * 100.0));
            }
            a.store.settings.volume = v;
            a.save();
        });
    });
    ui.on_toggle_shuffle(|| {
        with(|a| a.toggle_shuffle());
    });
    ui.on_queue_jump(|i| {
        with(|a| a.queue_jump(i as usize));
    });
    ui.on_history_jump(|i| {
        with(|a| a.history_jump(i as usize));
    });
    ui.on_queue_move(|i, d| {
        with(|a| a.queue_move(i as usize, d));
    });
    ui.on_queue_remove(|i| {
        with(|a| a.queue_remove(i as usize));
    });
    ui.on_toggle_repeat(|| {
        with(|a| a.cycle_repeat());
    });
    ui.on_conn_pick(|id| {
        with(|a| a.conn_pick(id.to_string()));
    });
    ui.on_do_import(|u| {
        with(|a| a.import(u.to_string()));
    });
    ui.on_set_lyrics_open(|open| {
        with(|a| a.set_lyrics_open(open));
    });
    ui.on_lyr_activity(|| {
        with(|a| a.lyr_activity());
    });
    ui.on_lyr_width(|w| {
        with(|a| {
            // ~21px на символ при 36px жирном шрифте
            let n = ((w / 21.0) as usize).clamp(12, 60);
            if n != a.lyr_wrap {
                a.lyr_wrap = n;
                a.layout_lyrics();
            }
        });
    });
    ui.on_lyr_seek(|i| {
        with(|a| {
            if let Some(t) = a.lyr.as_ref().and_then(|d| d.starts.get(i as usize).copied()) {
                a.seek_abs(t);
            }
        });
    });
    ui.on_set_lyr_edge(|v| {
        with(|a| {
            a.store.settings.lyr_edge = v;
            a.ui.set_lyr_edge(v);
            a.save();
        });
    });
    ui.on_toggle_lyr_tr(|| {
        with(|a| {
            let on = !a.store.settings.lyr_tr;
            a.store.settings.lyr_tr = on;
            a.ui.set_lyr_tr(on);
            a.save();
            if on {
                a.lyr_translate(true);
            }
            a.layout_lyrics();
        });
    });
    ui.on_set_karaoke(|on| {
        with(|a| {
            a.store.settings.karaoke = on;
            a.ui.set_karaoke(on);
            a.save();
        });
    });
    ui.on_set_lyr_style(|st| {
        with(|a| {
            a.store.settings.lyr_anim = st;
            a.ui.set_lyr_style(st);
            a.save();
        });
    });
    ui.on_set_lyr_glow(|v| {
        with(|a| {
            a.store.settings.glow = v;
            a.ui.set_lyr_glow(v);
            a.save();
        });
    });
    ui.on_set_fall_stay(|v| {
        with(|a| {
            a.store.settings.fall_stay = v;
            a.ui.set_fall_stay(v);
            a.save();
        });
    });
    ui.on_set_lyr_exit(|e| {
        with(|a| {
            a.store.settings.lyr_exit = e;
            a.ui.set_lyr_exit(e);
            a.save();
        });
    });
    ui.on_toggle_fullscreen(|| {
        with(|a| {
            let w = a.ui.window();
            w.set_fullscreen(!w.is_fullscreen());
        });
    });
    ui.on_set_lite(|on| {
        with(|a| {
            a.store.settings.lite = on;
            a.ui.set_lite(on);
            a.save();
            a.update_lyr_timer();
        });
    });
    ui.on_set_lang(|l| {
        with(|a| {
            a.set_lang(&l);
            a.store.settings.lang = l.to_string();
            a.save();
        });
    });
    ui.on_set_theme(|t| {
        with(|a| {
            // выбрал тему руками — цвет из обложки выключается
            a.store.settings.theme = t;
            a.store.settings.cover_theme = false;
            a.ui.set_cover_theme(false);
            a.apply_theme();
            a.save();
        });
    });
    ui.on_set_cover_theme(|on| {
        with(|a| {
            a.store.settings.cover_theme = on;
            a.ui.set_cover_theme(on);
            a.apply_theme();
            a.save();
            if on && a.cover_pal.is_none() {
                a.toast(&tr!("Цвета подстроятся под обложку следующего трека", "Colours will follow the next track's cover", "Кольори підлаштуються під обкладинку наступного треку", "Farby sa prispôsobia obalu ďalšej skladby"));
            }
        });
    });
    ui.on_set_xfade(|v| {
        with(|a| {
            a.store.settings.xfade = v.round().clamp(0.0, 12.0);
            a.ui.set_xfade(a.store.settings.xfade);
            a.save();
        });
    });
    ui.on_set_autoplay(|on| {
        with(|a| {
            a.store.settings.autoplay = on;
            a.ui.set_autoplay(on);
            a.save();
        });
    });
    ui.on_set_discord(|on| {
        with(|a| {
            a.store.settings.discord = on;
            a.ui.set_discord_on(on);
            if on && a.discord.is_none() {
                a.discord = discord::Discord::spawn();
            }
            a.discord_last = (String::from("?"), 0);
            a.discord_update();
            a.save();
        });
    });
    ui.on_set_sleep(|m| {
        with(|a| {
            a.set_sleep(m);
            let msg = match m {
                1..=3 => tr!("Усну через {} мин", "Sleeping in {} min", "Засну за {} хв", "Zaspím o {} min", [0, 15, 30, 60][m as usize]),
                4 => tr!("Усну после этого трека", "Sleeping after this track", "Засну після цього треку", "Zaspím po tejto skladbe"),
                _ => tr!("Таймер сна выключен", "Sleep timer off", "Таймер сну вимкнено", "Časovač spánku vypnutý"),
            };
            a.toast(&msg);
        });
    });
    ui.on_key_action(|t, c, al, sh, m| with(|a| a.key_action(&t, c, al, sh, m)).unwrap_or_default().into());
    ui.on_home_refresh(|| {
        with(|a| a.build_home(true));
    });
    ui.on_open_home(|| {
        with(|a| {
            a.ui.set_page("search".into());
            a.refresh_home();
            a.build_home(false);
        });
    });
    ui.on_set_offline(|on| {
        with(|a| {
            a.store.settings.offline = on;
            a.ui.set_offline(on);
            a.save();
        });
    });
    ui.on_set_cache_gb(|g| {
        with(|a| {
            a.store.settings.cache_gb = g;
            a.ui.set_cache_gb(g);
            a.save();
        });
    });
    ui.on_clear_cache(|| {
        with(|a| {
            cache::clear();
            a.show_cache_size(0);
            a.toast(&tr!("Офлайн-кэш очищен", "Offline cache cleared", "Офлайн-кеш очищено", "Offline cache vymazaná"));
        });
    });
    ui.on_start_rebind(|i| {
        with(|a| {
            a.rebinding = i;
            a.ui.set_rebinding(i);
        });
    });
    ui.on_reset_keys(|| {
        with(|a| {
            a.store.settings.keys.clear();
            a.save();
            a.refresh_keys();
        });
    });
    ui.on_open_stats(|| {
        with(|a| a.open_stats());
    });
    ui.on_stat_artist(|i| {
        with(|a| {
            if let Some(n) = a.stat_artists.get(i as usize).cloned() {
                a.open_artist_of(Track { artist: n, ..Default::default() });
            }
        });
    });
    ui.on_add_music_dir(|| {
        with(|a| a.add_music_dir());
    });
    ui.on_remove_music_dir(|i| {
        with(|a| a.remove_music_dir(i as usize));
    });
    ui.on_rescan_local(|| {
        with(|a| a.scan_local());
    });
    ui.on_local_play(|shuffle| {
        with(|a| {
            let t = a.local.clone();
            if t.is_empty() {
                return;
            }
            if shuffle {
                a.set_shuffle(true);
            }
            let i = if shuffle { rand_below(t.len()) } else { 0 };
            a.play_queue(t, i);
        });
    });
    ui.on_setup_done(|| {
        with(|a| {
            a.store.settings.setup_done = true;
            a.ui.set_setup_open(false);
            a.save();
        });
    });
    // временная кнопка для тестов первичной настройки
    ui.on_reset_settings(|| {
        with(|a| {
            a.store.settings = model::Settings::default();
            a.save();
            a.apply_settings();
            a.ui.set_setup_open(false); // настройка покажется при следующем запуске, а не прямо сейчас
            a.toast(&tr!(
                "Настройки сброшены — при следующем запуске будет быстрая настройка",
                "Settings reset — quick setup will run on the next launch",
                "Налаштування скинуто — під час наступного запуску буде швидке налаштування", "Nastavenia zresetované — pri ďalšom spustení bude rýchle nastavenie"
            ));
        });
    });
    ui.on_preview_volume(|v| {
        with(|a| {
            if let Some(p) = &a.player {
                p.send(Cmd::Volume(v as f64 * 100.0));
            }
        });
    });
    ui.on_set_hole_always(|on| {
        with(|a| {
            a.store.settings.hole_always = on;
            a.ui.set_hole_always(on);
            a.save();
        });
    });
    ui.on_open_url(|u| open_url(&u));
    // вход YouTube на телефон: куки браузера → в аккаунт, телефон заберёт сам
    ui.on_send_cookies(|| {
        std::thread::spawn(|| {
            let r = util::yt_cookies().and_then(|ck| cloud::put_yt_cookies(&cloud::cfg().context("облако не настроено")?, &ck));
            post(move |a| match r {
                Ok(()) => a.toast(&tr!(
                    "Вход YouTube отправлен. На телефоне: Аккаунт → Настройки → YouTube → «Взять с компа»",
                    "YouTube sign-in sent. On the phone: Account → Settings → YouTube → “Take from PC”",
                    "Вхід YouTube надіслано. На телефоні: Акаунт → Налаштування → YouTube → «Взяти з компа»", "Prihlásenie do YouTube odoslané. Na telefóne: Účet → Nastavenia → YouTube → „Zobrať z PC“"
                )),
                Err(e) => a.toast(&tr!("Не вышло: {:#}", "Failed: {:#}", "Не вийшло: {:#}", "Nevyšlo to: {:#}", e)),
            });
        });
    });
    ui.on_set_cookies(|b| {
        with(|a| {
            a.store.settings.cookies = b.to_string();
            util::set_cookies_browser(&b);
            a.ui.set_cookies(b);
            a.stream_cache.clear(); // ссылки, полученные без кук, могли быть урезанными
            a.save();
        });
    });
    ui.on_acc_login(|e, p| {
        let (e, p) = (e.trim().to_string(), p.to_string());
        with(|a| {
            a.acc_job(move |c, l| {
                Ok(if cloud::sign_in(c, l, &e, &p)? {
                    String::new()
                } else {
                    tr!("Почта не подтверждена: открой ссылку из письма или отправь его ещё раз", "Email isn't confirmed: open the link from the email or send it again", "Пошту не підтверджено: відкрий посилання з листа або надішли його ще раз", "E-mail nie je potvrdený: otvor odkaz z e-mailu alebo ho pošli znova")
                })
            })
        });
    });
    ui.on_acc_register(|e, p| {
        let (e, p) = (e.trim().to_string(), p.to_string());
        with(|a| {
            a.acc_job(move |c, l| {
                cloud::sign_up(c, l, &e, &p)?;
                Ok(tr!("Письмо ушло на {}. Открой ссылку в нём и вернись сюда", "Sent an email to {}. Open the link in it and come back", "Лист пішов на {}. Відкрий посилання в ньому і повертайся", "E-mail odišiel na {}. Otvor odkaz v ňom a vráť sa sem", e))
            })
        });
    });
    ui.on_acc_reset(|e| {
        let e = e.trim().to_string();
        if cloud::account().verified {
            // смена пароля со страницы аккаунта: ответ тостом, строку «синхронизировано» не трогаем
            let (Some(c), lang) = (cloud::cfg(), with(|a| a.store.settings.lang.clone()).unwrap_or_default()) else { return };
            std::thread::spawn(move || {
                let r = cloud::send_reset(&c, if lang.is_empty() { "en" } else { &lang }, &e);
                post(move |a| match r {
                    Ok(()) => a.toast(&tr!("Письмо для смены пароля ушло на {}", "Sent a password reset email to {}", "Лист для зміни пароля пішов на {}", "E-mail na zmenu hesla odišiel na {}", e)),
                    Err(err) => a.toast(&acc_err(&err)),
                });
            });
            return;
        }
        with(|a| {
            a.acc_job(move |c, l| {
                cloud::send_reset(c, l, &e)?;
                Ok(tr!("Письмо для смены пароля ушло на {}", "Sent a password reset email to {}", "Лист для зміни пароля пішов на {}", "E-mail na zmenu hesla odišiel na {}", e))
            })
        });
    });
    ui.on_acc_resend(|| {
        with(|a| {
            a.acc_job(|c, l| {
                cloud::send_verify(c, l)?;
                Ok(tr!("Отправил ещё раз, глянь и в спаме", "Sent again, check spam too", "Надіслав ще раз, глянь і в спамі", "Poslal som znova, pozri aj do spamu"))
            })
        });
    });
    ui.on_acc_check(|| {
        with(|a| a.cloud_sync(true));
    });
    ui.on_acc_logout(|| {
        // неотправленные правки — на сервер, пока ещё есть сессия
        with(|a| {
            if let (Some(c), true) = (cloud::cfg(), a.cloud_loaded && cloud::dirty(&a.store.playlists)) {
                let _ = cloud::sync(&c, &a.store.playlists);
            }
        });
        cloud::sign_out();
        with(|a| {
            a.acc_refresh_ui();
            a.ui.set_acc_msg(tr!("Вышел. Плейлисты остались в аккаунте", "Signed out. Playlists stay in your account", "Вийшов. Плейлисти лишилися в акаунті", "Odhlásený. Playlisty zostali v účte").into());
        });
    });
    ui.on_cloud_sync(|| {
        with(|a| a.cloud_sync(true));
    });
    ui.on_set_spicy_key(|k| {
        with(|a| {
            a.store.settings.spicy_key = k.trim().to_string();
            a.ui.set_spicy_key(k); // второе поле с ключом (быстрая настройка / настройки) тоже увидит
            a.save();
        });
    });
    ui.on_import_qr(|| {
        with(|a| a.import_qr());
    });
    ui.on_share_playlist(|| {
        with(|a| a.share_playlist());
    });
    ui.on_qr_page(|d| {
        with(|a| {
            let n = a.qr_imgs.len() as i32;
            if n > 0 {
                a.qr_page = ((a.qr_page as i32 + d).rem_euclid(n)) as usize;
                a.show_qr_page();
            }
        });
    });
    ui.on_qr_save(|| {
        with(|a| a.save_qr());
    });
    ui.on_radio(|i, kind| {
        with(|a| {
            if let Some(t) = a.track_at(kind, i as usize) {
                a.ask(Confirm::Radio(t));
            }
        });
    });
    ui.on_open_artist(|i, kind| {
        with(|a| {
            if let Some(t) = a.track_at(kind, i as usize) {
                a.open_artist_of(t);
            }
        });
    });
    ui.on_nav_back(|| {
        with(|a| a.nav_back());
    });
    ui.on_artist_play(|shuffle| {
        with(|a| a.artist_play(shuffle));
    });
    ui.on_artist_radio(|| {
        with(|a| {
            if let Some(t) = a.artist.as_ref().and_then(|ar| ar.top.first().cloned()) {
                a.start_radio(t);
            }
        });
    });
    ui.on_artist_all(|| {
        with(|a| a.artist_all());
    });
    ui.on_open_card(|which, i| {
        with(|a| a.open_card(which, i as usize));
    });
    ui.on_play_in(|kind, i| {
        with(|a| {
            let list = match kind {
                4 => a.artist.as_ref().map(|ar| ar.top.clone()),
                5 => Some(a.coll.tracks.clone()),
                6 => Some(a.local.clone()),
                7 => Some(a.stat_top.clone()),
                8 => Some(a.home_quick.clone()),
                _ => None,
            };
            if let Some(list) = list {
                a.play_queue(list, i as usize);
            }
        });
    });
    ui.on_coll_play(|shuffle| {
        with(|a| {
            let t = a.coll.tracks.clone();
            if t.is_empty() {
                return;
            }
            if shuffle {
                a.set_shuffle(true);
            }
            let i = if shuffle { rand_below(t.len()) } else { 0 };
            a.play_queue(t, i);
        });
    });
    ui.on_coll_save(|| {
        with(|a| {
            if a.coll.tracks.is_empty() {
                return;
            }
            let name = a.unique_name(&a.coll.title.clone());
            a.store.playlists.push(Playlist { name: name.clone(), tracks: a.coll.tracks.clone(), source: None, ..Default::default() });
            a.save();
            a.refresh_playlists();
            a.toast(&tr!("Сохранил как плейлист «{}»", "Saved as playlist “{}”", "Зберіг як плейлист «{}»", "Uložené ako playlist „{}“", name));
        });
    });
    ui.on_set_eq_band(|i, v| {
        with(|a| {
            // 0..1 со слайдера → -12..+12 дБ с шагом 0.5
            let db = ((v * 2.0 - 1.0) * fx::MAX_DB * 2.0).round() / 2.0;
            if let Some(g) = a.store.settings.eq.get_mut(i as usize) {
                *g = db;
            }
            a.eq_anim.stop();
            a.apply_fx();
        });
    });
    ui.on_set_eq_preset(|i| {
        with(|a| {
            if let Some((_, _, g)) = fx::PRESETS.get(i as usize) {
                a.animate_eq(g.to_vec());
            }
        });
    });
    ui.on_set_speed(|v| {
        with(|a| {
            a.store.settings.speed = ((v * 20.0).round() / 20.0).clamp(0.5, 2.0);
            a.apply_fx();
        });
    });
    ui.on_set_keep_pitch(|k| {
        with(|a| {
            a.store.settings.keep_pitch = k;
            a.apply_fx();
        });
    });
    ui.on_fx_reset(|| {
        with(|a| {
            (a.store.settings.speed, a.store.settings.keep_pitch) = (1.0, true);
            a.animate_eq(vec![0.0; fx::BANDS.len()]);
            a.toast(&tr!("Звук сброшен по умолчанию", "Sound reset to defaults", "Звук скинуто до стандартного", "Zvuk vrátený na predvolené"));
        });
    });
    ui.on_want_cover(|u| {
        with(|a| a.want_cover(&u));
    });

    // интро: 60 fps ~3.5 секунды, потом таймер стопается и больше ничего не тикает.
    // первые 250мс держим t=0 — композитор как раз успевает показать окно
    let t0 = std::time::Instant::now();
    with(|a| {
        a.intro.start(TimerMode::Repeated, Duration::from_millis(16), move || {
            let ms = t0.elapsed().as_secs_f32() * 1000.0 - 250.0;
            with(|a| {
                a.ui.set_intro_t(ms.max(0.0));
                if ms > 3300.0 {
                    a.intro.stop();
                }
            });
        });
    });

    with(|a| a.restore_last());
    // без аккаунта плейлисты локальные — «Любимые треки» заводим сразу (с аккаунтом — после загрузки из облака)
    with(|a| {
        if cloud::cfg().is_none() {
            a.ensure_liked();
            a.refresh_playlists();
        }
    });
    // плейлисты из облака: сразу при запуске и потом раз в минуту
    with(|a| {
        a.acc_refresh_ui();
        a.cloud_sync(false);
        a.cloud_timer.start(TimerMode::Repeated, Duration::from_secs(60), || {
            with(|a| a.cloud_sync(false));
        });
    });
    ui.run()?;
    // закрыли плеер — неотправленные правки плейлистов на сервер
    with(|a| {
        if let (Some(c), true) = (cloud::cfg(), a.cloud_loaded && cloud::dirty(&a.store.playlists)) {
            if let Ok(s) = cloud::sync(&c, &a.store.playlists) {
                cloud::update(|acc| acc.base = s.base);
            }
        }
        a.conn_bye();
        // и свою долю статистики — то, что наслушали за последнюю минуту
        if let (Some(c), true) = (cloud::cfg(), cloud::account().verified) {
            let _ = cloud::sync_stats(&c, &a.stats);
        }
    });
    with(|a| {
        a.save_last();
        a.stats.save();
        a.end_xfade();
        if let Some(d) = &a.discord {
            d.set(None);
        }
    });

    for p in with(|a| [a.player.clone(), a.player2.clone()]).into_iter().flatten().flatten() {
        p.shutdown();
    }
    Ok(())
}


#[cfg(test)]
mod key_tests {
    #[test]
    fn key_names_ignore_layout() {
        use super::key_name;
        let left: slint::SharedString = slint::platform::Key::LeftArrow.into();
        assert_eq!(key_name("m", false, false, false, false).as_deref(), Some("M"));
        assert_eq!(key_name("ь", false, false, false, false).as_deref(), Some("M")); // та же клавиша в русской раскладке
        assert_eq!(key_name("а", true, false, false, false).as_deref(), Some("Ctrl+F"));
        assert_eq!(key_name(" ", false, false, false, false).as_deref(), Some("Space"));
        assert_eq!(key_name(&left, true, false, false, false).as_deref(), Some("Ctrl+←"));
        let shift: slint::SharedString = slint::platform::Key::Shift.into();
        assert_eq!(key_name(&shift, false, false, true, false), None);
    }
}
