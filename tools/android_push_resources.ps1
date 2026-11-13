<#
.SYNOPSIS
Copies Horizon's prepared resources from this PC to an Android device (docs/android.md).

.DESCRIPTION
The resources come from your own Minecraft: Java Edition and need Java to prepare, so they are
made on a PC (by the Horizon launcher, or horizon-prepare) and pushed over adb. This packs them
into one archive, pushes it, and unpacks it into the app's folder on shared storage:
/sdcard/Android/data/dev.playhorizon.android/files/resources.

By default it takes the newest prepared version the launcher installed
(%APPDATA%\horizon-launcher\versions\<tag>\.local\horizon-client\26.3).

.PARAMETER Resources
A prepared resources folder (the one holding horizon-resources.json).

.PARAMETER Serial
The device's adb serial, when more than one is connected.

.PARAMETER Name
Also writes horizon-android.txt with this offline player name.

.EXAMPLE
pwsh tools/android_push_resources.ps1 -Name Steve
#>
param(
    [string]$Resources,
    [string]$Serial,
    [string]$Name
)
$ErrorActionPreference = 'Stop'
$Package = 'dev.playhorizon.android'
$Remote = "/sdcard/Android/data/$Package/files"

function Find-Adb {
    $onPath = Get-Command adb -ErrorAction SilentlyContinue
    if ($onPath) { return $onPath.Source }
    foreach ($sdk in @($env:ANDROID_HOME, $env:ANDROID_SDK_ROOT, "$env:LOCALAPPDATA\Android\Sdk")) {
        if ($sdk -and (Test-Path "$sdk\platform-tools\adb.exe")) { return "$sdk\platform-tools\adb.exe" }
    }
    throw 'adb not found: install Android SDK Platform-Tools (Android Studio > SDK Manager).'
}

# adb prints its progress on stderr, which Windows PowerShell turns into errors under 'Stop':
# judge adb by its exit code instead.
function Invoke-Adb {
    $saved = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try { & $script:adb @script:adbArgs @args 2>&1 | ForEach-Object { "$_" } }
    finally { $ErrorActionPreference = $saved }
}

if (-not $Resources) {
    $versions = Join-Path $env:APPDATA 'horizon-launcher\versions'
    $found = Get-ChildItem $versions -Directory -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending |
        ForEach-Object { Get-ChildItem (Join-Path $_.FullName '.local\horizon-client') -Directory -ErrorAction SilentlyContinue } |
        Where-Object { Test-Path (Join-Path $_.FullName 'horizon-resources.json') } |
        Select-Object -First 1
    if (-not $found) { throw "No prepared resources under $versions; play once with the Horizon launcher, or pass -Resources." }
    $Resources = $found.FullName
}
if (-not (Test-Path (Join-Path $Resources 'horizon-resources.json'))) {
    throw "$Resources has no horizon-resources.json: not a prepared resources folder."
}

$adb = Find-Adb
$adbArgs = @()
if ($Serial) { $adbArgs += @('-s', $Serial) }
$devices = (Invoke-Adb devices) | Select-Object -Skip 1 | Where-Object { $_ -match '\tdevice$' }
if (-not $devices) { throw 'No device: connect it by USB and allow USB debugging (Developer options).' }

Write-Host "Resources: $Resources"
$archive = Join-Path ([IO.Path]::GetTempPath()) 'horizon-resources.tar'
try {
    Write-Host 'Packing...'
    tar -cf $archive -C $Resources .
    if ($LASTEXITCODE -ne 0) { throw 'tar failed' }
    $size = [math]::Round((Get-Item $archive).Length / 1MB)
    Write-Host "Pushing $size MB..."
    Invoke-Adb shell "mkdir -p $Remote/resources.new" | Out-Null
    Invoke-Adb push $archive "$Remote/resources.tar"
    if ($LASTEXITCODE -ne 0) { throw 'adb push failed' }
    Write-Host 'Unpacking on the device...'
    # Into a new folder first: an interrupted unpack keeps the old resources.
    Invoke-Adb shell "cd $Remote/resources.new && tar -xf ../resources.tar && cd .. && rm -rf resources && mv resources.new resources && rm resources.tar"
    if ($LASTEXITCODE -ne 0) { throw 'unpacking on the device failed' }
} finally {
    Remove-Item $archive -ErrorAction SilentlyContinue
}

if ($Name) {
    $config = Join-Path ([IO.Path]::GetTempPath()) 'horizon-android.txt'
    "# Horizon for Android (docs/android.md)`nname=$Name`n" | Set-Content -NoNewline -Encoding ascii $config
    Invoke-Adb push $config "$Remote/horizon-android.txt" | Out-Null
    Remove-Item $config
    Write-Host "Player name: $Name"
}
Write-Host "Done: $Remote/resources"
