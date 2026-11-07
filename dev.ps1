# Geliştirme modunda çalıştırır. Arayüz (ui/) derleme gerektirmez; değişiklik
# sonrası pencerede Ctrl+R ile yeniden yüklenmesi için ui/ yerel bir sunucudan verilir.
param([int]$Port = 1430)

$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"

$server = Start-Job -ScriptBlock {
    param($ui, $port)
    $listener = [System.Net.HttpListener]::new()
    $listener.Prefixes.Add("http://localhost:$port/")
    $listener.Start()
    $types = @{ '.html' = 'text/html'; '.js' = 'text/javascript'; '.css' = 'text/css'; '.png' = 'image/png'; '.wasm' = 'application/wasm'; '.svg' = 'image/svg+xml'; '.json' = 'application/json' }
    while ($listener.IsListening) {
        $ctx = $listener.GetContext()
        $rel = [Uri]::UnescapeDataString($ctx.Request.Url.AbsolutePath.TrimStart('/'))
        if ($rel -eq '') { $rel = 'index.html' }
        $file = Join-Path $ui $rel
        if (Test-Path $file -PathType Leaf) {
            $bytes = [IO.File]::ReadAllBytes($file)
            $ext = [IO.Path]::GetExtension($file)
            $ctx.Response.ContentType = $(if ($types[$ext]) { $types[$ext] } else { 'application/octet-stream' })
            $ctx.Response.Headers.Add('Cache-Control', 'no-store')
            $ctx.Response.OutputStream.Write($bytes, 0, $bytes.Length)
        } else { $ctx.Response.StatusCode = 404 }
        $ctx.Response.Close()
    }
} -ArgumentList (Join-Path $root 'ui'), $Port

try {
    $env:TAURI_CONFIG = "{`"build`":{`"devUrl`":`"http://localhost:$Port`"}}"
    Push-Location (Join-Path $root 'src-tauri')
    cargo run
} finally {
    Pop-Location
    Stop-Job $server; Remove-Job $server
    Remove-Item Env:TAURI_CONFIG -ErrorAction SilentlyContinue
}
