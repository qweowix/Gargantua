//! yt-dlp, который живёт в фоне: Python, сам yt-dlp, куки Firefox и плеер YouTube грузятся один раз,
//! а не на каждый трек. Плюс быстрый путь — только tv_samsung-клиент, без страницы и initial data (меньше запросов
//! к YouTube); не вышло — тот же трек обычным путём. Замер на i3: ~10 с запуском yt-dlp на трек
//! против ~2 с здесь (без рекламы, см. FAST). Два процесса: для play (обычный приоритет) и для предзагрузки (nice 19, его
//! запускает поток предзагрузки). Без дела процесс сам выходит, чтобы не держать ~80 МБ памяти зря

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::util::{command, find_bin};

const SCRIPT: &str = r#"
import sys, json, select
sys.path.insert(0, sys.argv[1])  # yt-dlp из ~/.local/bin — zip-архив, импортируется прямо из него
import yt_dlp
# подготовленный плеер YouTube — в кеш на диск: без этого JS-разбор подписи шёл ~25 с на каждый трек, с ним ~5
try:
    from yt_dlp.extractor.youtube.jsc._builtin.ejs import EJSBaseJCP
    EJSBaseJCP._ENABLE_PREPROCESSED_PLAYER_CACHE = True
except Exception:
    pass
idle = float(sys.argv[2])
# клиент «телевизор Samsung», как у свежего yt-dlp: обычный tv YouTube сломал («page needs to be reloaded»)
# initial_data (запрос next) нам не нужен — только ссылка на звук, это минус секунда на трек.
# player_params=2AMB: YouTube не вешает рекламу, и ссылка открыта сразу, а не через ~4 с. Параметр недокументирован;
# перестанет работать — останется ожидание по available_at или обычный путь ниже. Похожий adPlaybackContext.pyv
# не брать: он тоже без рекламы, но отдаёт только начало файла (дальше 403)
FAST = ['--extractor-args', 'youtube:player_client=tv_samsung;player_skip=webpage,configs,initial_data;player_params=2AMB']
cache = {}
def ydl(args, fast):
    k = (tuple(args), fast)
    if k not in cache:
        o = yt_dlp.parse_options(list(args) + (FAST if fast else []) + ['-f', 'bestaudio/best', '--no-playlist', '-q', '--no-warnings', '--no-progress']).ydl_opts
        o['logger'] = Quiet()
        cache[k] = yt_dlp.YoutubeDL(o)
    return cache[k]
class Quiet:
    def debug(self, m): pass
    info = warning = debug
    def error(self, m): pass
print('ready', flush=True)
while True:
    if not select.select([sys.stdin], [], [], idle)[0]:
        sys.exit(0)
    line = sys.stdin.readline()
    if not line:
        sys.exit(0)
    req = json.loads(line)
    got, errs, at = {}, {}, 0
    def take(i):
        # без Премиума YouTube вешает рекламу: ссылка откроется, лишь когда её можно пропустить (до этого 403)
        global at
        at = max(at, i.get('available_at') or 0)
        return i['url']
    for u in req['urls']:
        try:
            # music.youtube.com тянет ещё и клиент web_music — лишний запрос; звук тот же
            got[u] = take(ydl(req['args'], True).extract_info(u.replace('//music.youtube.com/', '//www.youtube.com/'), download=False))
        except Exception as e:
            if 'not a bot' in str(e) or 'Sign in to confirm' in str(e):
                errs[u] = str(e)
                continue
            try:
                got[u] = take(ydl(req['args'], False).extract_info(u, download=False))
            except Exception as e:
                errs[u] = str(e)
    if not req['urls']:
        ydl(req['args'], True)  # прогрев: куки и клиент заранее
    print(json.dumps({'got': got, 'errs': errs, 'at': at}), flush=True)
"#;

struct Worker {
    child: Child,
    stdin: ChildStdin,
    out: BufReader<ChildStdout>,
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

static FG: Mutex<Option<Worker>> = Mutex::new(None);
static BG: Mutex<Option<Worker>> = Mutex::new(None);
// номер последнего запроса play: пока ждали очереди, юзер ткнул другой трек — этот уже не нужен
static FG_SEQ: AtomicU64 = AtomicU64::new(0);

fn spawn(idle_secs: u32) -> Option<Worker> {
    let py = find_bin("python3").or_else(|| find_bin("python"))?;
    let ytdlp = find_bin("yt-dlp")?;
    let mut c = command(&py);
    c.arg("-c").arg(SCRIPT).arg(&ytdlp).arg(idle_secs.to_string()).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    #[cfg(target_os = "linux")]
    unsafe {
        use std::os::unix::process::CommandExt;
        c.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            Ok(())
        });
    }
    let mut child = c.spawn().ok()?;
    let stdin = child.stdin.take()?;
    let mut out = BufReader::new(child.stdout.take()?);
    let mut line = String::new();
    // не импортировался yt_dlp (например, yt-dlp — бинарь PyInstaller) — работаем по-старому
    if out.read_line(&mut line).ok()? == 0 || line.trim() != "ready" {
        return None;
    }
    Some(Worker { child, stdin, out })
}

/// (ссылки, ошибки) по каждому url. None — живой yt-dlp недоступен: зовите обычный yt-dlp
pub fn resolve(urls: &[String], args: &[String], bg: bool) -> Option<(HashMap<String, String>, HashMap<String, String>)> {
    let slot = if bg { &BG } else { &FG };
    let seq = (!bg && !urls.is_empty()).then(|| FG_SEQ.fetch_add(1, Ordering::SeqCst) + 1);
    // занят — ждём: живой отдаёт трек за ~2 с, а обычный запуск yt-dlp без кеша плеера — 10–20 с
    let mut w = slot.lock().unwrap_or_else(|e| e.into_inner());
    if seq.is_some_and(|s| s != FG_SEQ.load(Ordering::SeqCst)) {
        return Some(Default::default());
    }
    for _ in 0..2 {
        if w.is_none() {
            *w = spawn(if bg { 120 } else { 900 });
        }
        let worker = w.as_mut()?;
        let req = json!({"args": args, "urls": urls}).to_string() + "\n";
        let mut line = String::new();
        let ok = worker.stdin.write_all(req.as_bytes()).and_then(|_| worker.stdin.flush()).is_ok() && worker.out.read_line(&mut line).is_ok_and(|n| n > 0);
        match serde_json::from_str::<Value>(&line) {
            Ok(v) if ok => {
                let map = |k: &str| v[k].as_object().map(|m| m.iter().filter_map(|(u, s)| Some((u.clone(), s.as_str()?.to_string()))).collect()).unwrap_or_default();
                let (got, errs) = (map("got"), map("errs"));
                // ждём открытия ссылки уже без замка — следующий трек резолвится параллельно
                drop(w);
                let at = std::time::UNIX_EPOCH + Duration::from_secs_f64(v["at"].as_f64().unwrap_or(0.0));
                if let Ok(d) = at.duration_since(std::time::SystemTime::now()) {
                    wait_open(&got, d);
                }
                return Some((got, errs));
            }
            // процесс вышел по простою или упал — поднимаем заново и повторяем
            _ => *w = None,
        }
    }
    None
}

/// yt-dlp называет время открытия с запасом (замер: обещал 4 с, открылась через 2.2), поэтому как плеер
/// на телефоне — щупаем ссылку байтом каждые 300 мс, пока не перестанет отвечать 403
fn wait_open(got: &HashMap<String, String>, promised: Duration) {
    let end = Instant::now() + promised + Duration::from_secs(1);
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(5)).build();
    for s in got.values() {
        while Instant::now() < end && agent.get(&format!("{s}&range=0-0")).call().is_err() {
            std::thread::sleep(Duration::from_millis(300));
        }
    }
}

/// Поднять процесс заранее (при запуске плеера), чтобы первый трек не ждал Python и куки
pub fn warm(args: Vec<String>) {
    std::thread::spawn(move || {
        resolve(&[], &args, false);
    });
}

#[cfg(test)]
mod tests {
    /// Живой YouTube: два трека через живой yt-dlp, второй — уже без запуска Python. Не гонять в цикле
    #[test]
    #[ignore]
    fn resolves_fast() {
        let args = crate::util::ytdlp_args(Some("firefox"));
        for v in ["eZXKCiUMRlc", "kSUmxeLL-ak"] {
            let t = std::time::Instant::now();
            let u = format!("https://music.youtube.com/watch?v={v}");
            let (got, errs) = super::resolve(&[u.clone()], &args, false).expect("живой yt-dlp не поднялся");
            println!("{v}: {:.1} с {:?}", t.elapsed().as_secs_f32(), errs);
            // ссылка должна реально отдавать звук, а не 403 (реклама без Премиума держит её закрытой)
            let s = got.get(&u).expect("нет ссылки");
            let r = ureq::get(&format!("{s}&range=0-1000")).call();
            assert!(r.is_ok_and(|r| r.status() == 200), "googlevideo не отдал звук");
            // и середину файла: бывают ссылки, которые отдают только начало
            let r = ureq::get(&format!("{s}&range=1000000-1001000")).call();
            assert!(r.is_ok_and(|r| r.status() == 200), "googlevideo не отдал середину файла");
        }
    }
}
