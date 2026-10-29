//! JNI: app.blackhole.Core.call(String method, String args): String

use jni::objects::{JClass, JString};
use jni::sys::jstring;
use jni::JNIEnv;

#[no_mangle]
pub extern "system" fn Java_app_blackhole_Core_call<'l>(mut env: JNIEnv<'l>, _: JClass<'l>, method: JString<'l>, args: JString<'l>) -> jstring {
    let get = |env: &mut JNIEnv<'l>, s: &JString<'l>| -> String { env.get_string(s).map(Into::into).unwrap_or_default() };
    let m = get(&mut env, &method);
    let a = get(&mut env, &args);
    let out = crate::call(&m, &a);
    env.new_string(out).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut())
}
