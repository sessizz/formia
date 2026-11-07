# Release derlemesi + Windows kurulum dosyası (NSIS).
# Tauri CLI, proje klasörüne node_modules kurmadan npx önbelleğinden çalıştırılır.
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
Remove-Item Env:TAURI_CONFIG -ErrorAction SilentlyContinue
Push-Location (Join-Path $PSScriptRoot 'src-tauri')
try {
    npx --yes @tauri-apps/cli@2 build
    if ($LASTEXITCODE -ne 0) { throw "tauri build başarısız (çıkış kodu $LASTEXITCODE)" }
    $target = (cargo metadata --format-version 1 --no-deps | ConvertFrom-Json).target_directory
    Write-Host "`nÇıktılar:" -ForegroundColor Green
    Get-ChildItem (Join-Path $target 'release\bundle') -Recurse -Include *.exe, *.msi | ForEach-Object { "  $($_.FullName)" }
    "  $(Join-Path $target 'release\formia.exe') (taşınabilir)"
} finally {
    Pop-Location
}
