# Installs the latest stable Flutter SDK for Windows into H:\dev\flutter
# Tries googleapis first, falls back to Tsinghua mirror. Idempotent.
$ErrorActionPreference = 'Stop'
$dest = 'H:\dev'
$zip = Join-Path $dest 'flutter_stable.zip'
New-Item -ItemType Directory -Force -Path $dest | Out-Null

$ok = $false
foreach ($ep in @(
    'https://storage.googleapis.com/flutter_infra_release/releases',
    'https://mirror.tuna.tsinghua.edu.cn/flutter/flutter_infra_release/releases'
)) {
    try {
        Write-Host "== Fetching release metadata from $ep ..."
        $meta = Invoke-RestMethod "$ep/releases_windows.json" -TimeoutSec 60
        $stableHash = $meta.current_release.stable
        $rel = $meta.releases |
            Where-Object { $_.hash -eq $stableHash -and $_.channel -eq 'stable' } |
            Select-Object -First 1
        if (-not $rel) { throw 'no stable release found' }
        $archive = $rel.archive  # e.g. releases/stable/windows/flutter_windows_x.y.z-stable.zip
        if ($ep -like '*tuna*') {
            $url = "https://mirror.tuna.tsinghua.edu.cn/flutter/flutter_infra_release/$archive"
        }
        else {
            $url = "$($meta.base_url)/$archive"
        }
        Write-Host "== Downloading: $url"
        & curl.exe -L --fail --retry 3 --silent --show-error -o $zip $url
        if ($LASTEXITCODE -ne 0) { throw "curl exit code $LASTEXITCODE" }
        if ((Get-Item $zip).Length -lt 100MB) { throw 'downloaded file too small' }
        $ok = $true
        break
    }
    catch {
        Write-Warning "Endpoint failed ($ep): $_"
        if (Test-Path $zip) { Remove-Item $zip -Force -ErrorAction SilentlyContinue }
    }
}
if (-not $ok) { throw 'All download endpoints failed.'; exit 1 }

Write-Host '== Extracting (this can take a few minutes) ...'
if (Test-Path 'H:\dev\flutter') { Remove-Item -Recurse -Force 'H:\dev\flutter' }
Expand-Archive -Path $zip -DestinationPath $dest -Force
Remove-Item $zip -Force

# Persist to user PATH for future shells; current session uses explicit path.
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if ($userPath -notlike '*H:\dev\flutter\bin*') {
    [Environment]::SetEnvironmentVariable('Path', "$userPath;H:\dev\flutter\bin", 'User')
    Write-Host '== Added H:\dev\flutter\bin to user PATH'
}

Write-Host '== flutter --version (first run bootstraps Dart SDK) ...'
& 'H:\dev\flutter\bin\flutter.bat' --version
Write-Host '== DONE =='
