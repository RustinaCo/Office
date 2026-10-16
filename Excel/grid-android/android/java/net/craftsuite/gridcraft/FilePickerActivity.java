package net.craftsuite.gridcraft;

import android.app.Activity;
import android.content.Intent;
import android.net.Uri;
import android.os.Bundle;

/**
 * Transparent helper that runs the system file picker (Storage Access
 * Framework) and stashes the result for the Rust side to poll. Needed
 * because a NativeActivity cannot receive `onActivityResult` in Rust.
 *
 * <p>Modes (extra "mode"): "open" (EXTRA_MIME_TYPES csv) or "create"
 * (mime + title extras). Results land in {@link AndroidBridge} statics.
 */
public class FilePickerActivity extends Activity {
    private static final int REQ = 41;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        Intent src = getIntent();
        String mode = src == null ? null : src.getStringExtra("mode");
        try {
            if ("open".equals(mode)) {
                Intent open = new Intent(Intent.ACTION_OPEN_DOCUMENT);
                open.addCategory(Intent.CATEGORY_OPENABLE);
                open.setType("*/*");
                String csv = src.getStringExtra("mimes");
                if (csv != null && !csv.isEmpty()) {
                    open.putExtra(Intent.EXTRA_MIME_TYPES, csv.split(","));
                }
                open.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION
                        | Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION);
                startActivityForResult(open, REQ);
            } else if ("create".equals(mode)) {
                Intent create = new Intent(Intent.ACTION_CREATE_DOCUMENT);
                create.addCategory(Intent.CATEGORY_OPENABLE);
                String mime = src.getStringExtra("mime");
                create.setType(mime == null ? "*/*" : mime);
                String title = src.getStringExtra("title");
                if (title != null) create.putExtra(Intent.EXTRA_TITLE, title);
                create.addFlags(Intent.FLAG_GRANT_WRITE_URI_PERMISSION
                        | Intent.FLAG_GRANT_READ_URI_PERMISSION
                        | Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION);
                startActivityForResult(create, REQ);
            } else {
                finish();
            }
        } catch (Exception e) {
            AndroidBridge.pickerCancelled();
            finish();
        }
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        if (requestCode == REQ && resultCode == RESULT_OK && data != null
                && data.getData() != null) {
            Uri uri = data.getData();
            try {
                getContentResolver().takePersistableUriPermission(
                        uri,
                        Intent.FLAG_GRANT_READ_URI_PERMISSION
                                | Intent.FLAG_GRANT_WRITE_URI_PERMISSION);
            } catch (Exception ignored) {
                // Read-only grants (open) reject the write flag; read persists.
                try {
                    getContentResolver().takePersistableUriPermission(
                            uri, Intent.FLAG_GRANT_READ_URI_PERMISSION);
                } catch (Exception ignored2) {
                }
            }
            String mode = getIntent().getStringExtra("mode");
            if ("open".equals(mode)) {
                AndroidBridge.stashOpen(this, uri);
            } else {
                AndroidBridge.stashCreate(uri);
            }
        } else {
            AndroidBridge.pickerCancelled();
        }
        finish();
    }
}
