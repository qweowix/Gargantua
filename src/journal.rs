//! История прослушиваний и заметки к моментам треков. История — в папке настроек (history.json),
//! заметки — в кеше (notes.json), как просили: это пометки для себя, а не данные аккаунта.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::model::{Store, Track};

const KEEP: usize = 3000;
static LOCK: Mutex<()> = Mutex::new(());

fn history_path() -> PathBuf {
    Store::dir().join("history.json")
}

fn notes_path() -> PathBuf {
    dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join("BlackHole").join("notes.json")
}

fn read<T: for<'a> Deserialize<'a> + Default>(p: &PathBuf) -> T {
    std::fs::read(p).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn write<T: Serialize>(p: &PathBuf, v: &T) {
    let _ = std::fs::create_dir_all(p.parent().unwrap_or(p));
    let tmp = p.with_extension("tmp");
    if serde_json::to_vec(v).ok().and_then(|b| std::fs::write(&tmp, b).ok()).is_some() {
        let _ = std::fs::rename(tmp, p);
    }
}

/// Трек засчитан как прослушанный — в историю (время — мс от эпохи). Хранятся последние 3000
pub fn record(t: &Track) {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut h: Vec<(i64, Track)> = read(&history_path());
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64);
    h.push((now, t.clone()));
    if h.len() > KEEP {
        h.drain(..h.len() - KEEP);
    }
    write(&history_path(), &h);
}

/// История, свежие первыми
pub fn history(limit: usize) -> Vec<(i64, Track)> {
    let mut h: Vec<(i64, Track)> = read(&history_path());
    h.reverse();
    h.truncate(limit);
    h
}

#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Debug)]
pub struct Note {
    /// секунда трека
    pub pos: f64,
    pub text: String,
}

/// Заметки трека по порядку времени
pub fn notes(key: &str) -> Vec<Note> {
    read::<HashMap<String, Vec<Note>>>(&notes_path()).remove(key).unwrap_or_default()
}

pub fn note_add(key: &str, pos: f64, text: &str) -> Vec<Note> {
    edit(key, |v| {
        v.push(Note { pos, text: text.trim().to_string() });
        v.sort_by(|a, b| a.pos.total_cmp(&b.pos));
    })
}

pub fn note_remove(key: &str, i: usize) -> Vec<Note> {
    edit(key, |v| {
        if i < v.len() {
            v.remove(i);
        }
    })
}

fn edit(key: &str, f: impl FnOnce(&mut Vec<Note>)) -> Vec<Note> {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut all: HashMap<String, Vec<Note>> = read(&notes_path());
    let v = all.entry(key.to_string()).or_default();
    f(v);
    let out = v.clone();
    if out.is_empty() {
        all.remove(key);
    }
    write(&notes_path(), &all);
    out
}
