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
    $acceptance = Get-Content -LiteralPath 'research/audio-acceptance/artifacts/delivery-acceptance.json' -Raw | ConvertFrom-Json
    if ($acceptance.version -ne $version -or $acceptance.status -notin @('passed','passed_with_accepted_limitations') -or $acceptance.binary_sha256 -ne (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash.ToLowerInvariant()) { throw 'Executable and version must match verified hardware/UI build' }
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
        $guide = [System.IO.File]::ReadAllText("$taskRoot/docs/user-guide.md").Replace('../research/audio-acceptance/README.md','AUDIO-ACCEPTANCE.md')
        [System.IO.File]::WriteAllText("$staging/README.md",$guide,[System.Text.UTF8Encoding]::new($false))
        $hardware = [System.IO.File]::ReadAllText("$taskRoot/research/audio-acceptance/README.md").Replace('../../docs/audio-optimization.md','AUDIO-OPTIMIZATION.md')
        [System.IO.File]::WriteAllText("$staging/AUDIO-ACCEPTANCE.md",$hardware,[System.Text.UTF8Encoding]::new($false))
        $optimization = [System.IO.File]::ReadAllText("$taskRoot/docs/audio-optimization.md").Replace('../research/audio-acceptance/README.md','AUDIO-ACCEPTANCE.md')
        [System.IO.File]::WriteAllText("$staging/AUDIO-OPTIMIZATION.md",$optimization,[System.Text.UTF8Encoding]::new($false))
        $portablePath = "$destination/isMyMicThatBad-$tag-portable-x64.zip"
        Compress-Archive -Path "$staging/*" -DestinationPath "$portablePath.tmp.zip" -Force
        Move-Item -LiteralPath "$portablePath.tmp.zip" -Destination $portablePath -Force
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        $archive = [System.IO.Compression.ZipFile]::OpenRead($portablePath)
        try {
            foreach ($name in 'isMyMicThatBad.exe','portable.marker','msvcp140.dll','vcruntime140.dll','vcruntime140_1.dll','resources/vb-cable/VBCABLE_Setup_x64.exe','resources/THIRD-PARTY-NOTICES.txt','README.md','AUDIO-ACCEPTANCE.md','AUDIO-OPTIMIZATION.md') {
                $entry = $archive.Entries | Where-Object { $_.FullName.Replace('\','/') -eq $name }
                if (-not $entry -or ($name -ne 'portable.marker' -and $entry.Length -le 0)) { throw "Portable ZIP missing $name" }
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

Windows x64 portable voice changer with 11 filters and 7 presets. Gray-white frosted-glass application and tray icons match the audio workspace. Optional close-to-tray keeps audio processing active; click the tray icon or choose Show Window to return. The tray processing checkbox starts/stops the same audio controller as the window; failures restore the window with an explanation. Choose Exit to stop processing and quit. Old settings default to normal window-close behavior. First-party native source is maintained in src/native; build scripts generate the ignored local Tauri workspace.

Validation: Rust DSP/settings regressions; frontend tests and TypeScript; optimized Windows build; real WebView2 settings persistence and tray lifecycle checks. The DSP implementation is unchanged from v0.3.0; its hardware measurements are inherited, not remeasured for v0.3.2. The accepted female-preset underrun and untested long-term stability remain known limitations. See README.md and AUDIO-ACCEPTANCE.md inside the ZIP.

Portable ZIP only. This packaging script does not publish; the validated package is uploaded separately to the authorized GitHub Release.
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
