$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
$source = Join-Path $taskRoot 'src/native'
$destination = Join-Path $taskRoot 'src/app/src-tauri'
# Keep authored source in src/native; the local Tauri workspace and vendor resources stay ignored.
[void][System.IO.Directory]::CreateDirectory($destination)
foreach ($name in 'src','capabilities','icons','ai') {
    [void][System.IO.Directory]::CreateDirectory("$destination/$name")
    Copy-Item -Path "$source/$name/*" -Destination "$destination/$name" -Recurse -Force
}
foreach ($name in 'Cargo.toml','Cargo.lock','build.rs','tauri.conf.json') {
    Copy-Item -LiteralPath "$source/$name" -Destination "$destination/$name" -Force
}
