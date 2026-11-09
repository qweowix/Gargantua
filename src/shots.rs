//! Снимки интерфейса без окна: программный рендер Slint в PNG. Экран юзера не трогаем.
//! Заодно тест производительности: главный экран не должен перерисовываться сам по себе,
//! а кадр — укладываться в бюджет. cargo test --release ui -- --nocapture → картинки в target/shots/

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, WindowAdapter};
use slint::{ComponentHandle, Model, VecModel};

use super::*;

thread_local! {
    static NOW: Cell<Duration> = const { Cell::new(Duration::ZERO) };
}

thread_local!(static CLIP: std::cell::RefCell<String> = Default::default());

struct Offscreen(Rc<MinimalSoftwareWindow>);

impl Platform for Offscreen {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
    // время двигаем сами: перед снимком проматываем все анимации до конца
    fn duration_since_start(&self) -> Duration {
        NOW.with(Cell::get)
    }
    fn set_clipboard_text(&self, text: &str, _: slint::platform::Clipboard) {
        CLIP.with(|c| *c.borrow_mut() = text.to_string());
    }
}

const W: u32 = 1280;
const H: u32 = 800;

fn snap(win: &MinimalSoftwareWindow, name: &str) -> image::RgbImage {
    for _ in 0..4 {
        NOW.with(|n| n.set(n.get() + Duration::from_secs(5)));
        slint::platform::update_timers_and_animations();
    }
    let mut buf = vec![slint::Rgb8Pixel::default(); (W * H) as usize];
    win.request_redraw();
    win.draw_if_needed(|r| {
        r.render(&mut buf, W as usize);
    });
    let raw: Vec<u8> = buf.iter().flat_map(|p| [p.r, p.g, p.b]).collect();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/shots");
    std::fs::create_dir_all(&dir).unwrap();
    let img = image::RgbImage::from_raw(W, H, raw).unwrap();
    img.save(dir.join(format!("{name}.png"))).unwrap();
    img
}

/// На сколько пикселей по x сдвинута строка текста между двумя кадрами (полоса y0..y1, правая колонка)
fn shift_x(a: &image::RgbImage, b: &image::RgbImage, y0: u32, y1: u32) -> i32 {
    let cols = |img: &image::RgbImage| -> Vec<bool> { (0..W).map(|x| x > 480 && (y0..y1).any(|y| img.get_pixel(x, y)[0] > 60)).collect() };
    let (ca, cb) = (cols(a), cols(b));
    (-6i32..=6)
        .min_by_key(|s| (0..W as i32).filter(|&x| ca[x as usize] != *cb.get((x + s) as usize).unwrap_or(&false)).count())
        .unwrap()
}

fn track(title: &str, artist: &str, dl: i32) -> TrackItem {
    TrackItem { title: title.into(), artist: artist.into(), source: "yt".into(), duration: "3:19".into(), dl, ..Default::default() }
}

/// Время полной перерисовки кадра программным рендером (на femtovg у юзера порядок тот же).
/// Лучший из 20: среднее плавало в 1.5 раза от фоновой нагрузки машины
fn frame_ms(win: &MinimalSoftwareWindow) -> f64 {
    let mut buf = vec![slint::Rgb8Pixel::default(); (W * H) as usize];
    (0..20)
        .map(|_| {
            let t = Instant::now();
            win.request_redraw();
            win.draw_if_needed(|r| {
                r.render(&mut buf, W as usize);
            });
            t.elapsed().as_secs_f64() * 1000.0
        })
        .fold(f64::MAX, f64::min)
}

/// Прошла секунда, никто ничего не трогал — окну перерисовываться незачем
fn idle(ui: &AppWindow, win: &MinimalSoftwareWindow) -> bool {
    win.draw_if_needed(|_| {});
    NOW.with(|n| n.set(n.get() + Duration::from_secs(1)));
    slint::platform::update_timers_and_animations();
    !ui.window().has_active_animations() && !win.draw_if_needed(|_| {})
}

#[test]
fn ui() {
    let win = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Offscreen(win.clone()))).unwrap();
    win.set_size(slint::PhysicalSize::new(W, H));
    let ui = AppWindow::new().unwrap();
    ui.show().unwrap();
    // интро уже прошло, язык — исходный русский (иначе Slint берёт системную локаль)
    ui.set_intro_t(100_000.0);
    ui.set_intro_gone(true);
    slint::select_bundled_translation("").unwrap();
    ui.invoke_copy_text("https://soundcloud.com/a/b".into());
    assert_eq!(CLIP.with(|c| c.borrow().clone()), "https://soundcloud.com/a/b");
    let tracks = Rc::new(VecModel::from(vec![
        track("Cigarettes out the Window", "TV Girl", 0),
        track("Lovers Rock", "TV Girl", 2),
        track("Not Allowed", "TV Girl", 1),
    ]));
    ui.set_pl_tracks(tracks.clone().into());
    ui.set_playlists(Rc::new(VecModel::from(vec![PlItem { name: "GOATED".into(), count: 3, liked: false }])).into());
    ui.set_pl_title("GOATED".into());
    ui.set_has_track(true);
    ui.set_now_title("Cigarettes out the Window".into());
    ui.set_now_artist("TV Girl".into());
    ui.set_volume(0.63);
    ui.set_cookies("firefox".into());
    ui.set_deps("mpv: есть ✓   ·   yt-dlp: есть ✓".into());
    ui.set_version("0.1.0".into());

    // длинный ключ не должен раздвигать окно за край
    ui.set_spicy_key("sl_sk_".repeat(20).into());
    ui.set_setup_open(true);
    snap(&win, "01-setup");
    ui.set_spicy_key("".into());
    ui.set_setup_open(false);

    // главная: строка поиска пустая — миксы, быстрый подбор, недавнее, плейлисты
    let card = |t: &str, s: &str| Card { title: t.into(), sub: s.into(), ..Default::default() };
    ui.set_home_hello("Добрый вечер".into());
    ui.set_home_empty(false);
    ui.set_home_mixes(Rc::new(VecModel::from(vec![card("Твой микс", "TV Girl, Cigarettes"), card("Микс: TV Girl", "TV Girl, Mac DeMarco"), card("Открытия", "Beach House")])).into());
    ui.set_home_quick(tracks.clone().into());
    ui.set_home_recent(Rc::new(VecModel::from(vec![card("Lovers Rock", "TV Girl"), card("Это любовь", "Скриптонит")])).into());
    ui.set_home_pls(Rc::new(VecModel::from(vec![card("GOATED", "3 треков")])).into());
    ui.set_page("search".into());
    snap(&win, "00-home");
    assert!(idle(&ui, &win), "главная перерисовывается сама по себе");
    let home_ms = frame_ms(&win);
    println!("перф: главная {home_ms:.1} мс/кадр");
    assert!(home_ms < 80.0, "главная {home_ms:.1} мс/кадр — дорого");

    ui.set_page("playlist".into());
    ui.set_pl_total(3);
    snap(&win, "02-playlist");

    // перф: играет музыка, плейлист на 300 треков — главный экран стоит и рисуется быстро
    let big: Vec<TrackItem> = (0..300).map(|i| track(&format!("Track {i}"), "TV Girl", i % 3)).collect();
    ui.set_pl_tracks(Rc::new(VecModel::from(big)).into());
    ui.set_playing(true);
    snap(&win, "02-playlist-big");
    assert!(idle(&ui, &win), "главный экран перерисовывается сам по себе, пока играет музыка");
    let main_ms = frame_ms(&win);
    println!("перф: главный экран {main_ms:.1} мс/кадр");
    ui.set_pl_tracks(tracks.clone().into());

    ui.set_pl_desc("Ночные треки для дороги домой, когда в наушниках только дождь и TV Girl".into());
    ui.set_edit_name("GOATED".into());
    ui.set_edit_desc("Ночные треки для дороги домой".into());
    ui.set_modal(7);
    snap(&win, "02-playlist-edit");
    ui.set_modal(0);
    snap(&win, "02-playlist-desc");

    ui.set_confirm_title("Скачать в mp3?".into());
    ui.set_confirm_text("«TV Girl — Lovers Rock» сохранится в /home/x/Music/BlackHole".into());
    ui.set_confirm_ok("Скачать".into());
    ui.set_modal(5);
    snap(&win, "03-confirm");

    ui.set_ask_want("ROSÉ — new trick  ·  3:02".into());
    ui.set_ask_got("NOCTICADIA — TRICK  ·  2:31".into());
    ui.set_ask_left(3);
    ui.set_modal(6);
    snap(&win, "04-review");
    ui.set_modal(0);

    ui.set_page("settings".into());
    snap(&win, "05-settings");
    // аккаунт: форма входа, ожидание письма, вошёл
    ui.set_acc_on(true);
    ui.set_acc_msg("Неверная почта или пароль".into());
    snap(&win, "05a-account");
    ui.set_acc_reg(true);
    snap(&win, "05a-account-reg");
    ui.set_acc_reg(false);
    ui.set_acc_state(1);
    ui.set_acc_email("qweowix@example.com".into());
    ui.set_acc_msg("".into());
    snap(&win, "05b-account-verify");
    ui.set_acc_state(2);
    ui.set_acc_msg("Синхронизировано в 14:02".into());
    ui.set_acc_letter("Q".into());
    ui.set_page("account".into());
    snap(&win, "05c-account-in");
    ui.set_page("playlist".into());
    snap(&win, "05d-playlist-sync");
    ui.set_acc_on(false);
    ui.set_acc_state(0);
    ui.set_acc_msg("".into());
    ui.set_page("settings".into());
    for t in [1, 3, 5] {
        ui.global::<Theme>().set_id(t);
        snap(&win, &format!("06-theme-{t}"));
    }
    ui.global::<Theme>().set_id(0);

    ui.set_page("about".into());
    snap(&win, "07-about");

    // статистика: год клетками, любимый артист, топ треков
    let heat: Vec<HeatCell> = (0..370).map(|i| HeatCell { level: [0, 0, 1, 2, 0, 3, 4][(i * 7 + i / 3) % 7] as i32, tip: format!("день {i}").into() }).collect();
    ui.set_st_heat(Rc::new(VecModel::from(heat)).into());
    ui.set_st_months(Rc::new(VecModel::from(vec![MonthMark { col: 3, text: "ноя".into() }, MonthMark { col: 8, text: "дек".into() }])).into());
    ui.set_st_today("1 ч 12 мин".into());
    ui.set_st_week("9 ч 40 мин".into());
    ui.set_st_month("31 ч".into());
    ui.set_st_total("112 ч 5 мин".into());
    let art = |n: &str, t: &str| StatArtist { name: n.into(), time: t.into() };
    ui.set_st_artists(Rc::new(VecModel::from(vec![art("TV Girl", "41 ч 3 мин"), art("Скриптонит", "12 ч")])).into());
    ui.set_st_more(Rc::new(VecModel::from(vec![art("Скриптонит", "12 ч")])).into());
    ui.set_st_tracks(tracks.clone().into());
    ui.set_page("stats".into());
    snap(&win, "12-stats");
    ui.set_local_tracks(tracks.clone().into());
    ui.set_music_dirs(Rc::new(VecModel::from(vec![slint::SharedString::from("/home/x/Музыка")])).into());
    ui.set_page("local".into());
    snap(&win, "13-local");
    ui.set_sleep_mode(2);
    ui.set_sleep_left("29".into());
    snap(&win, "14-sleep");
    // таймер сна тикает в Rust, а сам экран из-за него перерисовываться не должен
    assert!(idle(&ui, &win), "экран перерисовывается сам по себе с включённым таймером сна");
    let kb = |n: &str, k: &str| KeyBind { name: n.into(), key: k.into() };
    ui.set_key_binds(Rc::new(VecModel::from(vec![kb("Пауза / играть", "Пробел"), kb("Следующий трек", "Ctrl+→"), kb("Без звука", "M")])).into());
    ui.set_rebinding(1);
    ui.set_page("settings".into());
    snap(&win, "15-settings-new");
    ui.set_rebinding(-1);

    // экран текста: точки проигрыша раскрыты над второй строкой
    let rows = |s: &str| -> slint::ModelRc<LyrRow> {
        let chars: Vec<slint::SharedString> = s.chars().map(|c| c.to_string().into()).collect();
        let pre: Vec<slint::SharedString> = s.char_indices().map(|(i, c)| s[..i + c.len_utf8()].into()).collect();
        Rc::new(VecModel::from(vec![LyrRow { text: s.into(), chars: Rc::new(VecModel::from(chars)).into(), pre: Rc::new(VecModel::from(pre)).into(), a: 0.0, b: 1.0 }])).into()
    };
    let line = |s: &str, top: f32, dots: bool| LyrLine { rows: if dots { rows("") } else { rows(s) }, top, empty: false, right: false, dots, bg: false, tr: Default::default() };
    ui.set_lyr_lines(
        Rc::new(VecModel::from(vec![
            line("She'd just say she did", 0.0, false),
            line("", 84.0, true),
            line("Boy now who smokes", 84.0, false),
            line("Cigarettes out the window", 168.0, false),
        ]))
        .into(),
    );
    ui.set_lyr_state(1);
    ui.set_lyr_cur(1);
    ui.set_lyr_cur_top(84.0);
    ui.set_act0(1);
    ui.set_p0(0.5);
    ui.set_lyr_dots_open(1);
    ui.set_lyrics_open(true);
    snap(&win, "08-lyrics");
    // lyrics тикает 20 fps: главный интерфейс под ним рисоваться не должен, кадр — дешёвый
    let lyr_ms = frame_ms(&win);
    println!("перф: экран текста {lyr_ms:.1} мс/кадр");
    // худший случай: поётся строка по буквам — волна, заливка и ореол у каждой буквы
    ui.set_lyr_dots_open(-1);
    ui.set_karaoke(true);
    ui.set_act0(2);
    ui.set_p0(0.5);
    snap(&win, "08-lyrics-karaoke");
    let kar_ms = frame_ms(&win);
    println!("перф: караоке {kar_ms:.1} мс/кадр");
    // строка допета: по буквам (волна ещё гаснет) и потом цельным текстом — на месте, без сдвига
    ui.set_p0(1.0);
    let by_letters = snap(&win, "10-line-letters");
    ui.set_act0(-1);
    let whole = snap(&win, "11-line-whole");
    let dx = shift_x(&by_letters, &whole, 280, 335);
    println!("сдвиг строки при переходе с букв на цельный текст: {dx} px");
    assert_eq!(dx, 0, "строка прыгает на {dx} px, когда буквы снова становятся цельным текстом");
    ui.set_lyr_idle(true);
    snap(&win, "09-lyrics-idle");
    assert!(tracks.row_count() == 3);
    // бюджеты с запасом под слабое железо; программный рендер медленнее GPU,
    // замер на i3-7020U: главный ~39, текст ~15 (до правки текст рисовал ещё и главный экран под собой)
    assert!(main_ms < 80.0, "главный экран {main_ms:.1} мс/кадр — дорого");
    assert!(lyr_ms < 30.0, "экран текста {lyr_ms:.1} мс/кадр — дорого, главный интерфейс под ним снова рисуется?");
    assert!(kar_ms < 30.0, "караоке {kar_ms:.1} мс/кадр — дорого");
}

/// Замер рывков: кадр за кадром после открытия текста и после смены песни (cargo test --release hitch -- --ignored --nocapture)
#[test]
#[ignore]
fn hitch() {
    let win = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Offscreen(win.clone()))).unwrap();
    win.set_size(slint::PhysicalSize::new(W, H));
    let ui = AppWindow::new().unwrap();
    ui.show().unwrap();
    ui.set_intro_t(100_000.0);
    ui.set_intro_gone(true);
    let mut buf = vec![slint::Rgb8Pixel::default(); (W * H) as usize];
    let mut run = |what: &str, n: usize, ui: &AppWindow, tick: &dyn Fn(&AppWindow, usize)| {
        let mut v = vec![];
        for f in 0..n {
            NOW.with(|t| t.set(t.get() + Duration::from_millis(16)));
            tick(ui, f);
            slint::platform::update_timers_and_animations();
            let t = Instant::now();
            win.request_redraw();
            win.draw_if_needed(|r| {
                r.render(&mut buf, W as usize);
            });
            v.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        let chunk: Vec<String> = v.chunks(6).map(|c| format!("{:.0}", c.iter().cloned().fold(0.0, f64::max))).collect();
        println!("{what}: макс по 0.1 с → {}", chunk.join(" "));
    };
    let song = |n: usize, word: &str| -> slint::ModelRc<LyrLine> {
        let lines: Vec<LyrLine> = (0..n)
            .map(|i| {
                let s = format!("{word} line number {i} goes like this");
                let chars: Vec<slint::SharedString> = s.chars().map(|c| c.to_string().into()).collect();
                let pre: Vec<slint::SharedString> = s.char_indices().map(|(k, c)| s[..k + c.len_utf8()].into()).collect();
                let row = LyrRow { text: s.as_str().into(), chars: Rc::new(VecModel::from(chars)).into(), pre: Rc::new(VecModel::from(pre)).into(), a: 0.0, b: 1.0 };
                LyrLine { rows: Rc::new(VecModel::from(vec![row])).into(), top: i as f32 * 84.0, empty: false, right: false, dots: false, bg: false, tr: Default::default() }
            })
            .collect();
        Rc::new(VecModel::from(lines)).into()
    };
    // караоке идёт: заливка строки растёт, как от тика lyrics
    let sing = |ui: &AppWindow, f: usize| ui.set_p0((f as f32 * 0.01) % 1.0);
    ui.set_karaoke(true);
    ui.set_lyr_lines(song(70, "addiction"));
    ui.set_lyr_state(1);
    ui.set_lyr_cur(0);
    ui.set_act0(0);
    run("главная", 12, &ui, &|_, _| {});
    // обычное открытие: экран текста выезжает снизу поверх главного, с фоном из обложки
    let px = |w: u32, a: u8| {
        let mut b = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(w, w);
        for (i, p) in b.make_mut_slice().iter_mut().enumerate() {
            *p = slint::Rgba8Pixel { r: (i * 7 % 255) as u8, g: 40, b: 160, a };
        }
        slint::Image::from_rgba8(b)
    };
    ui.set_bg_img(px(20, 255));
    ui.set_blob_img(px(48, 160));
    ui.set_has_big(true);
    ui.set_lyrics_open(true);
    run("выезд текста", 40, &ui, &sing);
    ui.set_lyrics_open(false);
    run("закрыли", 40, &ui, &|_, _| {});
    ui.set_lyrics_open(true);
    run("открыли текст", 90, &ui, &sing);
    run("текст, обычный ход", 30, &ui, &sing);
    for exit in [0, 7, 4, 6] {
        ui.set_lyr_exit(exit);
        ui.set_lyr_lines(song(70, &format!("song{exit}")));
        ui.set_lyr_jump(9);
        ui.set_lyr_cur(10);
        ui.set_lyr_cur_top(840.0);
        ui.set_act0(10);
        run(&format!("сменили песню, уход {exit}"), 90, &ui, &sing);
        ui.set_act0(11);
        ui.set_lyr_cur(11);
        ui.set_lyr_cur_top(924.0);
        run(&format!("следующая строка, уход {exit}"), 60, &ui, &sing);
    }
}

/// Дуэт: вторая строка начинается, пока первая ещё звучит. Прокрутка должна ехать плавно —
/// печатаем, где по вертикали первая (белая) строка каждый кадр (cargo test --release duet -- --ignored --nocapture)
#[test]
#[ignore]
fn duet_scroll() {
    let win = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Offscreen(win.clone()))).unwrap();
    win.set_size(slint::PhysicalSize::new(W, H));
    let ui = AppWindow::new().unwrap();
    ui.show().unwrap();
    ui.set_intro_t(100_000.0);
    ui.set_intro_gone(true);
    let lines: Vec<LyrLine> = (0..30)
        .map(|i| {
            let s = format!("line number {i} goes like this");
            let chars: Vec<slint::SharedString> = s.chars().map(|c| c.to_string().into()).collect();
            let pre: Vec<slint::SharedString> = s.char_indices().map(|(k, c)| s[..k + c.len_utf8()].into()).collect();
            let row = LyrRow { text: s.as_str().into(), chars: Rc::new(VecModel::from(chars)).into(), pre: Rc::new(VecModel::from(pre)).into(), a: 0.0, b: 1.0 };
            LyrLine { rows: Rc::new(VecModel::from(vec![row])).into(), top: i as f32 * 84.0, empty: false, right: false, dots: false, bg: false, tr: Default::default() }
        })
        .collect();
    ui.set_lyr_lines(Rc::new(VecModel::from(lines)).into());
    ui.set_lyr_state(1);
    ui.set_lyr_cur(10);
    ui.set_lyr_cur_top(840.0);
    ui.set_act0(10);
    ui.set_p0(1.0);
    ui.set_lyrics_open(true);
    snap(&win, "duet-0");
    let mut buf = vec![slint::Rgb8Pixel::default(); (W * H) as usize];
    // верх самой яркой строки: первая сверху строка пикселей с почти белым цветом
    let mut ms = vec![];
    let mut top = || {
        let t = Instant::now();
        win.request_redraw();
        win.draw_if_needed(|r| {
            r.render(&mut buf, W as usize);
        });
        ms.push(t.elapsed().as_secs_f64() * 1000.0);
        (0..H).find(|&y| (0..W).any(|x| buf[(y * W + x) as usize].r > 235 && buf[(y * W + x) as usize].b > 235)).unwrap_or(0) as i32
    };
    let mut ys = vec![top()];
    for f in 0..60 {
        if f == 2 {
            ui.set_act1(11);
            ui.set_p1(1.0);
            ui.set_act_max(11);
            ui.set_lyr_cur(11);
            ui.set_lyr_cur_top(924.0);
        }
        NOW.with(|t| t.set(t.get() + Duration::from_millis(16)));
        slint::platform::update_timers_and_animations();
        ys.push(top());
    }
    let steps: Vec<i32> = ys.windows(2).map(|w| w[1] - w[0]).collect();
    drop(top);
    let worst: Vec<String> = ms.chunks(6).map(|c| format!("{:.0}", c.iter().cloned().fold(0.0, f64::max))).collect();
    println!("y: {ys:?}\nшаг: {steps:?}\nмс/кадр, макс по 0.1 с: {}", worst.join(" "));
}

/// Переход на следующую строку с караоке и без: во что обходится караоке на прокрутке
/// (cargo test --release karaoke_step -- --ignored --nocapture)
#[test]
#[ignore]
fn karaoke_step() {
    let win = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Offscreen(win.clone()))).unwrap();
    win.set_size(slint::PhysicalSize::new(W, H));
    let ui = AppWindow::new().unwrap();
    ui.show().unwrap();
    ui.set_intro_t(100_000.0);
    ui.set_intro_gone(true);
    let lines: Vec<LyrLine> = (0..30)
        .map(|i| {
            let s = format!("Сколько б не дрались, но нам, как всегда {i}");
            let chars: Vec<slint::SharedString> = s.chars().map(|c| c.to_string().into()).collect();
            let pre: Vec<slint::SharedString> = s.char_indices().map(|(k, c)| s[..k + c.len_utf8()].into()).collect();
            let row = LyrRow { text: s.as_str().into(), chars: Rc::new(VecModel::from(chars)).into(), pre: Rc::new(VecModel::from(pre)).into(), a: 0.0, b: 1.0 };
            LyrLine { rows: Rc::new(VecModel::from(vec![row])).into(), top: i as f32 * 84.0, empty: false, right: false, dots: false, bg: false, tr: Default::default() }
        })
        .collect();
    ui.set_lyr_lines(Rc::new(VecModel::from(lines)).into());
    ui.set_lyr_state(1);
    ui.set_lyrics_open(true);
    let mut buf = vec![slint::Rgb8Pixel::default(); (W * H) as usize];
    for (name, karaoke, glow) in [("без караоке", false, 1.0), ("караоке", true, 1.0), ("караоке без свечения", true, 0.0)] {
        ui.set_karaoke(karaoke);
        ui.set_lyr_glow(glow);
        ui.set_lyr_cur(10);
        ui.set_lyr_cur_top(840.0);
        ui.set_act0(10);
        ui.set_p0(0.6);
        ui.set_lyr_prep(11);
        snap(&win, "karaoke-step");
        let mut ms = vec![];
        for f in 0..60 {
            if f == 4 {
                ui.set_lyr_cur(11);
                ui.set_lyr_cur_top(924.0);
                ui.set_act0(11);
                ui.set_lyr_prep(-1);
            }
            if f == 50 {
                ui.set_lyr_prep(12);
            }
            if f < 4 {
                ui.set_p0(0.6 + f as f32 * 0.1);
            } else {
                ui.set_p0((f - 4) as f32 * 0.01);
            }
            NOW.with(|t| t.set(t.get() + Duration::from_millis(16)));
            slint::platform::update_timers_and_animations();
            let t = Instant::now();
            win.request_redraw();
            win.draw_if_needed(|r| {
                r.render(&mut buf, W as usize);
            });
            ms.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        let worst: Vec<String> = ms.chunks(6).map(|c| format!("{:.0}", c.iter().cloned().fold(0.0, f64::max))).collect();
        println!("{name}: мс/кадр, макс по 0.1 с → {}", worst.join(" "));
        println!("  первые кадры: {:?}", ms.iter().take(12).map(|m| m.round() as i32).collect::<Vec<_>>());
    }
}

/// Растворение строк у краёв экрана текста: снимки при 0, 10 и 80 px — смотреть глазами (target/shots/edge-*.png)
/// и проверка, что при 10 px крайние строки не пропадают целиком
#[test]
#[ignore]
fn edge_fade() {
    let win = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Offscreen(win.clone()))).unwrap();
    win.set_size(slint::PhysicalSize::new(W, H));
    let ui = AppWindow::new().unwrap();
    ui.show().unwrap();
    ui.set_intro_t(100_000.0);
    ui.set_intro_gone(true);
    let lines: Vec<LyrLine> = (0..40)
        .map(|i| {
            let s = format!("Строка номер {i} для проверки краёв");
            let chars: Vec<slint::SharedString> = s.chars().map(|c| c.to_string().into()).collect();
            let pre: Vec<slint::SharedString> = s.char_indices().map(|(k, c)| s[..k + c.len_utf8()].into()).collect();
            let row = LyrRow { text: s.as_str().into(), chars: Rc::new(VecModel::from(chars)).into(), pre: Rc::new(VecModel::from(pre)).into(), a: 0.0, b: 1.0 };
            LyrLine { rows: Rc::new(VecModel::from(vec![row])).into(), top: i as f32 * 84.0, empty: false, right: false, dots: false, bg: false, tr: Default::default() }
        })
        .collect();
    ui.set_lyr_lines(Rc::new(VecModel::from(lines)).into());
    ui.set_lyr_state(1);
    ui.set_lyr_exit(0);
    ui.set_lyrics_open(true);
    ui.set_lyr_cur(15);
    ui.set_lyr_cur_top(15.0 * 84.0);
    // сколько рядов пикселей с текстом (светлых) в верхней и нижней восьмушке экрана
    let lit = |img: &image::RgbImage, y0: u32, y1: u32| (y0..y1).filter(|&y| (500..W).any(|x| img.get_pixel(x, y)[0] > 70)).count();
    let mut got = vec![];
    for edge in [0.0, 10.0, 80.0] {
        ui.set_lyr_edge(edge);
        let img = snap(&win, &format!("edge-{edge}"));
        let (top, bottom) = (lit(&img, 0, H / 8), lit(&img, H - H / 8, H));
        println!("край {edge} px: светлых рядов сверху {top}, снизу {bottom}");
        got.push((top, bottom));
    }
    // 10 px гасит только самую кромку — строк должно остаться почти столько же, сколько без растворения
    assert!(got[1].0 * 10 >= got[0].0 * 7 && got[1].1 * 10 >= got[0].1 * 7, "при 10 px пропали строки у края: {got:?}");
}
