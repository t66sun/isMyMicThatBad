param([switch]$ChecksOnly)
. "$PSScriptRoot/Use-Toolchain.ps1"
Push-Location $taskRoot
try {
    if (-not (Test-Path -LiteralPath 'src/app/node_modules')) {
        npm.cmd --prefix src/app ci
        if ($LASTEXITCODE -ne 0) { throw 'npm ci failed' }
    }
    cargo test --manifest-path src/app/src-tauri/Cargo.toml --lib --test audio_pipeline --test hardware_latency
    if ($LASTEXITCODE -ne 0) { throw 'Audio tests failed' }
    npm.cmd --prefix src/app test
    if ($LASTEXITCODE -ne 0) { throw 'Editor tests failed' }
    npm.cmd --prefix src/app run typecheck
    if ($LASTEXITCODE -ne 0) { throw 'TypeScript checks failed' }
    if ($ChecksOnly) { return }
    $version = ([System.IO.File]::ReadAllText((Join-Path $taskRoot 'VERSION'))).Trim()
    $package = Get-Content -LiteralPath 'src/app/package.json' -Raw | ConvertFrom-Json
    $config = Get-Content -LiteralPath 'src/app/src-tauri/tauri.conf.json' -Raw | ConvertFrom-Json
    if ($package.version -ne $version -or $config.version -ne $version) { throw 'Version metadata must match VERSION' }
    $metadataText = cargo metadata --locked --format-version 1 --filter-platform x86_64-pc-windows-msvc --manifest-path src/app/src-tauri/Cargo.toml
    if ($LASTEXITCODE -ne 0) { throw 'Cargo metadata failed' }
    $metadata = $metadataText | ConvertFrom-Json
    $notices = [System.Text.StringBuilder]::new()
    [void]$notices.AppendLine("Third-party notices for isMyMicThatBad $version")
    [void]$notices.AppendLine('VB-CABLE is a separate VB-Audio product. See VB-CABLE-NOTICE.txt and the unmodified vendor readme.')
    foreach ($dependency in $metadata.packages | Where-Object { $_.source }) {
        $folder = Split-Path -Parent $dependency.manifest_path
        [void]$notices.AppendLine("`n--- $($dependency.name) $($dependency.version): $($dependency.license) ---")
        foreach ($file in Get-ChildItem -LiteralPath $folder -File | Where-Object { $_.Name -match '^(LICENSE|LICENCE|COPYING)' }) {
            [void]$notices.AppendLine([System.IO.File]::ReadAllText($file.FullName))
        }
    }
    $npmPaths = npm.cmd --prefix src/app list --omit=dev --all --parseable
    if ($LASTEXITCODE -ne 0) { throw 'Runtime npm dependency listing failed' }
    foreach ($folder in $npmPaths | Select-Object -Skip 1) {
        [void]$notices.AppendLine("`n--- npm: $(Split-Path -Leaf $folder) ---")
        foreach ($file in Get-ChildItem -LiteralPath $folder -File | Where-Object { $_.Name -match '^(LICENSE|LICENCE|COPYING)' }) {
            [void]$notices.AppendLine([System.IO.File]::ReadAllText($file.FullName))
        }
    }
    [System.IO.File]::WriteAllText((Join-Path $taskRoot 'src/app/src-tauri/resources/THIRD-PARTY-NOTICES.txt'), $notices.ToString(), [System.Text.UTF8Encoding]::new($false))
    npm.cmd --prefix src/app run build:win
    if ($LASTEXITCODE -ne 0) { throw 'Windows build failed' }
} finally { Pop-Location }
