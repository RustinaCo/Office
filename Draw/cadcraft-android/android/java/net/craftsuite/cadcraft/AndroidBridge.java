package net.craftsuite.cadcraft;

import android.app.Activity;
import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.ContentResolver;
import android.content.ContentValues;
import android.content.Context;
import android.content.Intent;
import android.database.Cursor;
import android.net.Uri;
import android.os.Environment;
import android.provider.MediaStore;
import android.provider.OpenableColumns;
import android.text.Editable;
import android.text.TextWatcher;
import android.view.Gravity;
import android.view.ViewGroup;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputMethodManager;
import android.widget.EditText;
import android.widget.FrameLayout;

import java.io.ByteArrayOutputStream;
import java.io.InputStream;
import java.io.OutputStream;

/**
 * Native-side Android services for the CADCraft (Rustina Draw) Rust core. All methods are
 * static, fire-and-forget friendly (results are polled, never callbacks), so
 * the Rust side needs no activity-result plumbing.
 */
public class AndroidBridge {
    private static Activity activity;
    private static EditText edit;
    private static String mirror = "";
    private static boolean done = false;
    // Host of the overlay window (kept for the process lifetime once attached).
    private static android.view.View panelHost;

    // ---- lifecycle -------------------------------------------------------

    public static void ensure(Activity act) {
        activity = act;
        act.runOnUiThread(() -> {
            if (edit != null) return;
            edit = new EditText(act);
edit.setCursorVisible(false);
            edit.setBackground(null);
            edit.setSingleLine(true);
            edit.setImeOptions(EditorInfo.IME_ACTION_DONE
                    | EditorInfo.IME_FLAG_NO_EXTRACT_UI
                    | EditorInfo.IME_FLAG_NO_FULLSCREEN);
            edit.setInputType(EditorInfo.TYPE_CLASS_TEXT);
            // Pin the overlay to a single pixel in the top-left corner: it still takes
            // focus and the keyboard, but touches anywhere else pass through to the
            // drawing (a full-size overlay would swallow every canvas tap).
            FrameLayout.LayoutParams lp = new FrameLayout.LayoutParams(1, 1);
            lp.gravity = Gravity.TOP | Gravity.LEFT;
            edit.setLayoutParams(lp);
            edit.setOnEditorActionListener((v, actionId, event) -> {
                if (actionId == EditorInfo.IME_ACTION_DONE) {
                    done = true;
                    return true;
                }
                return false;
            });            edit.addTextChangedListener(new TextWatcher() {
                public void beforeTextChanged(CharSequence s, int a, int b, int c) {}
                public void onTextChanged(CharSequence s, int a, int b, int c) {}
                public void afterTextChanged(Editable s) { mirror = s.toString(); }
            });
            // NOTE: not attached here (see ensurePanel).
        });
    }

    /**
     * NativeActivity draws its surface on top of every decor view, so an
     * overlay living in the decor stays invisible (and keyboards refuse it).
     * Instead the overlay lives in a non-touchable panel window above the
     * surface: taps pass through to the app, focus/keyboard work normally.
     */
    private static void ensurePanel() {
        if (panelHost != null || activity == null || edit == null) return;
        // Attached straight to the decor (a separate panel window breaks
        // window-focus tracking for the IME).
        ((ViewGroup) activity.getWindow().getDecorView()).addView(edit);
        panelHost = edit;
    }
    /** Current overlay text (updated on the UI thread by the TextWatcher). */
    public static String imePoll() { return mirror == null ? "" : mirror; }

    public static void imeFocus(String text) {
        if (activity == null || edit == null) return;
        done = false;
        final String seed = text == null ? "" : text;
        activity.runOnUiThread(() -> {
            ensurePanel();
            edit.setText(seed);
            edit.setSelection(edit.getText().length());
            edit.setFocusable(true);
            edit.setFocusableInTouchMode(true);
            edit.setShowSoftInputOnFocus(true);
            edit.requestFocusFromTouch();
            // showSoftInput must run after the focus change settles: called
            // synchronously the request dies with "view is not served".
            edit.postDelayed(
                    () -> {
                        InputMethodManager imm2 =
                                (InputMethodManager) activity.getSystemService(Context.INPUT_METHOD_SERVICE);
                        imm2.showSoftInput(edit, android.view.inputmethod.InputMethodManager.SHOW_IMPLICIT);
                    },
                    120);
        });
    }

    /** Replace the overlay text without re-focusing or re-showing the keyboard. */
    public static void imeSetText(String text) {
        if (activity == null || edit == null) return;
        final String t = text == null ? "" : text;
        activity.runOnUiThread(() -> {
            done = false;
            edit.setText(t);
            edit.setSelection(edit.getText().length());
        });
    }

    /** Re-request the keyboard while the overlay owns the session (cheap once shown). */
    public static void imeShow() {
        if (activity == null || edit == null) return;
        activity.runOnUiThread(() -> {
            try {
                InputMethodManager imm =
                        (InputMethodManager) activity.getSystemService(Context.INPUT_METHOD_SERVICE);
                imm.showSoftInput(edit, android.view.inputmethod.InputMethodManager.SHOW_IMPLICIT);
            } catch (Exception ignored) {
            }
        });
    }

    public static boolean imeTakeDone() {
        boolean d = done;
        done = false;
        return d;
    }

    public static void imeHide() {
        if (activity == null || edit == null) return;
        activity.runOnUiThread(() -> {
            InputMethodManager imm =
                    (InputMethodManager) activity.getSystemService(Context.INPUT_METHOD_SERVICE);
            imm.hideSoftInputFromWindow(edit.getWindowToken(), 0);
            edit.clearFocus();
        });
    }

    // ---- clipboard --------------------------------------------------------

    public static void putText(String s) {
        if (activity == null) return;
        ClipboardManager cm =
                (ClipboardManager) activity.getSystemService(Context.CLIPBOARD_SERVICE);
        cm.setPrimaryClip(ClipData.newPlainText("Rustina Draw", s == null ? "" : s));
    }

    public static String getText() {
        try {
            if (activity == null) return null;
            ClipboardManager cm =
                    (ClipboardManager) activity.getSystemService(Context.CLIPBOARD_SERVICE);
            if (!cm.hasPrimaryClip()) return null;
            CharSequence t = cm.getPrimaryClip().getItemAt(0).coerceToText(activity);
            return t == null ? null : t.toString();
        } catch (Exception e) {
            return null;
        }
    }

    // ---- locale -----------------------------------------------------------

    /** The device's language tag ("fa-IR", "de-DE"), for the interface language. */
    public static String localeTag() {
        try {
            return java.util.Locale.getDefault().toLanguageTag();
        } catch (Exception e) {
            return null;
        }
    }

    // ---- system bars: solid + contrasting icons, never hidden --------------

    /**
     * Solid status/navigation bars with icons contrasting the theme: white
     * icons on black bars (dark theme), dark icons on white bars (light).
     * Called whenever the app theme changes.
     */
    public static void styleBars(final boolean dark) {
        if (activity == null) return;
        activity.runOnUiThread(() -> {
            try {
                android.view.Window w = activity.getWindow();
                int bar = dark ? 0xFF000000 : 0xFFFFFFFF;
                w.setStatusBarColor(bar);
                w.setNavigationBarColor(bar);
                if (android.os.Build.VERSION.SDK_INT >= 30) {
                    android.view.WindowInsetsController c = w.getInsetsController();
                    if (c != null) {
                        int mask = android.view.WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS
                                | android.view.WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS;
                        c.setSystemBarsAppearance(dark ? 0 : mask, mask);
                    }
                } else {
                    android.view.View decor = w.getDecorView();
                    int vis = decor.getSystemUiVisibility();
                    int flags = android.view.View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR
                            | android.view.View.SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR;
                    if (dark) vis &= ~flags;
                    else vis |= flags;
                    decor.setSystemUiVisibility(vis);
                }
            } catch (Exception ignored) {
            }
        });
    }


    /** Live system-bar insets in dp as "top,bottom", or null before attach. */
    public static String systemInsetsDp() {
        try {
            if (activity == null) return null;
            android.view.View decor = activity.getWindow().getDecorView();
            android.view.WindowInsets insets = decor.getRootWindowInsets();
            if (insets == null) return null;
            android.graphics.Insets sys =
                    insets.getInsetsIgnoringVisibility(android.view.WindowInsets.Type.systemBars());
            float density = activity.getResources().getDisplayMetrics().density;
            if (density <= 0) return null;
            return (sys.top / density) + "," + (sys.bottom / density);
        } catch (Exception e) {
            return null;
        }
    }

    // ---- system file picker (SAF via transparent helper activity) ---------

    public static void launchOpen(final String mimesCsv) {
        if (activity == null) return;
        activity.runOnUiThread(() -> {
            Intent i = new Intent(activity, FilePickerActivity.class);
            i.putExtra("mode", "open");
            i.putExtra("mimes", mimesCsv == null ? "" : mimesCsv);
            try {
                activity.startActivity(i);
            } catch (Exception ignored) {
            }
        });
    }

    public static void launchCreate(final String mime, final String title) {
        if (activity == null) return;
        activity.runOnUiThread(() -> {
            Intent i = new Intent(activity, FilePickerActivity.class);
            i.putExtra("mode", "create");
            i.putExtra("mime", mime == null ? "*/*" : mime);
            i.putExtra("title", title == null ? "" : title);
            try {
                activity.startActivity(i);
            } catch (Exception ignored) {
            }
        });
    }

    private static String openName;
    private static byte[] openBytes;
    private static String createUri;
    private static boolean cancelled = false;

    /** Called on the UI thread by {@link FilePickerActivity}. */
    public static synchronized void stashOpen(android.content.Context ctx, Uri uri) {
        openName = null;
        openBytes = null;
        try (Cursor c = ctx.getContentResolver().query(uri, null, null, null, null)) {
            if (c != null && c.moveToFirst()) {
                int idx = c.getColumnIndex(OpenableColumns.DISPLAY_NAME);
                if (idx >= 0) openName = c.getString(idx);
            }
        } catch (Exception e) {
            openName = null;
        }
        try (InputStream in = ctx.getContentResolver().openInputStream(uri);
                ByteArrayOutputStream out = new ByteArrayOutputStream()) {
            if (in == null) return;
            byte[] buf = new byte[65536];
            int n;
            while ((n = in.read(buf)) > 0) out.write(buf, 0, n);
            openBytes = out.toByteArray();
        } catch (Exception e) {
            openBytes = null;
        }
        if (openName == null) openName = "shared-file";
        cancelled = false;
    }

    /** Called on the UI thread by {@link FilePickerActivity}. */
    public static synchronized void stashCreate(Uri uri) {
        createUri = uri == null ? null : uri.toString();
        cancelled = false;
    }

    /** Called on the UI thread by {@link FilePickerActivity}. */
    public static synchronized void pickerCancelled() {
        cancelled = true;
    }

    public static synchronized String takeOpenName() {
        String n = openName;
        openName = null;
        return n;
    }

    public static synchronized byte[] takeOpenBytes() {
        byte[] b = openBytes;
        openBytes = null;
        return b;
    }

    public static synchronized String takeCreateUri() {
        String u = createUri;
        createUri = null;
        return u;
    }

    public static synchronized boolean takeCancelled() {
        boolean c = cancelled;
        cancelled = false;
        return c;
    }

    public static boolean writeUri(String uri, byte[] data) {
        try {
            Uri u = Uri.parse(uri);
            try (OutputStream os = activity.getContentResolver().openOutputStream(u, "wt")) {
                if (os == null) return false;
                os.write(data);
            }
            return true;
        } catch (Exception e) {
            return false;
        }
    }

    // ---- inbound SEND share (read once at startup) -------------------------

    private static boolean inboundRead = false;
    private static String inboundName;
    private static byte[] inboundBytes;

    public static String inboundName(Activity act) {
        readInbound(act);
        return inboundName;
    }

    public static byte[] inboundBytes(Activity act) {
        readInbound(act);
        return inboundBytes;
    }

    private static void readInbound(Activity act) {
        if (inboundRead) return;
        inboundRead = true;
        try {
            Intent intent = act.getIntent();
            if (intent == null || !Intent.ACTION_SEND.equals(intent.getAction())) return;
            Uri uri = intent.getParcelableExtra(Intent.EXTRA_STREAM);
            if (uri == null) return;
            try (Cursor c = act.getContentResolver().query(uri, null, null, null, null)) {
                if (c != null && c.moveToFirst()) {
                    int idx = c.getColumnIndex(OpenableColumns.DISPLAY_NAME);
                    if (idx >= 0) inboundName = c.getString(idx);
                }
            }
            try (InputStream in = act.getContentResolver().openInputStream(uri);
                    ByteArrayOutputStream out = new ByteArrayOutputStream()) {
                if (in == null) return;
                byte[] buf = new byte[65536];
                int n;
                while ((n = in.read(buf)) > 0) out.write(buf, 0, n);
                inboundBytes = out.toByteArray();
            }
            if (inboundName == null) inboundName = "shared-file";
        } catch (Exception e) {
            inboundName = null;
            inboundBytes = null;
        }
    }
}
