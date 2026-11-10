//! Воспроизведение через mpv (+ yt-dlp) по JSON IPC.
//! Один поток владеет сокетом/пайпом: команды и опрос идут строго по очереди,
//! поэтому одинаково работает и с unix-сокетом, и с виндовым named pipe.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::process::{Child, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::{json, Value};

use crate::util::{command, find_bin};

pub enum Cmd {
    Load(String, u64),
    Toggle,
    Stop,
    Seek(f64),
    Volume(f64),
    Af(String),      // цепочка фильтров (эквалайзер); пусто — без фильтров
    Speed(f64),
    KeepPitch(bool), // при смене скорости держать тональность (иначе — slowed / sped up)
    Quit,
}

pub enum Ev {
    Status { tok: u64, pos: f64, dur: f64, paused: bool, loading: bool },
    Ended(u64),
    Failed(u64, String),
}

/// Уровни полос спектра (дБ) с временем трека, к которому они относятся: mpv считает звук заранее
type Viz = Arc<Mutex<std::collections::VecDeque<(f64, [f32; crate::fx::VIZ_BANDS])>>>;

#[derive(Clone)]
pub struct Player {
    tx: mpsc::Sender<Cmd>,
    handle: Arc<Mutex<Option<JoinHandle<()>>>>,
    viz: Viz,
}

impl Player {
    pub fn spawn(volume: f64, on: impl Fn(Ev) + Send + 'static) -> Result<Player> {
        let mpv = find_bin("mpv").context("mpv не найден")?;
        // у каждого mpv свой сокет: с кроссфейдом их два, и с общим именем второй плеер подключался
        // к первому mpv — оба рулили одним, и переход глушил свежий трек
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if n == 0 && cfg!(target_os = "linux") {
            sweep_sockets();
        }
        let ipc = if cfg!(windows) {
            format!(r"\\.\pipe\blackhole-mpv-{}-{n}", std::process::id())
        } else {
            std::env::temp_dir().join(format!("blackhole-mpv-{}-{n}.sock", std::process::id())).display().to_string()
        };
        let mut cmd = command(&mpv);
        cmd.args([
            "--idle=yes",
            "--no-video",
            "--no-terminal",
            "--audio-display=no",
            "--ytdl-format=bestaudio/best",
            "--cache=yes",
            "--demuxer-max-bytes=32MiB",
        ])
        .arg(format!("--input-ipc-server={ipc}"))
        .arg(format!("--volume={volume:.0}"))
        .stdin(Stdio::null())
        // в stdout mpv сам ничего не пишет (--no-terminal) — туда фильтр спектра печатает уровни полос
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
        if let Some(y) = find_bin("yt-dlp") {
            cmd.arg(format!("--script-opts=ytdl_hook-ytdl_path={}", y.display()));
        }
        // mpv должен сдохнуть вместе с нами, даже если нас прибили
        #[cfg(target_os = "linux")]
        unsafe {
            use std::os::unix::process::CommandExt;
            cmd.pre_exec(|| {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                Ok(())
            });
        }
        let mut child = cmd.spawn().context("не смог запустить mpv")?;
        #[cfg(windows)]
        kill_with_parent(&child);
        let viz = Viz::default();
        // читаем всегда, даже когда спектр никому не нужен: иначе труба забьётся и mpv встанет
        if let Some(out) = child.stdout.take() {
            let viz = viz.clone();
            thread::spawn(move || read_viz(out, viz));
        }
        let (tx, rx) = mpsc::channel();
        let h = thread::spawn(move || run(child, ipc, rx, on));
        Ok(Player { tx, handle: Arc::new(Mutex::new(Some(h))), viz })
    }

    /// Уровни полос (дБ) того места трека, что звучит сейчас
    pub fn viz_at(&self, pos: f64) -> Option<[f32; crate::fx::VIZ_BANDS]> {
        let mut q = self.viz.lock().unwrap();
        // всё, что уже отзвучало, кроме самого свежего из отзвучавшего
        while q.len() > 1 && q[1].0 <= pos {
            q.pop_front();
        }
        q.front().filter(|f| f.0 <= pos && pos - f.0 < 0.5).map(|f| f.1)
    }

    pub fn send(&self, c: Cmd) {
        let _ = self.tx.send(c);
    }

    pub fn shutdown(&self) {
        self.send(Cmd::Quit);
        if let Some(h) = self.handle.lock().unwrap().take() {
            let _ = h.join();
        }
    }
}

/// Разбор вывода фильтра спектра (ametadata): строка «frame:… pts_time:12.3», за ней по строке
/// «lavfi.astats.N.Peak_level=-23.4» на полосу
fn read_viz(out: impl Read, viz: Viz) {
    const N: usize = crate::fx::VIZ_BANDS;
    let (mut pts, mut bands) = (0.0f64, [f32::NEG_INFINITY; N]);
    for line in BufReader::new(out).lines() {
        let Ok(line) = line else { break };
        if let Some(t) = line.strip_prefix("frame:").and_then(|l| l.split("pts_time:").nth(1)) {
            pts = t.trim().parse().unwrap_or(pts);
        } else if let Some((k, v)) = line.strip_prefix("lavfi.astats.").and_then(|l| l.split_once(".Peak_level=")) {
            let Ok(k) = k.parse::<usize>() else { continue };
            if (1..=N).contains(&k) {
                bands[k - 1] = v.trim().parse().unwrap_or(f32::NEG_INFINITY);
            }
            if k == N {
                let mut q = viz.lock().unwrap();
                // перемотали назад или новый трек — то, что насчитано раньше, уже не про это место
                if q.back().is_some_and(|b| b.0 > pts) {
                    q.clear();
                }
                q.push_back((pts, bands));
                if q.len() > 400 {
                    q.pop_front();
                }
            }
        }
    }
}

/// Сокеты mpv от прошлых запусков, которые убили не дав прибраться: процесса уже нет — файл в мусор
fn sweep_sockets() {
    let Ok(rd) = std::fs::read_dir(std::env::temp_dir()) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let Some(rest) = name.strip_prefix("blackhole-mpv-").and_then(|r| r.strip_suffix(".sock")) else { continue };
        let pid = rest.split('-').next().unwrap_or("");
        if !pid.is_empty() && !std::path::Path::new("/proc").join(pid).exists() {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

#[cfg(windows)]
fn kill_with_parent(child: &Child) {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::JobObjects::*;
    unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            return;
        }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const _,
            std::mem::size_of_val(&info) as u32,
        );
        AssignProcessToJobObject(job, child.as_raw_handle() as _);
        // хэндл job'а не закрываем: винда закроет его при смерти процесса и прибьёт mpv
    }
}

struct Conn {
    r: BufReader<Box<dyn Read + Send>>,
    w: Box<dyn Write + Send>,
    id: u64,
}

impl Conn {
    fn open(path: &str) -> io::Result<Conn> {
        #[cfg(unix)]
        {
            let s = std::os::unix::net::UnixStream::connect(path)?;
            s.set_read_timeout(Some(Duration::from_secs(10)))?;
            let r = s.try_clone()?;
            Ok(Conn { r: BufReader::new(Box::new(r)), w: Box::new(s), id: 0 })
        }
        #[cfg(windows)]
        {
            let f = std::fs::OpenOptions::new().read(true).write(true).open(path)?;
            let r = f.try_clone()?;
            Ok(Conn { r: BufReader::new(Box::new(r)), w: Box::new(f), id: 0 })
        }
    }

    /// Шлёт пачку команд, возвращает их ответы и все события, что пришли по пути
    fn call(&mut self, cmds: &[Value]) -> io::Result<(Vec<Value>, Vec<Value>)> {
        let base = self.id;
        let n = cmds.len() as u64;
        let mut s = String::new();
        for (i, c) in cmds.iter().enumerate() {
            s += &json!({"command": c, "request_id": base + i as u64 + 1}).to_string();
            s.push('\n');
        }
        self.id += n;
        self.w.write_all(s.as_bytes())?;
        self.w.flush()?;

        let mut out = vec![Value::Null; cmds.len()];
        let mut events = Vec::new();
        let mut got = 0;
        while got < n {
            let mut line = String::new();
            if self.r.read_line(&mut line)? == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
            if let Some(id) = v.get("request_id").and_then(Value::as_u64) {
                if id > base && id <= base + n {
                    out[(id - base - 1) as usize] = v.get("data").cloned().unwrap_or(Value::Null);
                    got += 1;
                }
            } else if v.get("event").is_some() {
                events.push(v);
            }
        }
        Ok((out, events))
    }
}

fn run(mut child: Child, ipc: String, rx: mpsc::Receiver<Cmd>, on: impl Fn(Ev)) {
    let mut conn = None;
    for _ in 0..100 {
        if let Ok(c) = Conn::open(&ipc) {
            conn = Some(c);
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let Some(mut c) = conn else {
        on(Ev::Failed(0, "mpv не стартанул".into()));
        let _ = child.kill();
        return;
    };

    let mut tok = 0u64; // токен трека, который сейчас в mpv
    let mut loading = false;
    let mut af: Option<String> = None; // та же цепочка второй раз — mpv пересобрал бы фильтры со щелчком
    loop {
        match rx.recv_timeout(Duration::from_millis(333)) {
            Ok(Cmd::Quit) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Ok(cmd) => {
                let cmds = match cmd {
                    Cmd::Load(url, t) => {
                        tok = t;
                        loading = true;
                        vec![json!(["loadfile", url, "replace"]), json!(["set_property", "pause", false])]
                    }
                    Cmd::Toggle => vec![json!(["cycle", "pause"])],
                    Cmd::Stop => vec![json!(["stop"])],
                    Cmd::Seek(s) => vec![json!(["seek", s, "absolute"])],
                    Cmd::Volume(v) => vec![json!(["set_property", "volume", v])],
                    Cmd::Af(f) if af.as_deref() == Some(f.as_str()) => vec![],
                    Cmd::Af(f) => {
                        af = Some(f.clone());
                        vec![json!(["set_property", "af", f])]
                    }
                    Cmd::Speed(s) => vec![json!(["set_property", "speed", s])],
                    Cmd::KeepPitch(k) => vec![json!(["set_property", "audio-pitch-correction", k])],
                    Cmd::Quit => unreachable!(),
                };
                if c.call(&cmds).is_err() {
                    on(Ev::Failed(tok, "mpv отвалился".into()));
                    break;
                }
                continue;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }

        let props = ["time-pos", "duration", "pause"].map(|p| json!(["get_property", p]));
        let Ok((vals, events)) = c.call(&props) else {
            on(Ev::Failed(tok, "mpv отвалился".into()));
            break;
        };
        for e in &events {
            match (e["event"].as_str(), e["reason"].as_str()) {
                (Some("end-file"), Some("eof")) => on(Ev::Ended(tok)),
                (Some("end-file"), Some("error")) => {
                    let msg = e["file_error"].as_str().unwrap_or("ошибка воспроизведения").to_string();
                    on(Ev::Failed(tok, msg));
                }
                _ => {}
            }
        }
        let pos = vals[0].as_f64();
        if pos.is_some() {
            loading = false;
        }
        on(Ev::Status {
            tok,
            pos: pos.unwrap_or(0.0),
            dur: vals[1].as_f64().unwrap_or(0.0),
            paused: vals[2].as_bool().unwrap_or(false),
            loading,
        });
    }
    let _ = c.call(&[json!(["quit"])]);
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    /// Два плеера (кроссфейд) — два разных mpv: трек, загруженный в один, другой не видит
    #[test]
    fn two_players_are_separate() {
        if crate::util::find_bin("mpv").is_none() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let spawn = |id: u8| {
            let tx = tx.clone();
            super::Player::spawn(0.0, move |e| {
                if let super::Ev::Status { tok, pos, .. } = e {
                    let _ = tx.send((id, tok, pos));
                }
            })
            .unwrap()
        };
        let (a, b) = (spawn(1), spawn(2));
        a.send(super::Cmd::Load("av://lavfi:sine=d=30".into(), 7));
        let t0 = std::time::Instant::now();
        let (mut a_plays, mut b_tok) = (false, 0);
        while t0.elapsed().as_secs() < 10 && !(a_plays && b_tok == 0 && t0.elapsed().as_secs() >= 2) {
            if let Ok((id, tok, pos)) = rx.recv_timeout(std::time::Duration::from_millis(500)) {
                if id == 1 && tok == 7 && pos > 0.3 {
                    a_plays = true;
                }
                if id == 2 {
                    b_tok = b_tok.max(tok);
                }
            }
        }
        a.shutdown();
        b.shutdown();
        assert!(a_plays, "первый mpv не заиграл");
        assert_eq!(b_tok, 0, "второй плеер видит трек первого — они делят один mpv");
    }

    /// Спектр из настоящего mpv: тихий синус 440 Гц — уровни приходят с временем трека, громче всех полоса у 440
    #[test]
    fn viz_from_mpv() {
        if crate::util::find_bin("mpv").is_none() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let p = super::Player::spawn(0.0, move |e| {
            if let super::Ev::Status { pos, .. } = e {
                let _ = tx.send(pos);
            }
        })
        .unwrap();
        p.send(super::Cmd::Af(crate::fx::chain(&[0.0; 10], 1.0, true, true)));
        p.send(super::Cmd::Load("av://lavfi:sine=f=440:d=30".into(), 1));
        let t0 = std::time::Instant::now();
        let mut got = None;
        while t0.elapsed().as_secs() < 10 && got.is_none() {
            if let Ok(pos) = rx.recv_timeout(std::time::Duration::from_millis(500)) {
                got = Some(pos).filter(|p| *p > 0.5).and_then(|pos| p.viz_at(pos));
            }
        }
        p.shutdown();
        let bands = got.expect("mpv не отдал уровни спектра");
        let loud = (0..bands.len()).max_by(|a, b| bands[*a].total_cmp(&bands[*b])).unwrap();
        // полосы идут от 50 до 5200 Гц по логарифму: 440 Гц — седьмая-восьмая из шестнадцати
        assert!((6..=8).contains(&loud) && bands[loud] > -30.0, "полосы {bands:?}");
    }

    /// Живой mpv + yt-dlp: трек должен перестать грузиться и пойти по времени (нужны сеть и mpv)
    #[test]
    #[ignore]
    fn plays() {
        let (tx, rx) = std::sync::mpsc::channel();
        let p = super::Player::spawn(0.0, move |e| {
            if let super::Ev::Status { tok, pos, loading, .. } = e {
                let _ = tx.send((tok, pos, loading));
            } else if let super::Ev::Failed(_, m) = e {
                panic!("{m}");
            }
        })
        .unwrap();
        p.send(super::Cmd::Load("https://music.youtube.com/watch?v=dQw4w9WgXcQ".into(), 1));
        let t0 = std::time::Instant::now();
        while t0.elapsed().as_secs() < 30 {
            if let Ok((tok, pos, loading)) = rx.recv_timeout(std::time::Duration::from_secs(1)) {
                println!("{:.1}s tok={tok} pos={pos:.2} loading={loading}", t0.elapsed().as_secs_f32());
                if tok == 1 && !loading && pos > 1.0 {
                    p.shutdown();
                    return;
                }
            }
        }
        panic!("не заиграло за 30 с");
    }
}
