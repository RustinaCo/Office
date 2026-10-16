//! Android system-bar insets in dp, read once at startup over JNI.
//!
//! winit 0.30 has no safe-area API on Android, so we ask the platform directly:
//! `decorView.rootWindowInsets` (API 30+) with a `status_bar_height` /
//! `navigation_bar_height` dimen fallback (API 29). Failures also fall back to
//! conservative dp constants — a slightly tall spacer beats content under the
//! status bar.

use std::sync::atomic::{AtomicU32, Ordering};

use jni::objects::{JObject, JValue};
use jni::sys::jint;
use jni::{Env, jni_sig, jni_str};
use winit::platform::android::activity::AndroidApp;

const FALLBACK_TOP_DP: f32 = 28.0;
const FALLBACK_BOTTOM_DP: f32 = 48.0;

static TOP_BITS: AtomicU32 = AtomicU32::new(FALLBACK_TOP_DP.to_bits());
static BOTTOM_BITS: AtomicU32 = AtomicU32::new(FALLBACK_BOTTOM_DP.to_bits());

fn store(top_dp: f32, bottom_dp: f32) {
    TOP_BITS.store(top_dp.to_bits(), Ordering::Relaxed);
    BOTTOM_BITS.store(bottom_dp.to_bits(), Ordering::Relaxed);
}

/// Current insets in dp (== egui points once eframe applies the device scale).
pub fn current() -> (f32, f32) {
    (
        f32::from_bits(TOP_BITS.load(Ordering::Relaxed)),
        f32::from_bits(BOTTOM_BITS.load(Ordering::Relaxed)),
    )
}

/// Live refresh from the already-running activity (called every ~60 frames).
/// Startup runs before the decor view is attached, so the one-shot `init`
/// can see null insets; this re-query converges once the window is live.
pub fn refresh(top_dp: f32, bottom_dp: f32) {
    if top_dp > 0.0 && top_dp < 200.0 && bottom_dp >= 0.0 && bottom_dp < 200.0 {
        store(top_dp, bottom_dp);
    }
}

/// One-shot query; called from `android_main` before the event loop starts.
pub fn init(app: &AndroidApp) {
    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr() as *mut jni::sys::JavaVM) };
    let activity_raw = app.activity_as_ptr() as jni::sys::jobject;
    let outcome: Result<Option<(f32, f32)>, jni::errors::Error> =
        vm.attach_current_thread(|env: &mut Env| {
            let activity = unsafe { JObject::from_raw(env, activity_raw) };
            let found =
                try_window_insets(env, &activity).or_else(|| try_dimen_fallback(env, &activity));
            if found.is_none() {
                env.exception_clear();
            }
            Ok(found)
        });
    match outcome {
        Ok(Some((top, bottom))) => {
            eprintln!("deck-android: system insets {top:.1}dp top, {bottom:.1}dp bottom");
            store(top, bottom);
        }
        _ => eprintln!("deck-android: inset query failed, using fallback dp"),
    }
}

/// API 30+: decorView.rootWindowInsets.getInsetsIgnoringVisibility(systemBars()).
fn try_window_insets(env: &mut Env, activity: &JObject) -> Option<(f32, f32)> {
    let window = env
        .call_method(
            activity,
            jni_str!("getWindow"),
            jni_sig!("()Landroid/view/Window;"),
            &[],
        )
        .ok()?
        .l()
        .ok()?;
    let decor = env
        .call_method(
            &window,
            jni_str!("getDecorView"),
            jni_sig!("()Landroid/view/View;"),
            &[],
        )
        .ok()?
        .l()
        .ok()?;
    let insets = env
        .call_method(
            &decor,
            jni_str!("getRootWindowInsets"),
            jni_sig!("()Landroid/view/WindowInsets;"),
            &[],
        )
        .ok()?
        .l()
        .ok()?;
    if insets.as_raw().is_null() {
        return None;
    }
    let bars_class = env
        .find_class(jni_str!("android/view/WindowInsets$Type"))
        .ok()?;
    let bars = env
        .call_static_method(&bars_class, jni_str!("systemBars"), jni_sig!("()I"), &[])
        .ok()?
        .i()
        .ok()?;
    let sys = env
        .call_method(
            &insets,
            jni_str!("getInsetsIgnoringVisibility"),
            jni_sig!("(I)Landroid/graphics/Insets;"),
            &[JValue::Int(bars)],
        )
        .ok()?
        .l()
        .ok()?;
    let top_px: jint = env
        .get_field(&sys, jni_str!("top"), jni_sig!("I"))
        .ok()?
        .i()
        .ok()?;
    let bottom_px: jint = env
        .get_field(&sys, jni_str!("bottom"), jni_sig!("I"))
        .ok()?
        .i()
        .ok()?;
    let density = display_density(env, activity)?;
    Some((top_px as f32 / density, bottom_px as f32 / density))
}

/// API 29 fallback: android dimens `status_bar_height` / `navigation_bar_height`.
fn try_dimen_fallback(env: &mut Env, activity: &JObject) -> Option<(f32, f32)> {
    let resources = env
        .call_method(
            activity,
            jni_str!("getResources"),
            jni_sig!("()Landroid/content/res/Resources;"),
            &[],
        )
        .ok()?
        .l()
        .ok()?;
    let px = |env: &mut Env, name: &str| -> Option<jint> {
        let jname = env.new_string(name).ok()?;
        let jdimen = env.new_string("dimen").ok()?;
        let jpkg = env.new_string("android").ok()?;
        let id: jint = env
            .call_method(
                &resources,
                jni_str!("getIdentifier"),
                jni_sig!("(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)I"),
                &[
                    JValue::from(&jname),
                    JValue::from(&jdimen),
                    JValue::from(&jpkg),
                ],
            )
            .ok()?
            .i()
            .ok()?;
        if id == 0 {
            return None;
        }
        env.call_method(
            &resources,
            jni_str!("getDimensionPixelSize"),
            jni_sig!("(I)I"),
            &[JValue::Int(id)],
        )
        .ok()?
        .i()
        .ok()
    };
    let density = display_density(env, activity)?;
    Some((
        px(env, "status_bar_height")? as f32 / density,
        px(env, "navigation_bar_height")? as f32 / density,
    ))
}

fn display_density(env: &mut Env, activity: &JObject) -> Option<f32> {
    let resources = env
        .call_method(
            activity,
            jni_str!("getResources"),
            jni_sig!("()Landroid/content/res/Resources;"),
            &[],
        )
        .ok()?
        .l()
        .ok()?;
    let metrics = env
        .call_method(
            &resources,
            jni_str!("getDisplayMetrics"),
            jni_sig!("()Landroid/util/DisplayMetrics;"),
            &[],
        )
        .ok()?
        .l()
        .ok()?;
    env.get_field(&metrics, jni_str!("density"), jni_sig!("F"))
        .ok()?
        .f()
        .ok()
}
