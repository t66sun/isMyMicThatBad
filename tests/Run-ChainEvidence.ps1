param(
    [Parameter(Mandatory)][string]$TestExe,
    [Parameter(Mandatory)][string]$SessionPath,
    [Parameter(Mandatory)][string]$AiRoot,
    [Parameter(Mandatory)][string]$EvidenceDirectory,
    [Parameter(Mandatory)][string]$StimulusWav,
    [string]$SpeakerId,
    [Parameter(Mandatory)][string]$CableReaderId,
    [ValidateSet('six-on','six-off','rvc','dsp','digital','preset')][string]$Scenario = 'six-on',
    [ValidateRange(1,5)][int]$Repeats = 1,
    [ValidateRange(1,1800)][int]$Seconds = 90,
    [switch]$SilentReplay
)
$ErrorActionPreference = 'Stop'
if (!$SilentReplay -and !$SpeakerId) { throw 'Select the stimulus output endpoint' }
if ($SilentReplay -and $Scenario -eq 'digital') { throw 'Silent replay uses the selected capture clock' }
$repo = Split-Path -Parent $PSScriptRoot
$exe = (Resolve-Path -LiteralPath $TestExe).Path
$evidence = (Resolve-Path -LiteralPath $EvidenceDirectory).Path
$source = (Resolve-Path -LiteralPath $AiRoot).Path
$targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { "$repo/src/app/src-tauri/target" }
$runtime = Join-Path $targetDir ("audio-evidence-runtime-" + (Split-Path -Leaf $evidence))
# The production service writes worker.py and worker.log beside the runtime.
# Keep package contents and historical evidence intact, including Python caches.
if (!(Test-Path -LiteralPath "$runtime/ready.json")) {
    [void][IO.Directory]::CreateDirectory($runtime)
    foreach ($part in 'models','python','rvc') {
        & robocopy.exe "$source/$part" "$runtime/$part" /E /R:1 /W:1 /NFL /NDL /NJH /NJS /NP
        if ($LASTEXITCODE -ge 8) { throw "Could not stage $part" }
    }
    Copy-Item -LiteralPath "$source/ready.json" -Destination "$runtime/ready.json"
}
$session = Get-Content -LiteralPath $SessionPath -Raw -Encoding UTF8 | ConvertFrom-Json
if ($Scenario -eq 'preset') {
    if (!$session.chain.Count -or 'rvc' -in $session.chain.type) { throw 'Select an ordinary DSP preset' }
} elseif (($session.chain.type -join ',') -ne 'rvc,highpass,distortion,gain,bitcrusher,frequency') { throw 'Expected the saved six-card chain' }
if ($session.bufferMs -ne 5 -or @($session.chain | Where-Object { !$_.enabled }).Count) { throw 'Expected enabled cards and a 5 ms buffer' }
$session.monitoring = $Scenario -in 'six-on','rvc','dsp','preset'
switch ($Scenario) {
    'six-off' { $session.monitoring = $false }
    'rvc' { $session.chain = @($session.chain | Where-Object type -eq 'rvc') }
    'dsp' { $session.chain = @($session.chain | Where-Object type -ne 'rvc') }
    'digital' { $session.inputId = 'loopback:' + $SpeakerId; $session.monitoring = $false }
}
$names = 'MIC_TEST_RUN_DIR','MIC_TEST_SESSION','MIC_TEST_AI_ROOT','MIC_TEST_STIMULUS_WAV','MIC_TEST_SECONDS','MIC_TEST_SPEAKER_ID','MIC_TEST_CABLE_READER_ID'
$saved = @{}
foreach ($name in $names) { $saved[$name] = [Environment]::GetEnvironmentVariable($name) }
try {
    $env:MIC_TEST_AI_ROOT = $runtime
    $env:MIC_TEST_STIMULUS_WAV = (Resolve-Path -LiteralPath $StimulusWav).Path
    $env:MIC_TEST_SECONDS = "$Seconds"
    $env:MIC_TEST_SPEAKER_ID = if ($SilentReplay) { 'no-playback' } else { $SpeakerId }
    $env:MIC_TEST_CABLE_READER_ID = $CableReaderId
    $sha = (Get-FileHash -LiteralPath $exe).Hash
    $stimulusHash = (Get-FileHash -LiteralPath $env:MIC_TEST_STIMULUS_WAV).Hash
    $modelHash = $null
    $indexHash = $null
    $card = $session.chain | Where-Object type -eq 'rvc'
    if ($card) {
        $modelPath = if ($card.model.path) { $card.model.path } else { "$runtime/models/saisho.pth" }
        $indexPath = if ($card.model) { $card.model.index } else { "$runtime/models/saisho.index" }
        $modelHash = (Get-FileHash -LiteralPath $modelPath).Hash
        if ($indexPath) { $indexHash = (Get-FileHash -LiteralPath $indexPath).Hash }
    }
    $trials = @()
    for ($i=1; $i -le $Repeats; $i++) {
        $route = if ($SilentReplay) { "$Scenario-replay" } else { $Scenario }
        $dir = Join-Path $evidence ("{0}-{1}-{2}s-{3}" -f (Get-Date -Format 'yyyyMMdd-HHmmssfff'),$route,$Seconds,$i)
        [void](New-Item -ItemType Directory -Path $dir)
        $env:MIC_TEST_RUN_DIR = $dir
        $env:MIC_TEST_SESSION = "$dir/session.json"
        $encoding = [Text.UTF8Encoding]::new($false)
        [IO.File]::WriteAllText($env:MIC_TEST_SESSION, ($session | ConvertTo-Json -Depth 12), $encoding)
        $manifest = @{
            start_utc = [DateTime]::UtcNow.ToString('o')
            test_sha256 = $sha
            scenario = $Scenario
            capture_clocked_replay = $SilentReplay.IsPresent
            seconds = $Seconds
            sample_rate = 48000
            buffer_ms = $session.bufferMs
            monitoring = $session.monitoring
            speaker = $SpeakerId
            cable_reader = $CableReaderId
            stimulus = $env:MIC_TEST_STIMULUS_WAV
            stimulus_sha256 = $stimulusHash
            model_sha256 = $modelHash
            index_sha256 = $indexHash
            runtime_revision = (Get-Content -Raw "$runtime/ready.json" | ConvertFrom-Json).revision
        }
        [IO.File]::WriteAllText("$dir/manifest.json", ($manifest | ConvertTo-Json), $encoding)
        Write-Host "Starting $dir"
        $began = [DateTime]::UtcNow
        $gpu = $null
        try {
            if (Get-Command nvidia-smi.exe -ErrorAction SilentlyContinue) {
                $gpu = Start-Process -FilePath 'nvidia-smi.exe' -ArgumentList '--query-gpu=timestamp,pstate,utilization.gpu,clocks.sm,temperature.gpu,memory.used,power.draw','--format=csv','-l','1' -WindowStyle Hidden -PassThru -RedirectStandardOutput "$dir/gpu.csv" -RedirectStandardError "$dir/gpu-errors.log"
            }
            $test = if ($SilentReplay) { 'evidence::silent_fixed_chain' } else { 'evidence::fixed_chain' }
            & $exe --ignored --exact $test --nocapture *> "$dir/test.log"
            $exitCode = $LASTEXITCODE
        } finally {
            if ($gpu -and !$gpu.HasExited) { $gpu.Kill(); $gpu.WaitForExit() }
            if ($gpu) { $gpu.Dispose() }
        }
        if ((Test-Path -LiteralPath "$runtime/worker.log") -and (Get-Item "$runtime/worker.log").LastWriteTimeUtc -ge $began) {
            Copy-Item -LiteralPath "$runtime/worker.log" -Destination "$dir/worker.log"
        }
        if (!(Test-Path -LiteralPath "$dir/result.json")) {
            [IO.File]::WriteAllText("$dir/result.json", (@{outcome='harness_error';exit_code=$exitCode} | ConvertTo-Json), $encoding)
        }
        $result = Get-Content -LiteralPath "$dir/result.json" -Raw -Encoding UTF8 | ConvertFrom-Json
        $trials += [pscustomobject]@{directory=$dir;outcome=$result.outcome;exit_code=$exitCode}
        Write-Host "$Scenario attempt $i -> $($result.outcome)"
    }
    $trials
    if (@($trials | Where-Object { $_.outcome -ne 'stable' -or $_.exit_code -ne 0 }).Count) {
        throw "One or more $Scenario trials did not qualify. See $evidence"
    }
} finally {
    foreach ($name in $names) { [Environment]::SetEnvironmentVariable($name,$saved[$name]) }
}
