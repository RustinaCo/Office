# Bundled font attribution (Rustina Draw)

In Draw these fonts are **embedded at build time** rather than copied on first launch: the build
reads `manifest.txt` (craft-fonts format) and `include_bytes!`s each family, so the APK carries them
already. `CRAFT_FONTS_DIR=../fonts` in the build instructions points here. The files are NOT part of
the upstream CADCraft repository.

The Android apps cannot see system fonts, so these open-licensed fonts ship
with the app (copied to the app-private fonts folder on first launch). They
are NOT part of the upstream repositories (which forbid vendored fonts).

All files © their authors, licensed for redistribution:

| File | Family | Source | License |
|---|---|---|---|
| Carlito-Regular.ttf, Carlito-Bold.ttf | Carlito (Calibri-metric-compatible) | https://github.com/google/fonts (ofl/carlito) | OFL 1.1 |
| Caladea-Regular.ttf | Caladea (Cambria-metric-compatible) | https://github.com/google/fonts (ofl/caladea) | OFL 1.1 |
| Arimo-Regular.ttf | Arimo (Arial-metric-compatible) | https://github.com/google/fonts (ofl/arimo) | Apache 2.0 |
| Tinos-Regular.ttf | Tinos (Times-metric-compatible) | https://github.com/google/fonts (ofl/tinos) | Apache 2.0 |
| Cousine-Regular.ttf | Cousine (Courier-metric-compatible) | https://github.com/google/fonts (ofl/cousine) | Apache 2.0 |
| Vazirmatn-Regular.ttf, Vazirmatn-Bold.ttf | Vazirmatn (Persian/Arabic) | https://github.com/google/fonts (ofl/vazirmatn), by Saber Rastikerdar | OFL 1.1 |
| NotoNaskhArabic-Regular.ttf | Noto Naskh Arabic | https://github.com/google/fonts (ofl/notonaskharabic) | OFL 1.1 |
| NotoSansArabic-Regular.ttf | Noto Sans Arabic | https://github.com/google/fonts (ofl/notosansarabic) | OFL 1.1 |
| Cairo-Regular.ttf | Cairo (Arabic) | https://github.com/google/fonts (ofl/cairo) | OFL 1.1 |
| Tajawal-Regular.ttf | Tajawal (Arabic) | https://github.com/google/fonts (ofl/tajawal) | OFL 1.1 |

## Persian (Farsi) families

The popular Persian families, all under the SIL Open Font License 1.1. The
commercial Iranian families Word users ask for by name (B Nazanin, B Titr,
IRANSans, Yekan, IranNastaliq) are **not** here and must not be added: they are
proprietary. Their open equivalents are the text faces below (Nazanin-like:
Sahel, Samim, Shabnam, Estedad, Vazirmatn) and the display faces (Titr-like:
Lalezar, Katibeh, Aref Ruqaa), and a user who owns a licence can still install
one from their own files with the font box's "Add Font…".

| File | Family | Source | License |
|---|---|---|---|
| Sahel-Regular.ttf, Sahel-Bold.ttf | Sahel | https://github.com/rastikerdar/sahel-font, by Saber Rastikerdar | OFL 1.1 |
| Samim-Regular.ttf, Samim-Bold.ttf | Samim | https://github.com/rastikerdar/samim-font, by Saber Rastikerdar | OFL 1.1 |
| Shabnam-Regular.ttf, Shabnam-Bold.ttf | Shabnam | https://github.com/rastikerdar/shabnam-font, by Saber Rastikerdar | OFL 1.1 |
| Tanha-Regular.ttf | Tanha | https://github.com/rastikerdar/tanha-font, by Saber Rastikerdar | OFL 1.1 |
| Gandom-Regular.ttf | Gandom | https://github.com/rastikerdar/gandom-font, by Saber Rastikerdar | OFL 1.1 |
| Nahid-Regular.ttf | Nahid | https://github.com/rastikerdar/nahid-font, by Saber Rastikerdar | OFL 1.1 |
| Parastoo-Regular.ttf, Parastoo-Bold.ttf | Parastoo | https://github.com/rastikerdar/parastoo-font, by Saber Rastikerdar | OFL 1.1 |
| Estedad-Regular.ttf, Estedad-Bold.ttf | Estedad | https://github.com/aminabedi68/Estedad, by Amin Abedi | OFL 1.1 |
| Lalezar-Regular.ttf | Lalezar | https://github.com/google/fonts (ofl/lalezar) | OFL 1.1 |
| Amiri-Regular.ttf, Amiri-Bold.ttf | Amiri | https://github.com/google/fonts (ofl/amiri) | OFL 1.1 |
| ArefRuqaa-Regular.ttf, ArefRuqaa-Bold.ttf | Aref Ruqaa | https://github.com/google/fonts (ofl/arefruqaa) | OFL 1.1 |
| Almarai-Regular.ttf, Almarai-Bold.ttf | Almarai | https://github.com/google/fonts (ofl/almarai) | OFL 1.1 |
| Katibeh-Regular.ttf | Katibeh | https://github.com/google/fonts (ofl/katibeh) | OFL 1.1 |
| MarkaziText-Regular.ttf | Markazi Text | https://github.com/google/fonts (ofl/markazitext) | OFL 1.1 |
| Mirza-Regular.ttf | Mirza | https://github.com/google/fonts (ofl/mirza) | OFL 1.1 |
| NotoNastaliqUrdu-Regular.ttf | Noto Nastaliq Urdu (Nastaliq calligraphy) | https://github.com/google/fonts (ofl/notonastaliqurdu) | OFL 1.1 |
| ScheherazadeNew-Regular.ttf | Scheherazade New | https://github.com/google/fonts (ofl/scheherazadenew) | OFL 1.1 |

Fetched as static TTF subsets via Fontsource CDN
(`https://cdn.jsdelivr.net/fontsource/fonts/<family>@latest/<subset>-<weight>-normal.ttf`);
the Persian families above are complete files, from the projects' own
distributions (raw.githubusercontent.com / cdn.jsdelivr.net).
