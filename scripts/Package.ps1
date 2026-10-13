param([string]$AiRuntimePath, [string]$SettingsPath)
. "$PSScriptRoot/Use-Toolchain.ps1"
Push-Location $taskRoot
try {
    $commit = (git rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0) { throw 'Commit validated source before packaging' }
    git diff --quiet HEAD
    if ($LASTEXITCODE -ne 0) { throw 'Source differs from recorded commit' }
    if (git ls-files --others --exclude-standard -- ':!.codegraph/**') { throw 'Uncommitted source remains' }
    $version = (git show HEAD:VERSION).Trim()
    if ($LASTEXITCODE -ne 0 -or $version -notmatch '^\d+\.\d+\.\d+$') { throw 'Committed VERSION must contain SemVer' }
    $tag = "v$version"
    $destination = Join-Path $taskRoot "release/$tag"
    if (Test-Path -LiteralPath $destination) {
        if (-not (Test-Path -LiteralPath "$destination/release.json")) { throw 'Preserve unidentified release directory' }
        $previous = Get-Content -LiteralPath "$destination/release.json" -Raw | ConvertFrom-Json
        if ($previous.project -ne 'isMyMicThatBad' -or $previous.commit -ne $commit -or $previous.status -eq 'complete') { throw 'Release directory already owned' }
    }
    $executable = Join-Path $taskRoot 'src/app/src-tauri/target/release/ismymicthatbad.exe'
    $runtimeEntries = @('python/python.exe','python/python312.dll','python/python312._pth','python/LICENSE.txt','python/Lib/site-packages/torch/__init__.py','rvc/LICENSE','rvc/infer/rtrvc.py','rvc/RVCRealtimeVST/worker/rvc_worker.py','rvc/assets/hubert_base/config.json','rvc/assets/hubert_base/preprocessor_config.json','rvc/assets/hubert_base/pytorch_model.bin','rvc/assets/rmvpe/rmvpe.pt','models/saisho.pth','models/saisho.index','models/NOTICE.txt','ready.json','packages.txt')
    if ($AiRuntimePath) {
        $AiRuntimePath = (Resolve-Path -LiteralPath $AiRuntimePath).Path
        foreach ($name in $runtimeEntries) {
            if (-not (Test-Path -LiteralPath "$AiRuntimePath/$name" -PathType Leaf) -or (Get-Item -LiteralPath "$AiRuntimePath/$name").Length -le 0) { throw "AI runtime missing $name" }
        }
        $ready = Get-Content -LiteralPath "$AiRuntimePath/ready.json" -Raw | ConvertFrom-Json
        if ($ready.revision -ne '81eed5e8f68b6bed1789f682fe78cdd324495afc' -or $ready.python -ne '3.12.10' -or $ready.torch -ne '2.7.1+cu118') { throw 'AI runtime version differs from validated runtime' }
    }
    if ($SettingsPath) {
        $SettingsPath = (Resolve-Path -LiteralPath $SettingsPath).Path
        $settings = Get-Content -LiteralPath $SettingsPath -Raw | ConvertFrom-Json
        if ($settings.schemaVersion -ne 1 -or $settings.bufferMs -lt 5 -or $settings.bufferMs -gt 30) { throw 'Personal settings format is invalid' }
    }
    $acceptance = Get-Content -LiteralPath 'research/audio-acceptance/artifacts/delivery-acceptance.json' -Raw | ConvertFrom-Json
    if ($acceptance.version -ne $version -or $acceptance.status -notin @('passed','passed_with_accepted_limitations') -or $acceptance.binary_sha256 -ne (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash.ToLowerInvariant()) { throw 'Executable and version must match verified hardware/UI build' }
    if ($acceptance.microphone.status -notin @('passed','passed_with_accepted_latency') -or $acceptance.model_picker.status -ne 'passed') { throw 'Real microphone stability and model file-picker acceptance must both pass; explicitly accepted latency may exceed its target before formal packaging' }
    [void][System.IO.Directory]::CreateDirectory($destination)
    $record = [ordered]@{project='isMyMicThatBad';status='pending';version=$version;tag=$tag;commit=$commit;created_at=[datetime]::UtcNow.ToString('o');completed_at=$null;hardware_acceptance=$acceptance;files=@();error=$null}
    function Write-Manifest {
        [System.IO.File]::WriteAllText("$destination/release.json.tmp",($record|ConvertTo-Json -Depth 10),[System.Text.UTF8Encoding]::new($false))
        Move-Item -LiteralPath "$destination/release.json.tmp" -Destination "$destination/release.json" -Force
    }
    Write-Manifest
    try {
        $staging = Join-Path $taskRoot ('research/build-environment/artifacts/portable-' + [guid]::NewGuid().ToString('N'))
        [void][System.IO.Directory]::CreateDirectory($staging)
        Copy-Item -LiteralPath $executable -Destination "$staging/isMyMicThatBad.exe"
        Copy-Item -LiteralPath 'src/app/src-tauri/resources' -Destination "$staging/resources" -Recurse
        foreach ($name in 'msvcp140.dll','vcruntime140.dll','vcruntime140_1.dll') {
            Copy-Item -LiteralPath "src/app/src-tauri/resources/msvc-runtime/$name" -Destination "$staging/$name"
            Remove-Item -LiteralPath "$staging/resources/msvc-runtime/$name"
        }
        [System.IO.File]::WriteAllText("$staging/portable.marker", '', [System.Text.UTF8Encoding]::new($false))
        [void][System.IO.Directory]::CreateDirectory("$staging/data")
        if ($SettingsPath) { Copy-Item -LiteralPath $SettingsPath -Destination "$staging/data/settings.json" }
        if ($AiRuntimePath) {
            [void][System.IO.Directory]::CreateDirectory("$staging/data/ai")
            foreach ($name in 'python','rvc','models','ready.json','packages.txt') {
                $items = @(Get-Item -LiteralPath "$AiRuntimePath/$name")
                if ($items[0].PSIsContainer) { $items += Get-ChildItem -LiteralPath "$AiRuntimePath/$name" -Recurse -Force }
                if ($items | Where-Object { $_.Attributes -band [IO.FileAttributes]::ReparsePoint }) { throw 'AI runtime must contain real files, not junctions or symbolic links' }
                Copy-Item -LiteralPath "$AiRuntimePath/$name" -Destination "$staging/data/ai/$name" -Recurse -Force
            }
        }
        $guide = [System.IO.File]::ReadAllText("$taskRoot/docs/user-guide.md").Replace('../research/audio-acceptance/README.md','AUDIO-ACCEPTANCE.md')
        [System.IO.File]::WriteAllText("$staging/README.md",$guide,[System.Text.UTF8Encoding]::new($false))
        $hardware = @"
# $tag audio and UI acceptance

Commit: $commit
Executable SHA-256: $($acceptance.binary_sha256)
Status: $($acceptance.status)
Scope: $($acceptance.scope)

This record describes the executable in this ZIP. Historical release measurements are not used as current acceptance. Chat-app buffering, subjective voice quality and long-term stability are qualified only when explicitly recorded below.

~~~json
$($acceptance | ConvertTo-Json -Depth 20)
~~~
"@
        [System.IO.File]::WriteAllText("$staging/AUDIO-ACCEPTANCE.md",$hardware,[System.Text.UTF8Encoding]::new($false))
        $optimization = [System.IO.File]::ReadAllText("$taskRoot/docs/audio-optimization.md").Replace('user-guide.md','README.md')
        [System.IO.File]::WriteAllText("$staging/AUDIO-OPTIMIZATION.md",$optimization,[System.Text.UTF8Encoding]::new($false))
        $portablePath = "$destination/isMyMicThatBad-$tag-portable-x64.zip"
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        $zipStream = [IO.File]::Create("$portablePath.tmp.zip")
        $zip = [IO.Compression.ZipArchive]::new($zipStream, [IO.Compression.ZipArchiveMode]::Create, $false)
        try {
            foreach ($file in Get-ChildItem -LiteralPath $staging -Recurse -File -Force) {
                $name = $file.FullName.Substring($staging.Length + 1).Replace('\','/')
                $entry = $zip.CreateEntry($name, [IO.Compression.CompressionLevel]::Optimal)
                $inputStream = $file.OpenRead()
                $outputStream = $entry.Open()
                try { $inputStream.CopyTo($outputStream) } finally { $outputStream.Dispose(); $inputStream.Dispose() }
            }
        } finally { $zip.Dispose(); $zipStream.Dispose() }
        Move-Item -LiteralPath "$portablePath.tmp.zip" -Destination $portablePath -Force
        $archive = [System.IO.Compression.ZipFile]::OpenRead($portablePath)
        try {
            foreach ($name in 'isMyMicThatBad.exe','portable.marker','msvcp140.dll','vcruntime140.dll','vcruntime140_1.dll','resources/vb-cable/VBCABLE_Setup_x64.exe','resources/THIRD-PARTY-NOTICES.txt','README.md','AUDIO-ACCEPTANCE.md','AUDIO-OPTIMIZATION.md') {
                $entry = $archive.Entries | Where-Object { $_.FullName.Replace('\','/') -eq $name }
                if (-not $entry -or ($name -ne 'portable.marker' -and $entry.Length -le 0)) { throw "Portable ZIP missing $name" }
            }
            $copiedEntries = @()
            if ($AiRuntimePath) { $copiedEntries += $runtimeEntries | ForEach-Object { "data/ai/$_" } }
            if ($SettingsPath) { $copiedEntries += 'data/settings.json' }
            foreach ($name in $copiedEntries) {
                $entry = $archive.GetEntry($name)
                if (-not $entry -or $entry.Length -le 0) { throw "Portable ZIP missing $name" }
                $inputStream = [IO.File]::OpenRead((Join-Path $staging $name))
                $entryStream = $entry.Open()
                $sha = [Security.Cryptography.SHA256]::Create()
                try {
                    $expectedHash = [BitConverter]::ToString($sha.ComputeHash($inputStream))
                    if ([BitConverter]::ToString($sha.ComputeHash($entryStream)) -ne $expectedHash) { throw "Portable ZIP differs from prepared file: $name" }
                } finally { $inputStream.Dispose(); $entryStream.Dispose(); $sha.Dispose() }
            }
            $entryStream = ($archive.Entries | Where-Object { $_.FullName -eq 'isMyMicThatBad.exe' }).Open()
            $sha = [System.Security.Cryptography.SHA256]::Create()
            try {
                $hash = [System.BitConverter]::ToString($sha.ComputeHash($entryStream)).Replace('-','').ToLowerInvariant()
                if ($hash -ne $acceptance.binary_sha256) { throw 'ZIP executable differs from tested binary' }
            } finally { $entryStream.Dispose(); $sha.Dispose() }
        } finally { $archive.Dispose() }
        $sourceRecords = foreach ($path in git ls-files -- .gitignore .gitattributes AGENTS.md README.md VERSION codegraph.json src scripts tests docs) {
            "$((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant())  $path"
        }
        [System.IO.File]::WriteAllLines("$destination/SOURCE-SHA256.txt",$sourceRecords,[System.Text.UTF8Encoding]::new($false))
        $nativeRecords = foreach ($file in Get-ChildItem -LiteralPath 'src/app/src-tauri/src' -Recurse -File) {
            "$((Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant())  $($file.FullName.Substring($taskRoot.Length + 1).Replace('\','/'))"
        }
        $nativeRecords += foreach ($path in 'src/app/src-tauri/Cargo.toml','src/app/src-tauri/Cargo.lock','src/app/src-tauri/build.rs','src/app/src-tauri/tauri.conf.json') {
            "$((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant())  $path"
        }
        [System.IO.File]::WriteAllLines("$destination/LOCAL-NATIVE-SHA256.txt",$nativeRecords,[System.Text.UTF8Encoding]::new($false))
        $changelog = @"
# $tag

Commit: $commit

Windows x64 portable voice changer with 14 filters and 7 built-in presets. Adds bitcrusher, original-audio comparison and one reorderable local RVC AI card. RVC v1/v2 model and optional index selection persist with presets; GPU inference runs in an isolated local process and failures stop output with an error. AI preparation downloads the isolated runtime once; packages built with -AiRuntimePath include the prepared runtime and default Saisho Utane voice model with its attribution. Portable settings and the AI runtime live under data beside the executable.

Validation scope and results: $($acceptance.scope)

The exact build's hardware/UI acceptance record is preserved in release.json. See README.md and AUDIO-ACCEPTANCE.md inside the ZIP for measurement methods and remaining limitations. Subjective voice quality and long-term stability are not claimed by this package.

Portable ZIP only. This packaging script does not publish to GitHub.
"@
        [System.IO.File]::WriteAllText("$destination/CHANGELOG.md",$changelog,[System.Text.UTF8Encoding]::new($false))
        $record.files = @(foreach ($path in $portablePath,"$destination/CHANGELOG.md","$destination/SOURCE-SHA256.txt","$destination/LOCAL-NATIVE-SHA256.txt") {
            $info = Get-Item -LiteralPath $path
            if ($info.Length -le 0) { throw "Empty delivery: $path" }
            [ordered]@{path=$info.Name;bytes=$info.Length;sha256=(Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()}
        })
        $record.status = 'complete'
        $record.completed_at = [datetime]::UtcNow.ToString('o')
        Write-Manifest
        Write-Output "Portable ZIP: $portablePath"
    } catch {
        $record.error = $_.Exception.Message
        Write-Manifest
        throw
    }
} finally { Pop-Location }
