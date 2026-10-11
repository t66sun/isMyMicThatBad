param([Parameter(Mandatory)][string]$Root)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$Root = [IO.Path]::GetFullPath($Root)
[void][IO.Directory]::CreateDirectory($Root)
$downloads = Join-Path $Root 'downloads'
[void][IO.Directory]::CreateDirectory($downloads)
$revision = '81eed5e8f68b6bed1789f682fe78cdd324495afc'

function Download([string]$Url, [string]$Path, [string]$Sha256) {
    if ((Test-Path -LiteralPath $Path) -and (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash -eq $Sha256) { return }
    [void][IO.Directory]::CreateDirectory((Split-Path -Parent $Path))
    Write-Output "Downloading $(Split-Path -Leaf $Path)"
    Invoke-WebRequest -UseBasicParsing -Uri $Url -OutFile "$Path.part"
    if ((Get-FileHash -LiteralPath "$Path.part" -Algorithm SHA256).Hash -ne $Sha256) { throw "Checksum mismatch: $Path" }
    Move-Item -LiteralPath "$Path.part" -Destination $Path -Force
}
function Python([string[]]$Arguments) {
    & "$Root/python/python.exe" -I @Arguments
    if ($LASTEXITCODE -ne 0) { throw "Python step failed ($LASTEXITCODE)" }
}

Download 'https://www.python.org/ftp/python/3.12.10/python-3.12.10-embed-amd64.zip' "$downloads/python.zip" '4ACBED6DD1C744B0376E3B1CF57CE906F9DC9E95E68824584C8099A63025A3C3'
if (-not (Test-Path -LiteralPath "$Root/python/python.exe")) { Expand-Archive -LiteralPath "$downloads/python.zip" -DestinationPath "$Root/python" }
[IO.File]::WriteAllText("$Root/python/python312._pth", "python312.zip`n.`nLib/site-packages`nimport site`n")
Download 'https://files.pythonhosted.org/packages/29/a2/d40fb2460e883eca5199c62cfc2463fd261f760556ae6290f88488c362c0/pip-25.1.1-py3-none-any.whl' "$downloads/pip.whl" '2913A38A2ABF4EA6B64AB507BD9E967F3B53DC1EDE74B01B0931E1CE548751AF'
if (-not (Test-Path -LiteralPath "$Root/python/Lib/site-packages/pip")) {
    Python @('-c', 'import sys,zipfile; zipfile.ZipFile(sys.argv[1]).extractall(sys.argv[2])', "$downloads/pip.whl", "$Root/python/Lib/site-packages")
}

Download "https://codeload.github.com/RVC-Project/Retrieval-based-Voice-Conversion-WebUI/zip/$revision" "$downloads/rvc.zip" 'C0427564FBEB584D1A4805A5E71B5897132A41B9029A0FA9F417497A6DF13E43'
if (-not (Test-Path -LiteralPath "$Root/rvc/infer/rtrvc.py")) {
    Expand-Archive -LiteralPath "$downloads/rvc.zip" -DestinationPath "$downloads/source" -Force
    Move-Item -LiteralPath "$downloads/source/Retrieval-based-Voice-Conversion-WebUI-$revision" -Destination "$Root/rvc"
}
Write-Output 'Installing isolated CUDA PyTorch and inference dependencies'
Python @('-m','pip','install','--disable-pip-version-check','--only-binary=:all:','--no-warn-script-location','--cache-dir',"$downloads/pip",'torch==2.7.1+cu118','torchaudio==2.7.1+cu118','--index-url','https://download.pytorch.org/whl/cu118','--extra-index-url','https://pypi.org/simple')
Python @('-m','pip','install','--disable-pip-version-check','--only-binary=:all:','--no-warn-script-location','--cache-dir',"$downloads/pip",'numpy==1.26.4','librosa==0.10.2.post1','scipy==1.13.1','transformers==4.49.0','faiss-cpu==1.13.0','praat-parselmouth==0.4.6','einops==0.8.1')

$base = 'https://huggingface.co/lj1995/VoiceConversionWebUI/resolve/e6d0c1a17da07c33557852f9dfa2bd44cc75737d'
Download "$base/hubert_base/config.json" "$Root/rvc/assets/hubert_base/config.json" '0346950779DFB7F9316FA74ED846E2B8A22A08EEDFDC5387B73F327CB1A4A7CF'
Download "$base/hubert_base/preprocessor_config.json" "$Root/rvc/assets/hubert_base/preprocessor_config.json" '7C1976A680FB7ACC757CD36FB08EEF878FA36C70B4C9D2D595DF9C608BBBBF0E'
Download "$base/hubert_base/pytorch_model.bin" "$Root/rvc/assets/hubert_base/pytorch_model.bin" 'CC8C20F4B90A520757260197A3FF2505705A7ADBD20AD9EEAA4E1A9B38442EF5'
Download "$base/rmvpe.pt" "$Root/rvc/assets/rmvpe/rmvpe.pt" '6D62215F4306E3CA278246188607209F09AF3DC77ED4232EFDD069798C4EC193'
$voice = 'https://huggingface.co/Saisho-Utane/Saisho-Model-RVC/resolve/7a61b17e4f014fc5013a489f2b5cd3a7ee9e2b1a'
Download "$voice/model.pth" "$Root/models/saisho.pth" '82312A29A3996A451AAA3D3776566CEC85B47B5414FCE53DA82B2D66B04F3D35'
Download "$voice/model.index" "$Root/models/saisho.index" 'E9F7396E5DB3077274C720D055E43ABB8F1EAC5609CBDF48B198CA4F4A35CAD5'
[IO.File]::WriteAllText("$Root/models/NOTICE.txt", "Voice model created by Saisho Utane - https://huggingface.co/Saisho-Utane . Used under the CC BY 4.0 license. Model: https://huggingface.co/Saisho-Utane/Saisho-Model-RVC`n", [Text.UTF8Encoding]::new($false))
Python @('-m','pip','check')
Python @('-c','import sys,torch,torchaudio,numpy,librosa,faiss,parselmouth,transformers; assert sys.version_info[:3] == (3,12,10); assert torch.__version__ == "2.7.1+cu118"; print("Runtime imports verified")')
Python @('-m','pip','freeze') | Set-Content -LiteralPath "$Root/packages.txt" -Encoding UTF8
[IO.File]::WriteAllText("$Root/ready.json", ('{"revision":"' + $revision + '","python":"3.12.10","torch":"2.7.1+cu118"}'))
Write-Output 'Offline runtime prepared'
