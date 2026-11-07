# Formia

[MIT lisanslı](LICENSE) açık kaynak · [Yol haritası](ROADMAP.md)

3D baskı arşivleri için dosya gezgini. Klasörleri Windows Gezgini gibi gezer, STL / 3MF / OBJ / STEP ve diğer 3D dosyaları **küçük resimleriyle** listeler, **Boşluk (Space)** tuşuyla macOS Finder'daki gibi büyük, döndürülebilir bir önizleme açar.

Tauri 2 ile yazıldı: dosya sistemi, model okuma ve küçük resim üretimi Rust'ta (hızlı, çok çekirdekli); arayüz düz HTML/CSS/JS + three.js. Windows için derlenir, aynı kod macOS'ta da derlenebilir.

## Özellikler

- **Kenar çubuğu:** Hızlı erişim (Ana klasör, Masaüstü, Belgeler, İndirilenler, Resimler), **Favoriler** (sürükleyerek sıralama, yeniden adlandırma), tüm sürücüler (doluluk çubuğuyla; USB, ağ ve Google Drive sürücüleri dahil).
- **Küçük resimler:** Rust tarafında yazılım rasterleştirici ile çizilir (GPU gerekmez), diskte önbelleğe alınır, ekranda görünen dosyalar önce işlenir.
  - 3MF dosyalarında slicer'ın gömdüğü plaka görseli kullanılır (Ayarlar'dan kapatılabilir).
  - G-code içine gömülü önizleme görselleri (PrusaSlicer / Bambu / Orca) okunur.
- **Quick Look önizleme (Space):** fareyle döndürme, kaydırma, yakınlaştırma; ölçüler (mm), üçgen sayısı; tel kafes, yumuşak gölgeleme, zemin ızgarası, otomatik döndürme, yukarı ekseni (Z/Y) değiştirme. Ok tuşları ızgaradaki gibi çalışır (← → önceki/sonraki, ↑ ↓ üst/alt satır).
- **AMS / filament renkleri:** Bambu Studio, Orca Slicer ve PrusaSlicer 3MF'lerinde obje/parça filament atamaları, boyanmış yüzeyler (`paint_color`, `mmu_segmentation`) ve projedeki filament renkleri okunur; önizleme ve (slicer görseli kapalıyken) küçük resimler bu renklerle çizilir. Standart 3MF `basematerials`/`colorgroup` renkleri ve glTF malzeme renkleri de desteklenir. Önizlemede `C` ile tek renge geçilir.
- **Renk menüsü:** önizlemenin altındaki renk düğmesiyle filament renkleri (tek renkli dosyalarda model rengi) yalnızca bakmak için değiştirilir; hazır PLA renkleri ya da istenen renk. Dosya ve küçük resimler değişmez; çok plakalı projelerde seçim bütün plakalara uygulanır, başka dosyaya geçince sıfırlanır.
- **Plakalar:** Bambu Studio / Orca projelerinde birden çok plaka varsa önizlemenin solunda plakalar küçük resimleriyle sıralanır, ortada seçilen plaka gösterilir.
- **G-code bitmiş baskı görünümü:** ekstrüzyon hatları G-code'daki çizgi genişliği ve katman yüksekliğiyle şerit olarak çizilir (PrusaSlicer, Bambu Studio, Orca, Cura yorumları; G2/G3 yayları; T0/T1 araç değişimleri filament renkleriyle). İç dolgu ve başlangıç temizleme hattı çizilmez. Çok büyük dosyalarda ince takım yolu çizgilerine dönülür.
- **Silme ve yeniden adlandırma:** `Delete` seçili dosyaları (Ctrl/Shift ile çoklu seçim) onay sorduktan sonra Geri Dönüşüm Kutusu'na taşır; onay penceresinde odak "Sil" üzerindedir, `Esc` iptal eder. Önizleme açıkken `Delete` dosyayı silip sıradakine geçer. `F2` ile yerinde yeniden adlandırma.
- **Zip arşivleri klasör gibi açılır**, içindeki modellerin küçük resmi ve önizlemesi doğrudan gösterilir.
- **Alt klasörleri tara:** bulunduğunuz klasörün altındaki (istenirse zip içleri dahil) tüm 3D modelleri tek listede toplar; her birinin konumu gösterilir.
- **Sadece 3D** filtresi, anlık arama (Türkçe karakter duyarsız), ızgara / liste görünümü, ad / tarih / boyut / tür / konum sıralaması.
- Dosyaları **sürükleyip slicer'a (Bambu Studio, Orca, Cura, PrusaSlicer) veya Gezgin'e bırakabilirsiniz**; zip içindekiler otomatik olarak geçici klasöre çıkarılır.
- Koyu / açık / sistem teması.

## Desteklenen biçimler

| Tür | Uzantılar | Küçük resim | 3B önizleme |
| --- | --- | --- | --- |
| Baskı modelleri | `stl` `3mf` `obj` `ply` `off` | Rust | Rust ayrıştırıcı → three.js |
| Sahne biçimleri | `glb` `gltf` | Rust | three.js (malzeme ve dokularla) |
| Diğer | `fbx` `dae` `3ds` `amf` `wrl` | three.js (önbelleğe alınır) | three.js |
| CAD | `step` `stp` `iges` `igs` `brep` | OpenCascade (WebAssembly) | OpenCascade, renkli |
| G-code | `gcode` `gco` `g` | gömülü görsel | bitmiş baskı (şerit; çok büyükse çizgi, 80 MB'a kadar) + gömülü görsel |
| Görsel / belge | `png` `jpg` `webp` `gif` `bmp` `tga` · `pdf` `txt` `md` `scad` … · `mp4` `webm` | görseller için | görsel, PDF, metin, video |

## Klavye

| Tuş | İşlev |
| --- | --- |
| `Space` | Önizlemeyi aç / kapat |
| `←` `→` `↑` `↓` | Seçimi taşı (önizlemede de aynı: ← → önceki/sonraki, ↑ ↓ üst/alt satır) |
| `Enter` / çift tık, `Alt+↓` (Mac: `Cmd+↓`) | Klasörü aç, dosyayı varsayılan uygulamada aç |
| `Delete` | Seçilenleri Geri Dönüşüm Kutusu'na taşı (onaylı; önizlemede de çalışır) |
| `F2` | Yeniden adlandır |
| `Backspace`, `Alt+↑` (Mac: `Cmd+↑`) | Üst klasör |
| `Alt+←` / `Alt+→`, fare geri/ileri tuşları | Geri / ileri |
| `Ctrl+L`, `F4` | Adres çubuğunu düzenle (yol yapıştırabilirsiniz) |
| `Ctrl+F` | Ara |
| `Ctrl+Shift+F` | Alt klasörleri tara / taramadan çık |
| `Ctrl+D` | Bulunduğun klasörü favorilere ekle / çıkar |
| `Ctrl+1` / `Ctrl+2` | Izgara / liste |
| `Ctrl+Tekerlek`, `Ctrl +` / `Ctrl -` | Küçük resim boyutu |
| `F5` | Yenile |
| Harf tuşları | O harfle başlayan dosyaya atla |
| Önizlemede `R` `C` `W` `S` `G` `A` `U` | Görünümü sıfırla, filament renkleri, tel kafes, yumuşak, ızgara, otomatik döndür, yukarı ekseni |

## Derleme

Gerekenler: [Rust](https://rustup.rs) (MSVC araç zinciri), Node.js (yalnızca Tauri CLI'yi `npx` ile çalıştırmak için), WebView2 (Windows 10/11'de hazır gelir).

```powershell
# Geliştirme: arayüz dosyaları değişince pencerede Ctrl+R yeterli
.\dev.ps1

# Kurulum dosyası (NSIS) + taşınabilir exe
.\build.ps1
```

Notlar:

- `node_modules` kullanılmaz; three.js ve occt-import-js `ui/vendor/` altına kopyalanmıştır (lisanslar: `THIRD_PARTY_NOTICES.md`). Tauri CLI `npx` ile indirilmeden çalışır.
- Proje klasörü bir eşitleme klasöründeyse (Drive, Seafile, OneDrive…) Rust derleme çıktısını yerel diske yönlendirin: `src-tauri/.cargo/config.toml` içine `[build] target-dir = "C:/…/formia-target"` (bu dosya repoya girmez). `build-mac.sh` bunu macOS'ta `CARGO_TARGET_DIR` ile kendisi yapar.
- Derleme çıktıları ve bağımlılıklar bilgisayarlar arasında eşitlenmez; her bilgisayar repoyu `git clone` ile alır ve kendisi derler.

macOS'ta: `./build-mac.sh` → `Formia.app` ve `.dmg`. Gerekenler: Xcode Command Line Tools (`xcode-select --install`), Rust (`curl https://sh.rustup.rs -sSf | sh`), Node.js. Uygulama imzasız olduğu için ilk açılışta Finder'da sağ tık → Aç demek gerekir. Mac'te silme kısayolu Finder'daki gibi `Cmd+Backspace` (ya da `Delete`).

## Mimari

```
src-tauri/src/
  lib.rs        Tauri komutları ve thumb:// raw:// mesh:// protokolleri
  vfs.rs        Gerçek yollar + zip içi sanal yollar (D:\a\paket.zip\parca.stl)
  fs_ops.rs     Klasör listeleme, sürücüler, hızlı erişim
  scan.rs       Özyinelemeli arşiv taraması (parça parça arayüze akar)
  thumbs.rs     Küçük resim servisi: LIFO kuyruk, iş parçacığı havuzu, disk önbelleği
  raster.rs     Yazılım rasterleştirici (z-buffer, 2x süper örnekleme, kontur)
  gcode.rs      G-code gömülü görsel okuyucu
  mesh/         STL, OBJ, 3MF (production extension dahil), PLY, OFF, glTF ayrıştırıcıları
ui/
  js/main.js        uygulama kabuğu: gezinme, filtre, tarama, kısayollar
  js/fileview.js    sanallaştırılmış ızgara/liste (on binlerce dosyada akıcı)
  js/preview.js     Quick Look penceresi
  js/viewer3d.js    three.js görüntüleyici
  js/loaders.js     biçime göre model yükleme (Rust mesh, three.js, OpenCascade)
  js/clientthumbs.js  Rust'ın okuyamadığı biçimler için küçük resim üretimi
```

Küçük resim önbelleği: `%LOCALAPPDATA%\com.sessizz.formia\thumbs` (Ayarlar → Temizle). Ayarlar ve favoriler: `%APPDATA%\com.sessizz.formia\settings.json`. Uygulamanın eski adıyla ("Model Gezgini", `com.modelgezgini.app`) kalan ayarlar, favoriler ve küçük resimler ilk açılışta otomatik olarak taşınır (`src-tauri/src/migrate.rs`).

Testler: `cd src-tauri && cargo test`

## Bilinen sınırlamalar

- Boyanmış bir üçgen kendi içinde birden çok renge bölünmüşse en büyük alanı kaplayan renkle çizilir (slicer'daki ince boyama sınırları biraz kabalaşır).
- Büyük STEP dosyalarının dönüştürülmesi birkaç saniye sürebilir.
- Zip arşivinin içindeki dosyalar silinemez / yeniden adlandırılamaz. Kopyalama ve taşıma yoktur; bunlar için sağ tık → "Klasörde göster".
