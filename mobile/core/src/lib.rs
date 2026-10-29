//! Ядро BlackHole для Android. Общие модули — те же файлы, что у плеера на ПК (../../src), здесь только
//! тонкий слой «метод + JSON → JSON» для Kotlin. Один вход через JNI: Core.call(method, args).
//! Ответ: {"ok": ...} или {"err": "код или текст"}. Всё блокирующее — Kotlin зовёт из фонового потока.

#![allow(dead_code)]

#[path = "../../../src/model.rs"]
pub mod model;
#[path = "../../../src/util.rs"]
pub mod util;
#[path = "../../../src/ytw.rs"]
pub mod ytw;
#[path = "../../../src/ytmusic.rs"]
pub mod ytmusic;
#[path = "../../../src/soundcloud.rs"]
pub mod soundcloud;
#[path = "../../../src/spotify.rs"]
pub mod spotify;
#[path = "../../../src/apple.rs"]
pub mod apple;
#[path = "../../../src/yandex.rs"]
pub mod yandex;
#[path = "../../../src/lyrics.rs"]
pub mod lyrics;
#[path = "../../../src/spicy.rs"]
pub mod spicy;
#[path = "../../../src/stats.rs"]
pub mod stats;
#[path = "../../../src/home.rs"]
pub mod home;
#[path = "../../../src/cloud.rs"]
pub mod cloud;
#[path = "../../../src/qr.rs"]
pub mod qr;
#[path = "../../../src/connect.rs"]
pub mod connect;

mod api;
#[cfg(target_os = "android")]
mod jni_entry;

pub use api::call;
