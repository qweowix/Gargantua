//! Разовая живая проверка JSON-входа ядра (по одному запросу): cargo run --release --example smoke
fn main() {
    let dir = std::env::temp_dir().join("bh-smoke");
    let a = format!(r#"{{"files":"{0}","cache":"{0}"}}"#, dir.display());
    println!("init: {}", blackhole_core::call("init", &a));
    println!("account: {}", blackhole_core::call("account", "{}"));
    let s = blackhole_core::call("search", r#"{"q":"tv girl cigarettes out the window","src":"yt"}"#);
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    let first = &v["ok"][0];
    println!("search: {} треков, первый: {} — {}", v["ok"].as_array().map_or(0, |a| a.len()), first["artist"], first["title"]);
    let t = serde_json::json!({ "track": first }).to_string();
    println!("resolve: {}", blackhole_core::call("resolve", &t));
    let l: serde_json::Value = serde_json::from_str(&blackhole_core::call("lyrics", &t)).unwrap();
    println!("lyrics: {} строк, {}, {}", l["ok"]["lines"].as_array().map_or(0, |a| a.len()), l["ok"]["kind"], l["ok"]["credit"]);
}
