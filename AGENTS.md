# Formia — ajan çalışma rehberi

## Proje

- Formia, 3D baskı arşivleri için açık kaynak dosya gezginidir. Lisans MIT; depo: https://github.com/sessizz/formia.
- Kullanıcıyla Türkçe iletişim kur. Değişiklikleri ve doğrulama sonucunu kısa, somut biçimde açıkla.
- Gereksinimler ve planlanan özellikler için `README.md` ve `ROADMAP.md` dosyalarını oku. Yol haritasındaki maddeleri kendiliğinden uygulamaya başlama.

## Çalışma kopyaları

- Windows'taki güncel çalışma kopyası bu depodur. Eski `3dviewer_claude` kopyasına değişiklik yapma.
- Mac için hedef konum `~/Code/formia`; her bilgisayarda GitHub'dan bağımsız bir klon kullan.
- Kaynak paylaşımı `git pull` / `git push` ile yapılır. Aynı `.git` klasörünü iki bilgisayarda bulut eşitlemesi üzerinden kullanma.
- İşe başlamadan `git status` kontrol et; kullanıcının değişikliklerini koru.
- Makineye özel yolları, kimlik bilgilerini ve derleme çıktılarını depoya ekleme.

## Mimari

- `src-tauri/src/`: Rust arka uç; Tauri komutları, dosya işlemleri, zip sanal dosya sistemi, tarama, model ayrıştırma ve küçük resimler.
- `src-tauri/src/mesh/`: model biçimi ayrıştırıcıları.
- `ui/`: düz HTML/CSS/JavaScript; `ui/js/` uygulama ve three.js görüntüleyici kodu.
- `ui/vendor/`: gömülü three.js ve OpenCascade bağımlılıkları. Güncelleme gerekmiyorsa değiştirme; lisanslarını `THIRD_PARTY_NOTICES.md` ile koru.
- Proje npm uygulaması değildir; köke `package.json` veya `node_modules` ekleme. Tauri CLI, `npx --yes @tauri-apps/cli@2` ile çalışır ve gerekirse npm önbelleğine indirilir.

## Derleme ve doğrulama

- Windows geliştirme: `./dev.ps1`; paketleme: `./build.ps1` (PowerShell).
- Rust testleri: `cd src-tauri` ardından `cargo test`. Windows'ta gerekirse Rust için `%USERPROFILE%/.cargo/bin` dizinini PATH'e ekle.
- macOS: Xcode Command Line Tools, Rust ve Node.js gerekir. Gerekirse önce `source ~/.cargo/env`, ardından depo kökünde `./build-mac.sh` çalıştır.
- Mac betiği varsayılan olarak `~/.cache/model-gezgini-target` altında derler; paketler `release/bundle/macos/` ve `release/bundle/dmg/` altındadır. Güncel hedef Apple Silicon'dır; Intel/universal paket üretimini ayrıca istenmedikçe ekleme.
- Windows'ta yerel Rust çıktı yolu `src-tauri/.cargo/config.toml` ile ayarlanabilir; bu dosya gitignored'dır. Çıktıları eşitleme klasörünün dışında tut.
- Değişiklikle ilgili kontrolleri çalıştır. Rust davranışı değiştiyse ilgili testleri, arayüz değiştiyse ilgili etkileşimleri doğrula. Çalıştırılmayan kontrolleri açıkça belirt.
- Başarılı derlemeyi uygulamanın görsel olarak doğrulandığı şeklinde sunma.

## Kullanıcı verileri ve platform davranışı

- Uygulama kimliği `com.sessizz.formia`; eski `com.modelgezgini.app` verilerinin geçişini koru.
- Kullanıcının açık uygulamasını izinsiz kapatma; ayarlarını, favorilerini ve pencere konumunu test sırasında değiştirme. Gerekli testleri ayrı veriyle çalıştır.
- Gerçek arşiv dosyalarını değiştirme veya silme. Silme, yeniden adlandırma ve zip içi salt okunur davranışlarını koru.
- Windows ve macOS yollarını/kısayollarını birlikte değerlendir; platforma özel değişiklikleri mümkün olduğunca dar tut.
- Başka bir bilgisayarda T3 işlemi yetki hatası verirse hatayı bildir; ilgisiz bir projenin dosyalarını değiştirme veya yetki engelini aşmaya çalışma.
