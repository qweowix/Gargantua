//! Вселенная, мосты, Music DNA, вкус и комнаты для телефона — тот же код, что на ПК (graph, universe,
//! explore, dna, taste, rooms), здесь только «метод + JSON → JSON». Подписи на языке интерфейса делает Kotlin.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::{json, Value};

use crate::graph::{Graph, Input};
use crate::model::{Playlist, Track};
use crate::universe::Universe;
use crate::{cloud, dna, explore, musicbrainz, rooms, taste, universe};

fn s<'a>(a: &'a Value, k: &str) -> &'a str {
    a[k].as_str().unwrap_or("")
}

fn uni() -> &'static Mutex<Option<Universe>> {
    static U: OnceLock<Mutex<Option<Universe>>> = OnceLock::new();
    U.get_or_init(|| Mutex::new(None))
}

/// Граф из того же, что на ПК: статистика аккаунта, плейлисты (их держит Kotlin), DNA, MusicBrainz, «похожие»
fn input(a: &Value) -> Input {
    let st = crate::api::stats_all();
    let pls: Vec<Playlist> = serde_json::from_value(a["playlists"].clone()).unwrap_or_default();
    let mut seen = HashSet::new();
    let mut tracks = Vec::new();
    for s in st.tracks.values() {
        if seen.insert(s.track.key()) {
            tracks.push((s.track.clone(), s.ms, s.plays));
        }
    }
    for p in &pls {
        for t in &p.tracks {
            if seen.insert(t.key()) {
                tracks.push((t.clone(), 0, 0));
            }
        }
    }
    let (mb_artists, mb_recs) = musicbrainz::snapshot();
    Input { tracks, playlists: pls.iter().map(|p| p.tracks.iter().map(|t| t.key()).collect()).collect(), dna: dna::all(), mb_artists, mb_recs, yt_similar: universe::similar_cache() }
}

fn kind_num(k: crate::graph::Kind) -> u8 {
    use crate::graph::Kind::*;
    match k {
        Track => 0,
        Artist => 1,
        Album => 2,
        Genre => 3,
        Person => 4,
        Label => 5,
    }
}

// ── комнаты: изменения из потока событий будят длинный опрос Kotlin ──

static ROOM_VER: AtomicU64 = AtomicU64::new(0);

fn room_cv() -> &'static (Mutex<()>, Condvar) {
    static C: OnceLock<(Mutex<()>, Condvar)> = OnceLock::new();
    C.get_or_init(|| (Mutex::new(()), Condvar::new()))
}

fn room_bump() {
    ROOM_VER.fetch_add(1, Ordering::SeqCst);
    room_cv().1.notify_all();
}

pub fn run(m: &str, a: &Value) -> Option<Result<Value>> {
    Some(run_inner(m, a))
}

fn run_inner(m: &str, a: &Value) -> Result<Value> {
    Ok(match m {
        // ── вселенная ──
        "uni_build" => {
            let u = Universe::new(Graph::build(&input(a)));
            let nodes: Vec<Value> = u
                .rank
                .iter()
                .map(|&i| {
                    let n = &u.g.nodes[i];
                    json!({"i": i, "n": n.name, "k": kind_num(n.kind), "x": u.pos[i].0, "y": u.pos[i].1, "r": u.radius[i], "c": u.color[i], "l": n.listened})
                })
                .collect();
            let edges: Vec<Value> = u.g.edges.iter().map(|e| json!([e.a, e.b, match e.rel.origin() { crate::graph::Origin::Confirmed => 0, crate::graph::Origin::External => 1, _ => 2 }, e.w])).collect();
            let (cx, cy, w, h) = u.bounds();
            let dna_n = u.g.nodes.iter().filter(|n| n.track.as_ref().is_some_and(|t| dna::get(&t.key()).is_some())).count();
            let out = json!({"nodes": nodes, "edges": edges, "bounds": [cx, cy, w, h], "dna": dna_n});
            *uni().lock().unwrap() = Some(u);
            out
        }
        "uni_node" => {
            let g = uni().lock().unwrap();
            let u = g.as_ref().context("нет вселенной")?;
            let i = a["i"].as_u64().unwrap_or(u64::MAX) as usize;
            let n = u.g.nodes.get(i).context("нет узла")?;
            let rels: Vec<Value> = explore::relations(&u.g, i, 300)
                .into_iter()
                .map(|e| {
                    let ed = &u.g.edges[e];
                    let o = u.g.other(e, i);
                    json!({"id": o, "name": u.g.nodes[o].name, "kind": kind_num(u.g.nodes[o].kind), "rel": ed.rel, "note": ed.note, "origin": ed.rel.origin()})
                })
                .collect();
            let d = explore::dna_of(&u.g, i);
            json!({
                "name": n.name, "kind": kind_num(n.kind), "listened": n.listened, "plays": n.plays, "track": n.track,
                "dna": d.as_ref().map(|(d, k)| json!({"bpm": d.bpm, "key": d.key_name(), "n": k})),
                "rels": rels, "tracks": explore::tracks_of(&u.g, i).into_iter().take(50).collect::<Vec<_>>(),
            })
        }
        "uni_search" => {
            let g = uni().lock().unwrap();
            let u = g.as_ref().context("нет вселенной")?;
            json!(u.search(s(a, "q"), 8))
        }
        "uni_bridge" => {
            let g = uni().lock().unwrap();
            let u = g.as_ref().context("нет вселенной")?;
            let (from, to) = (a["a"].as_u64().unwrap_or(0) as usize, a["b"].as_u64().unwrap_or(0) as usize);
            let groups: [bool; 4] = serde_json::from_value(a["groups"].clone()).unwrap_or([true; 4]);
            let depth = a["depth"].as_u64().unwrap_or(5) as usize;
            if from >= u.g.nodes.len() || to >= u.g.nodes.len() || from == to {
                return Ok(json!([]));
            }
            json!(u
                .g
                .bridges(from, to, depth, &explore::bridge_rels(groups), 3)
                .iter()
                .map(|p| {
                    let st = explore::steps(&u.g, from, p);
                    let facts = st.iter().filter(|s| s.origin == Some(crate::graph::Origin::Confirmed)).count();
                    json!({"steps": st, "facts": facts, "edges": p})
                })
                .collect::<Vec<_>>())
        }
        "uni_dna" => {
            let g = uni().lock().unwrap();
            let u = g.as_ref().context("нет вселенной")?;
            let i = a["i"].as_u64().unwrap_or(u64::MAX) as usize;
            let (d, n) = explore::dna_of(&u.g, i).context("нет DNA")?;
            let cmp = a["cmp"].as_u64().map(|c| c as usize).filter(|&c| c != i).and_then(|c| explore::dna_of(&u.g, c).map(|x| (c, x.0)));
            let (similar, far) = explore::neighbors(&u.g, i, &d);
            let row = |p: &explore::Pick| {
                let nd = &u.g.nodes[p.node];
                json!({"id": p.node, "name": nd.name, "sub": nd.track.as_ref().map(|t| t.artist.clone()), "sim": p.sim, "why": p.why})
            };
            json!({
                "dna": d, "n": n, "total": explore::tracks_of(&u.g, i).len(), "radar": explore::radar(&d),
                "cmp": cmp.as_ref().map(|(c, cd)| json!({"name": u.g.nodes[*c].name, "radar": explore::radar(cd), "sim": dna::compare(&d, cd)})),
                "similar": similar.iter().map(row).collect::<Vec<_>>(), "far": far.iter().map(row).collect::<Vec<_>>(),
            })
        }
        // «Подтянуть связи»: долго (MusicBrainz — запрос в секунду), Kotlin зовёт из фона
        "uni_fetch" => {
            let (artists, recs): (Vec<String>, Vec<Track>) = {
                let g = uni().lock().unwrap();
                let u = g.as_ref().context("нет вселенной")?;
                let mut arts: Vec<(u64, String)> = u.g.nodes.iter().filter(|n| n.kind == crate::graph::Kind::Artist).map(|n| (n.listened, n.name.clone())).collect();
                arts.sort_by(|a, b| b.0.cmp(&a.0));
                let mut recs: Vec<(u64, Track)> = u.g.nodes.iter().filter_map(|n| Some((n.listened, n.track.clone()?))).collect();
                recs.sort_by(|a, b| b.0.cmp(&a.0));
                (arts.into_iter().take(80).map(|a| a.1).collect(), recs.into_iter().take(400).map(|r| r.1).collect())
            };
            universe::fetch_similar(&artists[..artists.len().min(25)], &|| {});
            musicbrainz::fetch_blocking(&artists, &recs, &|| {});
            Value::Null
        }

        // ── Music DNA на телефоне: Kotlin раскодирует скачанный файл в моно f32le 22 050 Гц ──
        "dna_raw" => {
            let raw = std::fs::read(s(a, "path"))?;
            let pcm: Vec<f32> = raw.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
            let d = dna::analyze(&pcm)?;
            let out = json!({"bpm": d.bpm, "key": d.key_name()});
            dna::put(s(a, "key"), d);
            out
        }
        "dna_has" => json!(dna::get(s(a, "key")).is_some()),
        // отпечатки, посчитанные на других устройствах аккаунта (ПК разбирает кэш), — к себе
        "dna_sync" => json!(dna::sync()?),

        // ── вкус ──
        "taste" => {
            let inp = input(a);
            let g = Graph::build(&inp);
            let st = crate::api::stats_all();
            let (from, to) = taste::period(a["period"].as_i64().unwrap_or(0) as i32);
            let fp = taste::build(&st, &g, &crate::journal::history(3000), from, to);
            let (stars, paths) = taste::stars(&fp, &g);
            let acc = cloud::account();
            json!({"fp": fp, "stars": stars, "lines": paths[0], "arcs": paths[1], "code": taste::my_code(&acc.uid), "shows": taste::shows(), "published": taste::published()})
        }
        "taste_publish" => {
            let inp = input(a);
            let g = Graph::build(&inp);
            let st = crate::api::stats_all();
            let fp = taste::build(&st, &g, &[], 0, 0);
            let acc = cloud::account();
            let shows: [bool; 5] = serde_json::from_value(a["shows"].clone()).unwrap_or([true; 5]);
            taste::set_shows(shows);
            let p = taste::public(&fp, taste::top_tracks(&st, 40), shows, acc.email.split('@').next().unwrap_or("?"));
            let code = taste::my_code(&acc.uid);
            cloud::put_profile(&cloud::cfg().context("NOT_CONFIGURED")?, &code, &serde_json::to_string(&p)?)?;
            taste::set_published(true);
            json!(code)
        }
        "taste_unpublish" => {
            taste::set_published(false);
            cloud::delete_profile(&cloud::cfg().context("NOT_CONFIGURED")?, &taste::my_code(&cloud::account().uid))?;
            Value::Null
        }
        "taste_compare" => {
            let raw = cloud::get_profile(&cloud::cfg().context("NOT_CONFIGURED")?, &s(a, "code").trim().to_uppercase())?;
            let them: taste::Public = serde_json::from_str(&raw)?;
            let inp = input(a);
            let lib: HashSet<String> = inp.tracks.iter().map(|t| t.0.key()).collect();
            let g = Graph::build(&inp);
            let st = crate::api::stats_all();
            let me = taste::build(&st, &g, &[], 0, 0);
            json!({"name": them.name, "compat": taste::compare(&me, &them.fp), "discover": taste::discover(&me, &them, &lib, &g)})
        }

        // ── комнаты ──
        "rooms_create" => json!(rooms::create(s(a, "name"), a["public"].as_bool().unwrap_or(false), a["queue_all"].as_bool().unwrap_or(true), a["control_all"].as_bool().unwrap_or(false))?),
        "rooms_join" => {
            rooms::join(s(a, "code"), room_bump)?;
            room_bump();
            Value::Null
        }
        "rooms_leave" => {
            rooms::leave();
            room_bump();
            Value::Null
        }
        // длинный опрос: ждёт изменений после версии since, отдаёт комнату целиком и серверное время
        "rooms_wait" => {
            let since = a["since"].as_u64().unwrap_or(0);
            let (lock, cv) = room_cv();
            let g = lock.lock().unwrap();
            let _ = cv.wait_timeout_while(g, Duration::from_millis(a["ms"].as_u64().unwrap_or(20_000)), |_| ROOM_VER.load(Ordering::SeqCst) == since).unwrap();
            json!({"ver": ROOM_VER.load(Ordering::SeqCst), "now": crate::connect::now(), "me": rooms::uid(), "room": rooms::room()})
        }
        "rooms_state" => {
            let t: Option<Track> = serde_json::from_value(a["track"].clone()).ok();
            rooms::set_state(t.as_ref(), a["pos"].as_f64().unwrap_or(0.0), a["playing"].as_bool().unwrap_or(false), a["seq"].as_u64().unwrap_or(0))?;
            Value::Null
        }
        "rooms_beat" => {
            rooms::beat(a["pos"].as_f64().unwrap_or(0.0), a["playing"].as_bool().unwrap_or(false), s(a, "key"))?;
            Value::Null
        }
        "rooms_enqueue" => {
            rooms::enqueue(&serde_json::from_value(a["track"].clone())?)?;
            Value::Null
        }
        "rooms_unqueue" => {
            rooms::unqueue(s(a, "id"))?;
            Value::Null
        }
        "rooms_requeue" => {
            rooms::requeue(s(a, "id"), a["to"].as_u64().unwrap_or(0) as usize)?;
            Value::Null
        }
        "rooms_vote" => {
            rooms::vote(s(a, "id"), a["on"].as_bool().unwrap_or(true))?;
            Value::Null
        }
        "rooms_say" => {
            rooms::say(s(a, "text"), s(a, "emoji"), s(a, "key"))?;
            Value::Null
        }
        "rooms_meta" => {
            rooms::set_meta(a["public"].as_bool().unwrap_or(false), a["queue_all"].as_bool().unwrap_or(true), a["control_all"].as_bool().unwrap_or(false))?;
            Value::Null
        }
        "rooms_kick" => {
            rooms::kick(s(a, "uid"))?;
            Value::Null
        }
        "rooms_public" => json!(rooms::public_rooms()?.into_iter().map(|(c, n, o)| json!({"code": c, "name": n, "owner": o})).collect::<Vec<_>>()),
        "rooms_nudge" => json!(rooms::nudge(a["drift"].as_f64().unwrap_or(0.0))),
        "taste_line" => {
            let inp = input(a);
            let g = Graph::build(&inp);
            let fp = taste::build(&crate::api::stats_all(), &g, &[], 0, 0);
            json!(taste::one_liner(&fp))
        }
        _ => anyhow::bail!("нет метода {m}"),
    })
}
