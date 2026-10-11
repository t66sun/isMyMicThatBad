param([string]$StimulusWavPath, [switch]$PhysicalMicrophone)
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$artifacts = Join-Path $repoRoot 'research/audio-acceptance/artifacts'
[void][IO.Directory]::CreateDirectory($artifacts)
. "$repoRoot/scripts/Use-Toolchain.ps1"
& "$repoRoot/scripts/Initialize-Native.ps1"

if (-not $StimulusWavPath) {
    $StimulusWavPath = Join-Path $artifacts 'acceptance-stimulus.wav'
    if (-not (Test-Path -LiteralPath $StimulusWavPath)) {
        Add-Type -AssemblyName System.Speech
        $synth = [System.Speech.Synthesis.SpeechSynthesizer]::new()
        try {
            $format = [System.Speech.AudioFormat.SpeechAudioFormatInfo]::new(48000,
                [System.Speech.AudioFormat.AudioBitsPerSample]::Sixteen,
                [System.Speech.AudioFormat.AudioChannel]::Mono)
            $synth.SetOutputToWaveFile($StimulusWavPath, $format)
            $synth.Speak('This is a microphone audio acceptance test. The quick brown fox jumps over the lazy dog. We are checking speech clarity, audio processing, and end to end delay.')
        } finally { $synth.Dispose() }
    }
}
$StimulusWavPath = (Resolve-Path -LiteralPath $StimulusWavPath).Path
$nativeExe = Join-Path $repoRoot 'src/app/src-tauri/target/release/ismymicthatbad.exe'
if (-not (Test-Path -LiteralPath $nativeExe)) { throw 'Build the release application with scripts/Build.ps1 first.' }
$devices = (& $nativeExe --devices | ConvertFrom-Json)
if ($LASTEXITCODE -ne 0) { throw 'Audio device enumeration failed.' }
$microphones = @($devices | Where-Object { $_.input -and $_.name -match 'Intel.*Microphone' })
$speakers = @($devices | Where-Object { -not $_.input -and $_.name -match 'Realtek' })
if ($microphones.Count -ne 1 -or $speakers.Count -ne 1) { throw 'This hardware check requires one Intel array microphone and one Realtek speaker output.' }

$presetsPath = Join-Path $artifacts 'acceptance-original-presets.json'
& "$repoRoot/src/app/node_modules/.bin/tsx.cmd" "$repoRoot/tests/export-presets.ts" $presetsPath
if ($LASTEXITCODE -ne 0) { throw 'Preset export failed.' }
$presets = Get-Content -LiteralPath $presetsPath -Raw -Encoding UTF8 | ConvertFrom-Json
if ($presets.Count -ne 4) { throw 'Expected four shared acceptance presets.' }
$newPresets = @($presets[0..1])
$catalog = Get-Content "$repoRoot/src/app/src/filters.json" -Raw -Encoding UTF8 | ConvertFrom-Json
foreach ($definition in $catalog) {
    if ($definition.type -notin 'bitcrusher','tremolo') { continue }
    $parameters = @{}
    foreach ($parameter in $definition.parameters) { $parameters[$parameter.key] = $parameter.default }
    $newPresets += @{ name=$definition.name; chain=@(@{id="acceptance-$($definition.type)"; type=$definition.type; enabled=$true; params=$parameters}) }
}
$newPresetsPath = Join-Path $artifacts 'acceptance-new-filters.json'
[IO.File]::WriteAllText($newPresetsPath, (ConvertTo-Json -InputObject $newPresets -Depth 10), [Text.UTF8Encoding]::new($false))
$manifest = Join-Path $repoRoot 'src/app/src-tauri/Cargo.toml'
$log = Join-Path $artifacts ('audio-acceptance-' + (Get-Date -Format 'yyyyMMdd-HHmmss') + '.log')
function Run-Test([string]$testName) {
    Write-Host "Running $testName; WAV plays through the Realtek speakers."
    # PowerShell 5.1 represents native stderr as error records; cargo's exit code decides success.
    $ErrorActionPreference = 'Continue'
    & cargo test --manifest-path $manifest --release --features custom-protocol --test hardware_latency $testName -- --ignored --exact --nocapture 2>&1 | ForEach-Object { $_.ToString() } | Tee-Object -FilePath $log -Append
    if ($LASTEXITCODE -ne 0) { throw "$testName failed; see $log" }
}
$variables = @('MIC_TEST_INPUT_ID','MIC_TEST_SPEAKER_ID','MIC_TEST_STIMULUS_WAV','MIC_TEST_PRESETS_FILE','MIC_TEST_SECONDS')
$saved = @{}
foreach ($name in $variables) { $saved[$name] = [Environment]::GetEnvironmentVariable($name) }
try {
    $env:MIC_TEST_INPUT_ID = if ($PhysicalMicrophone) { $microphones[0].id } else { 'loopback:' + $speakers[0].id }
    $env:MIC_TEST_SPEAKER_ID = $speakers[0].id
    $env:MIC_TEST_STIMULUS_WAV = $StimulusWavPath
    Write-Host "Calibration input: $($env:MIC_TEST_INPUT_ID). Loopback results cover the software and VB-CABLE path; physical microphone capture is separate."
    if ($PhysicalMicrophone) {
        $rawTest = 'microphone_to_cable_p95_is_under_150ms'
        $presetTest = 'new_presets_microphone_to_cable_p95_is_under_200ms'
    } else {
        $rawTest = 'wav_loopback_to_cable_p95_is_under_200ms'
        $presetTest = 'wav_presets_to_cable_p95_is_under_250ms'
    }
    Run-Test $rawTest
    $env:MIC_TEST_PRESETS_FILE = $presetsPath
    Run-Test $presetTest
    $env:MIC_TEST_PRESETS_FILE = $newPresetsPath
    Run-Test $presetTest
    $env:MIC_TEST_PRESETS_FILE = $presetsPath
    $env:MIC_TEST_SECONDS = '60'
    Run-Test 'female_preset_runs_without_underruns'
    Write-Host "Audio acceptance passed. Log: $log"
} finally {
    foreach ($name in $variables) { [Environment]::SetEnvironmentVariable($name, $saved[$name]) }
}
