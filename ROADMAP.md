# Formia yol haritası

Sıra, öncelik sırasıdır. Bir madde başlamadan önce kısa bir tarifi yazılır ve onaylanır.

## Altyapı

- [ ] **Açık kaynak GitHub reposu**: `sessizz/formia`, herkese açık; lisans ve üçüncü taraf lisans notları (three.js, occt-import-js).
- [ ] **Projeyi Google Drive'dan yerel diske taşıma**: geliştirme klasörü sürüm geçmişiyle (git) yerel diskte; Drive yalnızca yedek.
- [ ] **Mac'te ayrı T3 projesi**: Mac, repoyu GitHub'dan klonlar; derleme `git pull` + `./build-mac.sh` ile yapılır, kaynak paketi taşıma gerekmez.

## Önizleme

- [ ] **Baskı bilgisi**: G-code ve `.gcode.3mf` için baskı süresi, filament gramajı/metresi, katman yüksekliği, nozul, yazıcı adı önizlemenin altında. 3MF projelerinde (dilimlenmemiş) varsa slicer ayarlarından katman yüksekliği ve yazıcı.
- [ ] **Ölçüm ve kesit**: 3D önizlemede iki nokta arası mesafe; modeli yatay (ve istenirse dikey) bir düzlemle kesip içini görme.
- [ ] **Yazıcı tablası**: model gerçek ölçüsünde yazıcının tablası üstünde gösterilir; tablaya sığmayan parça belirtilir. Ayarlarda yazıcı seçilir (ör. Bambu A1 mini 180 mm, A1 / P1S / X1C 256 mm, H2D 350×320 mm, Prusa MK4 250×210 mm…), tabla ölçüsü ona göre.
- [ ] **G-code küçük resimleri** (düşük öncelik): gömülü önizleme görseli olmayan G-code'lar (çoğu Cura çıktısı) için şerit modelden küçük resim. Yalnızca görseli olmayan dosyalar için, bir boyut sınırıyla ve bir kez (önbelleğe alınır).

## Arşiv

- [ ] **Arşiv dizini**: taranan klasörleri ve zip'leri hatırlayan yerel bir dizin; bütün arşivde isimle anında arama, klasör klasör gezmeden.
- [ ] **Kopya bulucu**: aynı modelin farklı klasörlerde / zip'lerde kaç kopyası olduğunu (içerik özetiyle) bulur, yer kazandırır.

## Uzun vade

- [ ] **İmzalama**: macOS'ta Developer ID + notarization (Apple geliştirici hesabı, yıllık 99 $); Windows'ta kod imzalama sertifikası. "Sağ tık → Aç" ve SmartScreen uyarısı kalkar.
- [ ] **Otomatik güncelleme**: yeni sürüm GitHub Releases'tan indirilir (Tauri updater).

## Yapılmayacaklar

- **Intel Mac desteği**: Apple Intel desteğini bıraktı; yalnızca Apple Silicon (arm64) derlenir.

## Yapılanlar (7 Kasım 2026)

Formia adı ve lavanta tema · Önizlemede renk menüsü (yalnızca bakmak için, plakalar arasında ortak) · 3MF plaka sütunu · G-code bitmiş baskı görünümü (şeritler) · dilimlenmiş `.gcode.3mf` · eski Bambu 3MF'lerinde gri zemin temizleme · Windows'ta çerçevesiz pencere, Alt+F4 · Mac'te Cmd+↑/↓, önizlemede Cmd+Backspace · Mac'te Finder adlarıyla sürücüler.
