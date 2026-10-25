# Joshua+ installer for Windows.
#
#   irm https://raw.githubusercontent.com/fabandrade88/joshuaplus/main/scripts/install.ps1 | iex
#
# Downloads the latest release (or $env:JOSHUA_VERSION = "v0.5.4") from
# GitHub, verifies it against the release's SHA256SUMS and runs the installer.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$Repo = 'fabandrade88/joshuaplus'
$Version = $env:JOSHUA_VERSION

function Say($msg) { Write-Host "> $msg" -ForegroundColor Yellow }

if (-not [Environment]::Is64BitOperatingSystem) { throw 'Joshua+ needs 64-bit Windows.' }

$headers = @{ 'User-Agent' = 'joshua-plus-installer' }
# The release is found through github.com itself, not the REST API: the API
# allows 60 anonymous requests per hour per address, which shared networks
# (offices, schools, CI) run out of.
if ($Version) {
    $tag = $Version
} else {
    Say 'Looking up the latest release…'
    $resp = Invoke-WebRequest -UseBasicParsing -Headers $headers -Uri "https://github.com/$Repo/releases/latest"
    # Windows PowerShell 5.1 and PowerShell 7 expose the final URL differently.
    $final = if ($resp.BaseResponse.ResponseUri) { $resp.BaseResponse.ResponseUri.AbsoluteUri } else { $resp.BaseResponse.RequestMessage.RequestUri.AbsoluteUri }
    if ($final -notmatch '/tag/([^/?#]+)') { throw 'No published release found.' }
    $tag = $Matches[1]
}
$ver = $tag.TrimStart('v')
$assetName = "Joshua+_${ver}_x64-setup.exe"
$base = "https://github.com/$Repo/releases/download/$tag"
$asset = @{ name = $assetName; browser_download_url = "$base/" + $assetName.Replace('+', '%2B') }
$sums = @{ browser_download_url = "$base/SHA256SUMS" }

$tmp = Join-Path ([IO.Path]::GetTempPath()) ("joshua-" + [Guid]::NewGuid())
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
    $exe = Join-Path $tmp 'joshua-plus-setup.exe'
    Say "Downloading $($asset.name) ($tag)"
    Invoke-WebRequest -UseBasicParsing -Headers $headers -Uri $asset.browser_download_url -OutFile $exe
    $sumText = (Invoke-WebRequest -UseBasicParsing -Headers $headers -Uri $sums.browser_download_url).Content
    if ($sumText -is [byte[]]) { $sumText = [Text.Encoding]::UTF8.GetString($sumText) }

    $line = ($sumText -split "`n") | Where-Object { $_.Trim().EndsWith($asset.name) } | Select-Object -First 1
    if (-not $line) { throw "$($asset.name) is not listed in SHA256SUMS." }
    $expected = ($line.Trim() -split '\s+')[0].ToLowerInvariant()
    $actual = (Get-FileHash -Algorithm SHA256 $exe).Hash.ToLowerInvariant()
    if ($actual -ne $expected) { throw "Checksum mismatch (expected $expected, got $actual)." }
    Say 'Checksum verified'

    Say 'Installing (per-user, no admin needed)…'
    $p = Start-Process -FilePath $exe -ArgumentList '/S' -Wait -PassThru
    if ($p.ExitCode -ne 0) { throw "Installer exited with code $($p.ExitCode)." }
    Say 'Installed. Start Joshua+ from the Start menu.'
} finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
