# Builds stepmaster.exe, signs it for the self-updater and (optionally) publishes a GitHub release.
#   npm run release                      -> build + sign current version
#   powershell -File scripts/release.ps1 -Version 1.0.1 -Publish   -> bump, build, sign, upload
param(
  [string]$Version = "",
  [switch]$Publish
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
$key = "$env:USERPROFILE\.stepmaster-keys\updater.key"

if ($Version) {
  if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw "Version must look like 1.2.3" }
  # Bump the version in all three places.
  # Read and write as UTF-8 without BOM (Windows PowerShell 5.1 defaults would break non-ASCII text).
  $utf8 = New-Object System.Text.UTF8Encoding($false)
  function Set-Version($file, $pattern, $replacement) {
    $path = Join-Path $root $file
    $text = [IO.File]::ReadAllText($path, $utf8)
    $new = [regex]::Replace($text, $pattern, $replacement, 1)
    [IO.File]::WriteAllText($path, $new, $utf8)
  }
  Set-Version "src-tauri\Cargo.toml" '(?m)^version = "[^"]+"' "version = `"$Version`""
  Set-Version "src-tauri\tauri.conf.json" '"version": "[^"]+"' "`"version`": `"$Version`""
  Set-Version "package.json" '"version": "[^"]+"' "`"version`": `"$Version`""
}
$ver = ((Get-Content src-tauri\Cargo.toml) | Select-String '^version = "(.+)"').Matches[0].Groups[1].Value
Write-Host "Building StepMaster $ver ..."

npx tauri build --no-bundle
if ($LASTEXITCODE -ne 0) { throw "Build failed" }

$name = "StepMaster.exe"
$exe = Join-Path $root "src-tauri\target\release\stepmaster.exe"
$out = Join-Path $root "release"
New-Item -ItemType Directory -Force $out | Out-Null
# Windows keeps the old letter case when overwriting, so remove old copies first.
foreach ($f in @((Join-Path $out $name), (Join-Path $out "$name.sig"), (Join-Path $root $name))) {
  if (Test-Path $f) { Remove-Item $f -Force }
}
Copy-Item $exe (Join-Path $out $name)

if (Test-Path $key) {
  $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ""
  npx tauri signer sign -f $key (Join-Path $out $name)
  if ($LASTEXITCODE -ne 0) { throw "Signing failed" }
} else {
  Write-Warning "No signing key at $key - the self-updater will reject this build."
}

# The ready-to-use copy lives in the project root.
Copy-Item (Join-Path $out $name) (Join-Path $root $name)
Write-Host "Done: $root\$name"

if ($Publish) {
  $repo = (Select-String -Path src-tauri\src\updater.rs -Pattern 'pub const REPO: &str = "([^"]*)"').Matches[0].Groups[1].Value
  if (-not $repo) { throw "Set REPO in src-tauri/src/updater.rs first" }
  gh release create "v$ver" (Join-Path $out $name) (Join-Path $out "$name.sig") --repo $repo --title "StepMaster $ver" --notes-file (Join-Path $root "scripts\release-notes.md")
  if ($LASTEXITCODE -ne 0) { throw "Publishing failed" }
  Write-Host "Published v$ver to $repo"
}
