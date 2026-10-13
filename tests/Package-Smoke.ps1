[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$ArchivePath,
    [Parameter(Mandatory)][string]$ExtractPath,
    [string]$ExpectedBinarySha256
)

$ErrorActionPreference = 'Stop'
$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..')).TrimEnd('\')
$archiveFullPath = [System.IO.Path]::GetFullPath($ArchivePath)
$extractInput = if ([System.IO.Path]::IsPathRooted($ExtractPath)) { $ExtractPath } else { Join-Path $repoRoot $ExtractPath }
$extractFullPath = [System.IO.Path]::GetFullPath($extractInput)
$allowedRoots = @((Join-Path $repoRoot 'release'), (Join-Path $repoRoot 'research'))
if (-not ($allowedRoots | Where-Object { $extractFullPath.StartsWith(([System.IO.Path]::GetFullPath($_).TrimEnd('\') + '\'), [StringComparison]::OrdinalIgnoreCase) })) {
    throw 'ExtractPath must be a new child directory under this repository release/ or research/.'
}
if (Test-Path -LiteralPath $extractFullPath) { throw 'ExtractPath must not already exist; existing data is preserved.' }
if (-not (Test-Path -LiteralPath $archiveFullPath -PathType Leaf)) { throw "Archive not found: $archiveFullPath" }

$relativePath = $extractFullPath.Substring($repoRoot.Length).TrimStart('\')
$currentPath = $repoRoot
foreach ($part in $relativePath.Split('\')) {
    $currentPath = Join-Path $currentPath $part
    if (Test-Path -LiteralPath $currentPath) {
        $item = Get-Item -LiteralPath $currentPath -Force
        if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "ExtractPath crosses a reparse point: $currentPath" }
    }
}

$manifestPath = Join-Path (Split-Path -Parent $archiveFullPath) 'release.json'
if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) { throw 'Adjacent release.json is required.' }
$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
if ($manifest.project -ne 'isMyMicThatBad' -or $manifest.status -ne 'complete') { throw 'release.json must describe a completed isMyMicThatBad release.' }
$acceptance = $manifest.hardware_acceptance
if ($acceptance.version -ne $manifest.version -or $acceptance.status -notin @('passed', 'passed_with_accepted_limitations')) { throw 'release.json hardware acceptance must match this release version and have an accepted status.' }
if ($acceptance.microphone.status -notin @('passed','passed_with_accepted_latency') -or $acceptance.model_picker.status -ne 'passed') { throw 'Real microphone stability and model file-picker acceptance must both pass; explicitly accepted latency may exceed its target.' }
$archiveRecord = @($manifest.files | Where-Object { $_.path -eq (Split-Path -Leaf $archiveFullPath) } | Select-Object -First 1)
if ($archiveRecord.Count -ne 1) { throw 'release.json has no file record for this ZIP.' }
$archiveInfo = Get-Item -LiteralPath $archiveFullPath
if ($archiveRecord[0].bytes -ne $archiveInfo.Length -or $archiveRecord[0].sha256 -ne (Get-FileHash -LiteralPath $archiveFullPath -Algorithm SHA256).Hash.ToLowerInvariant()) {
    throw 'ZIP size or SHA-256 does not match release.json.'
}

$manifestBinarySha256 = [string]$manifest.hardware_acceptance.binary_sha256
if ($ExpectedBinarySha256 -and $manifestBinarySha256 -and $ExpectedBinarySha256 -ne $manifestBinarySha256) { throw 'ExpectedBinarySha256 does not match hardware_acceptance in release.json.' }
$expectedSha256 = if ($ExpectedBinarySha256) { $ExpectedBinarySha256 } else { $manifestBinarySha256 }
if ($expectedSha256 -notmatch '^[0-9a-fA-F]{64}$') { throw 'Pass a valid expected binary SHA-256 or provide hardware_acceptance.binary_sha256 in release.json.' }

Add-Type -AssemblyName System.IO.Compression.FileSystem
$archive = [System.IO.Compression.ZipFile]::OpenRead($archiveFullPath)
try {
    $entryNames = [System.Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach ($entry in $archive.Entries) {
        $name = $entry.FullName.Replace('\', '/').TrimEnd('/')
        if (-not $name -or $name.StartsWith('/') -or $name -match '^[A-Za-z]:' -or $name -match '(^|/)\.\.?(/|$)') { throw "Unsafe ZIP path: $($entry.FullName)" }
        if (-not $entryNames.Add($name)) { throw "Duplicate ZIP path: $name" }
        if ($name.Split('/')[0] -in @('research', 'downloads', 'tests', 'logs')) { throw "Unexpected development path at ZIP root: $name" }
        if ($name.StartsWith('data/ai/', [StringComparison]::OrdinalIgnoreCase) -and (($name.Substring(8).Split('/')[0]) -notin @('python', 'rvc', 'models', 'ready.json', 'packages.txt'))) {
            throw "Unexpected AI runtime content: $name"
        }
        $unixMode = ($entry.ExternalAttributes -shr 16) -band 0xF000
        if ($unixMode -eq 0xA000) { throw "ZIP contains a symbolic link: $name" }
    }

    $requiredEntries = @(
        'isMyMicThatBad.exe', 'portable.marker',
        'msvcp140.dll', 'vcruntime140.dll', 'vcruntime140_1.dll',
        'resources/vb-cable/VBCABLE_Setup_x64.exe', 'resources/THIRD-PARTY-NOTICES.txt',
        'README.md', 'AUDIO-ACCEPTANCE.md', 'AUDIO-OPTIMIZATION.md', 'data/settings.json',
        'data/ai/python/python.exe', 'data/ai/python/python312.dll', 'data/ai/python/python312._pth', 'data/ai/python/LICENSE.txt',
        'data/ai/python/Lib/site-packages/torch/__init__.py', 'data/ai/rvc/LICENSE', 'data/ai/rvc/infer/rtrvc.py',
        'data/ai/rvc/RVCRealtimeVST/worker/rvc_worker.py', 'data/ai/rvc/assets/hubert_base/config.json',
        'data/ai/rvc/assets/hubert_base/preprocessor_config.json', 'data/ai/rvc/assets/hubert_base/pytorch_model.bin',
        'data/ai/rvc/assets/rmvpe/rmvpe.pt', 'data/ai/models/saisho.pth', 'data/ai/models/saisho.index',
        'data/ai/models/NOTICE.txt', 'data/ai/ready.json', 'data/ai/packages.txt'
    )
    foreach ($name in $requiredEntries) {
        $entry = $archive.GetEntry($name)
        if (-not $entry) { throw "ZIP is missing required file: $name" }
        if ($name -ne 'portable.marker' -and $entry.Length -le 0) { throw "ZIP contains an empty required file: $name" }
    }
    $binaryEntry = $archive.GetEntry('isMyMicThatBad.exe')
    $binaryStream = $binaryEntry.Open()
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try { $archiveBinarySha256 = [BitConverter]::ToString($sha.ComputeHash($binaryStream)).Replace('-', '').ToLowerInvariant() }
    finally { $binaryStream.Dispose(); $sha.Dispose() }
    if ($archiveBinarySha256 -ne $expectedSha256.ToLowerInvariant()) { throw 'ZIP executable SHA-256 does not match the accepted binary.' }
} finally { $archive.Dispose() }

[System.IO.Compression.ZipFile]::ExtractToDirectory($archiveFullPath, $extractFullPath)
foreach ($item in Get-ChildItem -LiteralPath $extractFullPath -Force -Recurse) {
    if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Extracted package contains a reparse point: $($item.FullName)" }
}
if ((Get-FileHash -LiteralPath (Join-Path $extractFullPath 'isMyMicThatBad.exe') -Algorithm SHA256).Hash -ne $expectedSha256) {
    throw 'Extracted executable SHA-256 does not match the accepted binary.'
}

$readyPath = Join-Path $extractFullPath 'data/ai/ready.json'
$readyText = Get-Content -LiteralPath $readyPath -Raw
$ready = $readyText | ConvertFrom-Json
if ($ready.revision -ne '81eed5e8f68b6bed1789f682fe78cdd324495afc' -or $ready.python -ne '3.12.10' -or $ready.torch -ne '2.7.1+cu118') { throw 'AI ready.json versions do not match the packaged runtime.' }
if ($readyText -match '(?i)([A-Z]:[\\/]|\\\\)') { throw 'AI ready.json contains an absolute path.' }

$pthPath = Join-Path $extractFullPath 'data/ai/python/python312._pth'
$pthLines = @(Get-Content -LiteralPath $pthPath | ForEach-Object { $_.Trim() } | Where-Object { $_ -and -not $_.StartsWith('#') })
if ($pthLines -notcontains 'import site') { throw 'Embedded Python _pth file must enable site packages.' }
foreach ($line in $pthLines | Where-Object { $_ -ne 'import site' }) {
    if ($line -match '^[A-Za-z]:|^\\|^/' -or $line -match '(?i)research') { throw "Python _pth path must be relative and portable: $line" }
}

$packages = Get-Content -LiteralPath (Join-Path $extractFullPath 'data/ai/packages.txt') -Raw
if ($packages -notmatch '(?m)^torch==2\.7\.1\+cu118\s*$' -or $packages -notmatch '(?m)^torchaudio==2\.7\.1\+cu118\s*$') { throw 'packages.txt does not pin the prepared CUDA runtime versions.' }
$settingsPath = Join-Path $extractFullPath 'data/settings.json'
$settingsText = Get-Content -LiteralPath $settingsPath -Raw
$settings = $settingsText | ConvertFrom-Json
if ($settings.schemaVersion -ne 1 -or $settingsText -match '(?i)([A-Z]:[\\/]|\\\\)') { throw 'Bundled settings must be schema version 1 and contain no absolute path.' }
$defaultRvc = @($settings.chain | Where-Object { $_.type -eq 'rvc' })
if ($defaultRvc.Count -eq 0 -or $null -ne $defaultRvc[0].PSObject.Properties['model']) { throw 'Bundled default RVC settings must resolve the packaged model without an explicit model path.' }

$acceptanceDoc = Get-Content -LiteralPath (Join-Path $extractFullPath 'AUDIO-ACCEPTANCE.md') -Raw
foreach ($line in @("Commit: $($manifest.commit)", "Executable SHA-256: $($acceptance.binary_sha256)", "Status: $($acceptance.status)", "Scope: $($acceptance.scope)")) {
    if (-not $acceptanceDoc.Contains($line)) { throw "AUDIO-ACCEPTANCE.md is missing current release data: $line" }
}
$acceptanceJson = [regex]::Match($acceptanceDoc, '(?s)~~~json\r?\n(.*?)\r?\n~~~')
if (-not $acceptanceJson.Success) { throw 'AUDIO-ACCEPTANCE.md must include the current hardware acceptance JSON.' }
$documentAcceptance = $acceptanceJson.Groups[1].Value | ConvertFrom-Json
$documentRecord = ConvertTo-Json -InputObject $documentAcceptance -Depth 20 -Compress
$manifestRecord = ConvertTo-Json -InputObject $acceptance -Depth 20 -Compress
if ($documentRecord -cne $manifestRecord) { throw 'AUDIO-ACCEPTANCE.md does not contain the same acceptance record as release.json.' }

Write-Output "Package smoke passed: $archiveFullPath"
