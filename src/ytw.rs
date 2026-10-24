//! yt-dlp, который живёт в фоне: Python, сам yt-dlp, куки Firefox и плеер YouTube грузятся один раз,
//! а не на каждый трек. Плюс быстрый путь — только tv-клиент и без загрузки страницы (меньше запросов
//! к YouTube); не вышло — тот же трек обычным путём. Замер на i3: ~10.4 с запуском yt-dlp на трек
//! против ~4 с здесь. Два процесса: для play (обычный приоритет) и для предзагрузки (nice 19, его
//! запускает поток предзагрузки). Без дела процесс сам выходит, чтобы не держать ~80 МБ памяти зря

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Stdio};
use std::sync::Mutex;

use serde_json::{json, Value};

use crate::util::{command, find_bin};

const SCRIPT: &str = r#"
import sys, json, select
sys.path.insert(0, sys.argv[1])  # yt-dlp из ~/.local/bin — zip-архив, импортируется прямо из него
import yt_dlp
idle = float(sys.argv[2])
FAST = ['--extractor-args', 'youtube:player_client=tv;player_skip=webpage,configs']
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
    got, errs = {}, {}
    for u in req['urls']:
        try:
            got[u] = ydl(req['args'], True).extract_info(u, download=False)['url']
        except Exception as e:
            if 'not a bot' in str(e) or 'Sign in to confirm' in str(e):
                errs[u] = str(e)
                continue
            try:
                got[u] = ydl(req['args'], False).extract_info(u, download=False)['url']
            except Exception as e:
                errs[u] = str(e)
    if not req['urls']:
        ydl(req['args'], True)  # прогрев: куки и клиент заранее
    print(json.dumps({'got': got, 'errs': errs}), flush=True)
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

/// (ссылки, ошибки) по каждому url. None — живой yt-dlp недоступен или занят: зовите обычный yt-dlp
pub fn resolve(urls: &[String], args: &[String], bg: bool) -> Option<(HashMap<String, String>, HashMap<String, String>)> {
    let slot = if bg { &BG } else { &FG };
    // занят другим треком (быстро листают) — не ждём его, пусть этот идёт обычным путём
    let mut w = slot.try_lock().ok()?;
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
                return Some((map("got"), map("errs")));
            }
            // процесс вышел по простою или упал — поднимаем заново и повторяем
            _ => *w = None,
        }
    }
    None
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
            assert!(got.get(&u).is_some_and(|s| s.starts_with("https://")));
        }
    }
}
