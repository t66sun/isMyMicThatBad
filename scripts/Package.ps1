. "$PSScriptRoot/Use-Toolchain.ps1"
Push-Location $taskRoot
try {
    $commit = (git rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0) { throw 'Commit validated source before packaging' }
    git diff --quiet HEAD
    if ($LASTEXITCODE -ne 0) { throw 'Source differs from recorded commit' }
    if (git ls-files --others --exclude-standard) { throw 'Uncommitted source remains' }
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
    if ($acceptance.status -ne 'passed' -or $acceptance.binary_sha256 -ne (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash.ToLowerInvariant()) { throw 'Executable must match verified hardware/UI build' }
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
            foreach ($name in 'isMyMicThatBad.exe','portable.marker','resources/vb-cable/VBCABLE_Setup_x64.exe','resources/THIRD-PARTY-NOTICES.txt','README.md','AUDIO-ACCEPTANCE.md','AUDIO-OPTIMIZATION.md') {
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
        $sourceRecords = foreach ($path in git ls-files -- .gitignore .gitattributes AGENTS.md VERSION src scripts tests docs research) {
            "$((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant())  $path"
        }
        [System.IO.File]::WriteAllLines("$destination/SOURCE-SHA256.txt",$sourceRecords,[System.Text.UTF8Encoding]::new($false))
        $changelog = @"
# $tag

Commit: $commit

Windows x64 portable voice changer: card-based chains, five filters, presets, a compiled custom filter template, VB-CABLE routing and optional monitoring.

Validation: Rust DSP/settings regressions and calibration; frontend tests and TypeScript; optimized Windows build; real WebView2 persistence/routing/live edits; p95 <=150 ms hardware acceptance. See README.md and AUDIO-ACCEPTANCE.md inside the ZIP for measured results and limitations.

No application installer, Git tag or GitHub Release is created.
"@
        [System.IO.File]::WriteAllText("$destination/CHANGELOG.md",$changelog,[System.Text.UTF8Encoding]::new($false))
        $record.files = @(foreach ($path in $portablePath,"$destination/CHANGELOG.md","$destination/SOURCE-SHA256.txt") {
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
