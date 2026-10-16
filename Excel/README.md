<div align="center">

# Rustina Excel

**A spreadsheet for Android — the Excel workflow you know, in your pocket.**

[← Rustina Office](../README.md) · [English](README.md) · [فارسی](#فارسی)

![Platform: Android 10+](https://img.shields.io/badge/Android-10%2B%20(arm64)-3ddc84?style=flat-square&logo=android&logoColor=white)
![License: MIT](https://img.shields.io/badge/license-MIT-blue?style=flat-square)

</div>

---

Rustina Excel is the spreadsheet app of the [Rustina Office](../README.md) suite: a native Android
app with the desktop ribbon, the desktop dialogs and every desktop option, laid out for a touch
screen. `.xlsx` is the native format, so your workbooks are the same files you use on the computer.

> The app is called *Rustina Exel* on the phone (package `net.craftsuite.gridcraft`).

<table>
  <tr>
    <td align="center"><img src="screenshots/file-page.png" width="200"><br><sub>The File page</sub></td>
    <td align="center"><img src="screenshots/grid.png" width="200"><br><sub>Editing a sheet</sub></td>
    <td align="center"><img src="screenshots/new-workbook.png" width="200"><br><sub>New workbook</sub></td>
    <td align="center"><img src="screenshots/options.png" width="200"><br><sub>Options</sub></td>
  </tr>
</table>

## What it does

- **Spreadsheets**: cell styles, number formats (Excel's full number-format language), conditional
  formatting and colour scales, borders and fills, frozen panes, merged cells, the fill handle,
  sorting and AutoFilter, data validation, outlining and subtotals, Flash Fill, text to columns,
  find and replace.
- **Formulas**: a dependency-graph calculation engine with dynamic arrays and spilling, **500+
  worksheet functions**, plus `LET`, `LAMBDA`, `MAP`, `REDUCE`, `SCAN`, `BYROW`, `BYCOL` and
  `MAKEARRAY`, structured table references, point-mode entry with coloured references, and the Name
  Box.
- **Data and objects**: tables, PivotTables with a field list, 17 chart types with a Format pane,
  sparklines, pictures (including pictures inside cells), shapes, comments, hyperlinks and text boxes.
- **Files**: opens and saves `.xlsx` (styles, themes, formulas, tables, conditional formats,
  validation, comments, hyperlinks, charts, pictures, sparklines, PivotTables and print settings all
  round-trip), plus `.csv`/`.tsv`. Excel 97-2003 `.xls` workbooks open with their values, formulas and
  number formats. Exports to `.pdf` and prints.
- **Also here**: page-break preview and print settings, dark mode, and the same engine as the desktop
  app.

## On a phone

- **It opens on File.** A full-screen list of destinations — Home, New, Open, Info, Save, Save As,
  Print, Export, Close, Options, About. Tap one for its own full-screen page; **Back** returns to the
  list.
- **Android's Back button.** It closes the keyboard first, then the topmost page or dialog. In the
  workbook, if there is unsaved work it asks **Save / Don't Save / Cancel** — the same question the
  desktop app asks when you close a workbook — and after it is closed you land on the File page.
- **AutoSave that works.** Once the workbook has been saved somewhere, the AutoSave switch in the
  title bar turns on and keeps writing the file as you work; the app keeps its own working copy too.
- **A keyboard that stays where you put it.** Tap a cell and the keyboard comes up; tap a ribbon
  button or a menu and it goes away and stays away.
- **Big targets**: tall ribbon buttons, pill-shaped tab tokens, and a footer with the sheet tabs and
  zoom controls. Tapping the tab you are already on folds the ribbon away, tapping a tab brings it
  back.
- **A Bluetooth keyboard works**: <kbd>Ctrl</kbd>+<kbd>C</kbd>/<kbd>V</kbd>, <kbd>Ctrl</kbd>+<kbd>Z</kbd>,
  <kbd>F2</kbd>, <kbd>Enter</kbd>/<kbd>Tab</kbd> movement, <kbd>Ctrl</kbd>+<kbd>Arrow</kbd> to the edge
  of the data, and the rest of Excel's shortcuts.

## Persian, Arabic and right-to-left

22 open-licensed Persian/Arabic families are bundled (Vazirmatn, Noto Naskh Arabic, Noto Sans Arabic,
Cairo, Tajawal, Sahel, Samim, Shabnam, Tanha, Gandom, Nahid, Parastoo, Estedad, Almarai, Lalezar,
Katibeh, Aref Ruqaa, Amiri, Markazi Text, Mirza, Noto Nastaliq Urdu, Scheherazade New) along with five
metric-compatible Latin families (Carlito ≈ Calibri, Arimo ≈ Arial, Tinos ≈ Times New Roman,
Cousine ≈ Courier New, Caladea ≈ Cambria).

Right-to-left sheet direction works, and the commercial Iranian families (**B Nazanin, B Titr,
B Yekan, IRANSans**…) can be added with **Add Font…**: install the font yourself and it stays in the
font list.

## Notes worth knowing

- **Android 10 (API 29) or newer, arm64.** The APK is signed for sideloading; there is no Play Store
  version.
- **The app only sees the files you hand it** through Android's file picker — which includes cloud
  providers (Google Drive, OneDrive, Dropbox, Nextcloud, a NAS…), so a workbook that lives in your
  laptop's synced folder can be opened and saved from anywhere.
- **It keeps a copy of what you save** in its own private folder, which is what makes AutoSave work
  when the chosen file lives in the cloud; the app writes your file back too, so both stay in step.
- **Very large workbooks are still workbooks.** The grid is virtualised and recalculation is
  incremental, but a 100 MB model on a phone is a phone-sized wait — the app stays responsive and
  recalculates what is on screen.
- **The desktop build is in the same source**: `cargo run --release -p gridcraft` in `gridcraft/`.

## Building

```bash
cargo install cargo-apk
rustup target add aarch64-linux-android

cd grid-android
export ANDROID_HOME=$HOME/Android/Sdk
export ANDROID_NDK_ROOT=$HOME/Android/Sdk/ndk/<version>
cargo apk2 build --lib --release          # cargo-apk ≥ 0.10; older: cargo apk build
```

The APK lands in `grid-android/target/release/apk/`. Package `net.craftsuite.gridcraft`, label
*Rustina Exel*, minSdk 29, targetSdk 35.

- `gridcraft/` — the app (engine, calculation, XLSX I/O, renderer, UI)
- `grid-android/` — the Android shell
- `fonts/` — the bundled open-licensed fonts, compiled into the APK

## Credits and license

Rustina Excel is the [ArtCraft team](https://getartcraft.com/)'s open-source **GridCraft** — engine,
calculation engine, XLSX support and 99% of everything above — with the Android shell and the phone
work added here. Thank you, ArtCraft. MIT OR Apache-2.0; the licence texts are in `gridcraft/`, and
[`../CREDITS.md`](../CREDITS.md) has the full story. Fonts are OFL, listed in `fonts/ATTRIBUTION.md`.

Microsoft and Excel are trademarks of Microsoft Corporation. This is an independent, clean-room
implementation, not affiliated with or endorsed by Microsoft.

<div dir="rtl">

## فارسی

**رستینا اکسل** صفحه‌گستردهٔ اندرویدی مجموعهٔ رستینا آفیس است: همان ریبون و تنظیمات نسخهٔ کامپیوتری،
چیده‌شده برای صفحهٔ لمسی.

- `xlsx` و `csv` را باز و ذخیره می‌کند (`xls` فقط مقدار، فرمول و قالب عددی) و `pdf` می‌سازد.
- بیش از ۵۰۰ تابع، آرایه‌های پویا، جدول، PivotTable، ۱۷ نوع نمودار، قالب‌بندی شرطی و…
- روی گوشی با **File** باز می‌شود؛ هر مقصد صفحهٔ خودش را دارد و **Back** به لیست برمی‌گردد.
- **Back** اندروید اول کیبورد، بعد صفحه، و در صورت تغییر ذخیره‌نشده **ذخیره / ذخیره نکن / لغو**.
- **ذخیرهٔ خودکار** پس از اولین ذخیره واقعاً کار می‌کند.
- ۲۲ فونت فارسی/عربی با مجوز باز همراه برنامه؛ فونت‌های تجاری را با **افزودن فونت…** خودت اضافه کن.
- اندروید ۱۰ یا بالاتر، arm64.

با تشکر از **تیم ArtCraft** سازندگان متن‌باز GridCraft. مجوز MIT.

</div>
