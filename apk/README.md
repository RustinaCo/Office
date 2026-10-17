# Rustina Office APKs

Signed release builds of the three apps: **arm64-v8a only**, **Android 10 (API 29) or newer**.
Install one by opening the file on the phone and allowing installs from this source when Android
asks (and turning off "install via USB" restrictions if your phone has them).

| File | App | What it opens | Size |
|---|---|---|---|
| [`Rustina-Write-1.0.0.apk`](Rustina-Write-1.0.0.apk) | Rustina Write | `.docx`, `.txt`, `.md`, `.html`, `.rtf`, `.odt` | 23.5 MB |
| [`Rustina-Gride-1.0.0.apk`](Rustina-Gride-1.0.0.apk) | Rustina Gride | `.xlsx`, `.csv` | 9.8 MB |
| [`Rustina-Slides-1.0.0.apk`](Rustina-Slides-1.0.0.apk) | Rustina Slides | `.pptx` | 17.0 MB |

These are built from the sources in this same tag — `Write/`, `Gride/` and `Slides/` — with
`cargo apk2 build --lib --release` (each app folder's README has the exact commands). They are signed
with a self-signed release key so they can be sideloaded; Android will warn about an unknown
developer the first time.

SHA-256 of each file at this tag, if you want to check what you downloaded:

```
44eb4b32042420fd50d2ea00101c267e800122710fc0a7d232420cc43af22bef  Rustina-Write-1.0.0.apk
60a735da240c4abf0241ffe448b0a9c80855a15e8e63da24b94c9f66dfd7c1e7  Rustina-Gride-1.0.0.apk
5de1bf56a16193af89de8fc930b3961f05d1bf9314aa19eec2f3550f5c8bc5ad  Rustina-Slides-1.0.0.apk
```

No account, no ads, no telemetry and no network permission: the apps ask only for the files you hand
them through the system file picker.
