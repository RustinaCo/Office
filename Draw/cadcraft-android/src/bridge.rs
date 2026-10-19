//! JNI calls into `AndroidBridge.java`: IME overlay, clipboard, system bars,
//! system file picker (SAF via transparent helper activity) and inbound SEND
//! shares.
//!
//! One attach scope per public call (never nested: detaching an inner scope
//! would strand the outer one). Only the activity handle is cached (global);
//! the bridge class is resolved per call through the activity's class loader
//! (`find_class` cannot see app classes from an attached thread; going through
//! `getClass()` first lands on the framework `NativeActivity` class whose
//! loader is the boot loader).

use std::sync::OnceLock;

use jni::objects::{Global, JByteArray, JClass, JObject, JString, JValue};
use jni::{Env, jni_sig, jni_str};
use winit::platform::android::activity::AndroidApp;

struct Ctx {
    vm: jni::JavaVM,
    activity: Global<JObject<'static>>,
}

static CTX: OnceLock<Ctx> = OnceLock::new();

/// Cache the VM + activity handle. Runs once from `android_main`.
pub fn init(app: &AndroidApp) {
    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr() as *mut jni::sys::JavaVM) };
    let activity_raw = app.activity_as_ptr() as jni::sys::jobject;
    let activity: Option<Global<JObject<'static>>> = vm
        .attach_current_thread(|env: &mut Env| {
            let activity = unsafe { JObject::from_raw(env, activity_raw) };
            let g = env.new_global_ref(&activity).ok();
            if g.is_none() {
                env.exception_clear();
            }
            Ok::<_, jni::errors::Error>(g)
        })
        .ok()
        .flatten();
    match activity {
        Some(activity) => {
            CTX.set(Ctx { vm, activity }).ok();
            with_class(|env, cls| {
                let act = CTX.get()?.activity.as_obj();
                env.call_static_method(
                    cls,
                    jni_str!("ensure"),
                    jni_sig!("(Landroid/app/Activity;)V"),
                    &[JValue::from(act)],
                )
                .ok()?;
                Some(())
            });
        }
        None => eprintln!("cadcraft-android: bridge init failed (no activity global)"),
    }
}

/// One attach scope + resolved bridge class; describes real Java exceptions,
/// stays quiet for null (no-result-yet) outcomes.
fn with_class<T>(f: impl FnOnce(&mut Env, &JClass) -> Option<T>) -> Option<T> {
    let c = CTX.get()?;
    let out: Result<Option<T>, jni::errors::Error> = c.vm.attach_current_thread(|env: &mut Env| {
        let v: Option<T> = (|| {
            let act = c.activity.as_obj();
            // getClassLoader() straight on the Activity (its base context's
            // app loader).
            let loader = env
                .call_method(
                    act,
                    jni_str!("getClassLoader"),
                    jni_sig!("()Ljava/lang/ClassLoader;"),
                    &[],
                )
                .ok()?
                .l()
                .ok()?;
            let name = JString::from_str(env, "net.craftsuite.cadcraft.AndroidBridge").ok()?;
            let bridge_obj = env
                .call_method(
                    &loader,
                    jni_str!("loadClass"),
                    jni_sig!("(Ljava/lang/String;)Ljava/lang/Class;"),
                    &[JValue::from(&name)],
                )
                .ok()?
                .l()
                .ok()?;
            let cls: JClass = env.new_cast_local_ref::<JClass>(&bridge_obj).ok()?;
            f(env, &cls)
        })();
        if v.is_none() {
            // Null results (e.g. no picker result yet) are normal idle
            // traffic; only describe real Java exceptions.
            env.exception_describe();
            env.exception_clear();
        }
        Ok(v)
    });
    out.ok().flatten()
}

fn jstring_result(env: &mut Env, call: Option<JObject>) -> Option<String> {
    let o = call?;
    if o.as_raw().is_null() {
        return None;
    }
    let js: JString = env.new_cast_local_ref::<JString>(&o).ok()?;
    js.try_to_string(env).ok()
}

// ---- IME overlay ------------------------------------------------------------

/// Focus the transparent `EditText` overlay on `text` and raise the keyboard.
pub fn ime_focus(text: &str) {
    with_class(|env, cls| {
        let t = JString::from_str(env, text).ok()?;
        env.call_static_method(
            cls,
            jni_str!("imeFocus"),
            jni_sig!("(Ljava/lang/String;)V"),
            &[JValue::from(&t)],
        )
        .ok()?;
        Some(())
    });
}

/// The overlay's current text (updated on the UI thread as the user types).
pub fn ime_poll() -> String {
    with_class(|env, cls| {
        let o = env
            .call_static_method(
                cls,
                jni_str!("imePoll"),
                jni_sig!("()Ljava/lang/String;"),
                &[],
            )
            .ok()?
            .l()
            .ok()?;
        jstring_result(env, Some(o))
    })
    .unwrap_or_default()
}

/// Whether the keyboard's Done action was pressed since the last call.
pub fn ime_take_done() -> bool {
    with_class(|env, cls| {
        env.call_static_method(cls, jni_str!("imeTakeDone"), jni_sig!("()Z"), &[])
            .ok()?
            .z()
            .ok()
    })
    .unwrap_or(false)
}

/// Re-assert the keyboard while the overlay keeps focus (the first request can
/// land before the view is served and be ignored).
pub fn ime_show() {
    with_class(|env, cls| {
        env.call_static_method(cls, jni_str!("imeShow"), jni_sig!("()V"), &[])
            .ok()?;
        Some(())
    });
}

pub fn ime_hide() {
    with_class(|env, cls| {
        env.call_static_method(cls, jni_str!("imeHide"), jni_sig!("()V"), &[])
            .ok()?;
        Some(())
    });
}

/// Replace the overlay's text without touching focus or the keyboard (used after Done
/// submits a line: the next command starts empty on the same keyboard).
pub fn ime_set_text(text: &str) {
    with_class(|env, cls| {
        let t = JString::from_str(env, text).ok()?;
        env.call_static_method(
            cls,
            jni_str!("imeSetText"),
            jni_sig!("(Ljava/lang/String;)V"),
            &[JValue::from(&t)],
        )
        .ok()?;
        Some(())
    });
}

// ---- clipboard ----------------------------------------------------------------

pub fn put_text(s: &str) {
    with_class(|env, cls| {
        let t = JString::from_str(env, s).ok()?;
        env.call_static_method(
            cls,
            jni_str!("putText"),
            jni_sig!("(Ljava/lang/String;)V"),
            &[JValue::from(&t)],
        )
        .ok()?;
        Some(())
    });
}

/// The clipboard's text, for paste.
///
/// Ported with the rest of the bridge but not wired: egui turns a paste into
/// `Event::Paste(text)` only where the host reads the clipboard, and eframe's
/// Android backend does not. A keyboard overlay would have to route Android's
/// paste action here (see the gap list in the crate docs).
#[allow(dead_code)]
pub fn get_text() -> Option<String> {
    with_class(|env, cls| {
        let o = env
            .call_static_method(
                cls,
                jni_str!("getText"),
                jni_sig!("()Ljava/lang/String;"),
                &[],
            )
            .ok()?
            .l()
            .ok()?;
        jstring_result(env, Some(o))
    })
}

// ---- system bars --------------------------------------------------------------

/// Solid status/navigation bars with theme-contrasting icons (white on black
/// in dark mode, dark on white in light mode). Called when the app theme
/// changes; the bars are never hidden.
pub fn style_bars(dark: bool) {
    with_class(|env, cls| {
        env.call_static_method(
            cls,
            jni_str!("styleBars"),
            jni_sig!("(Z)V"),
            &[JValue::Bool(dark)],
        )
        .ok()?;
        Some(())
    });
}

/// Live system-bar insets in dp (top, bottom), for the safe-area spacers.
/// Returns None before the decor view is attached.
pub fn system_insets() -> Option<(f32, f32)> {
    with_class(|env, cls| {
        let s: String = {
            let o = env
                .call_static_method(
                    cls,
                    jni_str!("systemInsetsDp"),
                    jni_sig!("()Ljava/lang/String;"),
                    &[],
                )
                .ok()?
                .l()
                .ok()?;
            jstring_result(env, Some(o))?
        };
        let (t, b) = s.split_once(',')?;
        Some((t.parse().ok()?, b.parse().ok()?))
    })
}

// ---- locale -------------------------------------------------------------------

/// The device's language tag (`Locale.getDefault().toLanguageTag()`, e.g.
/// `fa-IR`), for CADCraft's interface language.
pub fn locale_tag() -> Option<String> {
    with_class(|env, cls| {
        let o = env
            .call_static_method(
                cls,
                jni_str!("localeTag"),
                jni_sig!("()Ljava/lang/String;"),
                &[],
            )
            .ok()?
            .l()
            .ok()?;
        jstring_result(env, Some(o))
    })
}

// ---- system file picker (SAF via transparent helper activity) ---------------

pub fn launch_open(mimes_csv: &str) {
    with_class(|env, cls| {
        let m = JString::from_str(env, mimes_csv).ok()?;
        env.call_static_method(
            cls,
            jni_str!("launchOpen"),
            jni_sig!("(Ljava/lang/String;)V"),
            &[JValue::from(&m)],
        )
        .ok()?;
        Some(())
    });
}

pub fn launch_create(mime: &str, title: &str) {
    with_class(|env, cls| {
        let m = JString::from_str(env, mime).ok()?;
        let t = JString::from_str(env, title).ok()?;
        env.call_static_method(
            cls,
            jni_str!("launchCreate"),
            jni_sig!("(Ljava/lang/String;Ljava/lang/String;)V"),
            &[JValue::from(&m), JValue::from(&t)],
        )
        .ok()?;
        Some(())
    });
}

/// A picked file (display name, bytes), if the open flow completed.
pub fn take_open() -> Option<(String, Vec<u8>)> {
    with_class(|env, cls| {
        let name = {
            let o = env
                .call_static_method(
                    cls,
                    jni_str!("takeOpenName"),
                    jni_sig!("()Ljava/lang/String;"),
                    &[],
                )
                .ok()?
                .l()
                .ok()?;
            jstring_result(env, Some(o))?
        };
        let bytes = {
            let o = env
                .call_static_method(cls, jni_str!("takeOpenBytes"), jni_sig!("()[B"), &[])
                .ok()?
                .l()
                .ok()?;
            if o.as_raw().is_null() {
                return None;
            }
            let arr: JByteArray = env.new_cast_local_ref::<JByteArray>(&o).ok()?;
            env.convert_byte_array(&arr).ok()?
        };
        Some((name, bytes))
    })
}

/// The user-chosen save destination (content URI), if the create flow completed.
pub fn take_create_uri() -> Option<String> {
    with_class(|env, cls| {
        let o = env
            .call_static_method(
                cls,
                jni_str!("takeCreateUri"),
                jni_sig!("()Ljava/lang/String;"),
                &[],
            )
            .ok()?
            .l()
            .ok()?;
        jstring_result(env, Some(o))
    })
}

/// Whether the user cancelled the picker (clears any pending file op).
pub fn take_cancelled() -> bool {
    with_class(|env, cls| {
        env.call_static_method(cls, jni_str!("takeCancelled"), jni_sig!("()Z"), &[])
            .ok()?
            .z()
            .ok()
    })
    .unwrap_or(false)
}

pub fn write_uri(uri: &str, data: &[u8]) -> bool {
    with_class(|env, cls| {
        let u = JString::from_str(env, uri).ok()?;
        let b = env.byte_array_from_slice(data).ok()?;
        env.call_static_method(
            cls,
            jni_str!("writeUri"),
            jni_sig!("(Ljava/lang/String;[B)Z"),
            &[JValue::from(&u), JValue::from(&b)],
        )
        .ok()?
        .z()
        .ok()
    })
    .unwrap_or(false)
}

// ---- inbound SEND share ---------------------------------------------------------

/// A file shared *to* Rustina Draw from another app (name, bytes), if any.
/// The Java side latches the intent extras on first read.
pub fn inbound_share() -> Option<(String, Vec<u8>)> {
    with_class(|env, cls| {
        let act = CTX.get()?.activity.as_obj();
        let name = {
            let o = env
                .call_static_method(
                    cls,
                    jni_str!("inboundName"),
                    jni_sig!("(Landroid/app/Activity;)Ljava/lang/String;"),
                    &[JValue::from(act)],
                )
                .ok()?
                .l()
                .ok()?;
            jstring_result(env, Some(o))?
        };
        let bytes = {
            let o = env
                .call_static_method(
                    cls,
                    jni_str!("inboundBytes"),
                    jni_sig!("(Landroid/app/Activity;)[B"),
                    &[JValue::from(act)],
                )
                .ok()?
                .l()
                .ok()?;
            if o.as_raw().is_null() {
                return None;
            }
            let arr: JByteArray = env.new_cast_local_ref::<JByteArray>(&o).ok()?;
            env.convert_byte_array(&arr).ok()?
        };
        Some((name, bytes))
    })
}
