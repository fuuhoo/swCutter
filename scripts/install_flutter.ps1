# Installs the latest stable Flutter SDK for Windows into H:\dev\flutter
# Endpoint priority: Tencent Cloud mirror (fast, verified ~22MB/s)
#   > Tsinghua TUNA > googleapis. Fresh download each run. Idempotent.
$ErrorActionPreference = 'Stop'
$dest = 'H:\dev'
$zip = Join-Path $dest 'flutter_stable.zip'
New-Item -ItemType Directory -Force -Path $dest | Out-Null

# Mirror env vars: used by flutter bootstrap (Dart SDK) and later pub get,
# persisted for User scope AND set for this session.
$flutterStorage = 'https://mirrors.cloud.tencent.com/flutter'
$pubHosted = 'https://pub.flutter-io.cn'
[Environment]::SetEnvironmentVariable('FLUTTER_STORAGE_BASE_URL', $flutterStorage, 'User')
[Environment]::SetEnvironmentVariable('PUB_HOSTED_URL', $pubHosted, 'User')
$env:FLUTTER_STORAGE_BASE_URL = $flutterStorage
$env:PUB_HOSTED_URL = $pubHosted

$ok = $false
foreach ($ep in @(
    'https://mirrors.cloud.tencent.com/flutter/flutter_infra_release/releases',
    'https://storage.googleapis.com/flutter_infra_release/releases'
)) {
    try {
        Write-Host "== Fetching release metadata from $ep ..."
        $meta = Invoke-RestMethod "$ep/releases_windows.json" -TimeoutSec 60
        $stableHash = $meta.current_release.stable
        $rel = $meta.releases |
            Where-Object { $_.hash -eq $stableHash -and $_.channel -eq 'stable' } |
            Select-Object -First 1
        if (-not $rel) { throw 'no stable release found' }
        $archive = $rel.archive
        # Some mirrors return archive paths without the leading "releases/" segment.
        if ($archive -notlike 'releases/*') { $archive = "releases/$archive" }
        $host_root = switch -Wildcard ($ep) {
            '*tencent*' { 'https://mirrors.cloud.tencent.com/flutter/flutter_infra_release' }
            '*tuna*'    { 'https://mirror.tuna.tsinghua.edu.cn/flutter/flutter_infra_release' }
            default     { $meta.base_url }
        }
        $url = "$host_root/$archive"
        # NOTE: no Range requests against tencent CDN (unsupported); fresh download.
        if (Test-Path $zip) { Remove-Item $zip -Force }
        Write-Host "== Downloading: $url"
        & curl.exe -L --fail `
            --retry 5 --retry-all-errors -C - `
            --silent --show-error -o $zip $url
        if ($LASTEXITCODE -ne 0) { throw "curl exit code $LASTEXITCODE" }
        if ((Get-Item $zip).Length -lt 500MB) { throw 'downloaded file too small' }
        $ok = $true
        break
    }
    catch {
        Write-Warning "Endpoint failed ($ep): $_"
    }
}
if (-not $ok) { throw 'All download endpoints failed.'; exit 1 }

Write-Host ("== Download complete: {0:N0} MB" -f ((Get-Item $zip).Length / 1MB))
Write-Host '== Extracting (this can take a few minutes) ...'
if (Test-Path 'H:\dev\flutter') { Remove-Item -Recurse -Force 'H:\dev\flutter' }
Expand-Archive -Path $zip -DestinationPath $dest -Force
Remove-Item $zip -Force

$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if ($userPath -notlike '*H:\dev\flutter\bin*') {
    [Environment]::SetEnvironmentVariable('Path', "$userPath;H:\dev\flutter\bin", 'User')
    Write-Host '== Added H:\dev\flutter\bin to user PATH'
}

Write-Host '== flutter --version (first run bootstraps Dart SDK via mirror) ...'
& 'H:\dev\flutter\bin\flutter.bat' --version
if ($LASTEXITCODE -ne 0) { throw 'flutter bootstrap failed' }
Write-Host '== DONE =='
