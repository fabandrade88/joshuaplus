# Joshua+ installer for Windows.
#
#   irm https://raw.githubusercontent.com/fabandrade88/joshuaplus/main/scripts/install.ps1 | iex
#
# Downloads the latest release (or $env:JOSHUA_VERSION = "v0.0.1-beta") from
# GitHub, verifies it against the release's SHA256SUMS and runs the installer.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$Repo = 'fabandrade88/joshuaplus'
$Version = $env:JOSHUA_VERSION

function Say($msg) { Write-Host "> $msg" -ForegroundColor Yellow }

if (-not [Environment]::Is64BitOperatingSystem) { throw 'Joshua+ needs 64-bit Windows.' }

$headers = @{ Accept = 'application/vnd.github+json'; 'User-Agent' = 'joshua-plus-installer' }
if ($Version) {
    $release = Invoke-RestMethod -Headers $headers "https://api.github.com/repos/$Repo/releases/tags/$Version"
} else {
    # /releases/latest skips pre-releases, so take the newest release of any kind.
    $release = (Invoke-RestMethod -Headers $headers "https://api.github.com/repos/$Repo/releases?per_page=1")[0]
}
if (-not $release) { throw 'No release found.' }

$asset = $release.assets | Where-Object { $_.name -match '_x64-setup\.exe$' } | Select-Object -First 1
$sums = $release.assets | Where-Object { $_.name -eq 'SHA256SUMS' } | Select-Object -First 1
if (-not $asset) { throw 'This release has no Windows installer.' }
if (-not $sums) { throw 'Release has no SHA256SUMS; refusing to install unverified files.' }

$tmp = Join-Path ([IO.Path]::GetTempPath()) ("joshua-" + [Guid]::NewGuid())
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
    $exe = Join-Path $tmp 'joshua-plus-setup.exe'
    Say "Downloading $($asset.name) ($($release.tag_name))"
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
