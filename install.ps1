$ErrorActionPreference = "Stop"

$Repo = "awssat/shai"
$BaseUrl = "https://github.com/$Repo/releases/latest/download"
$Artifact = "shai-windows-x86_64.zip"

$TmpDir = Join-Path $env:TEMP "shai-install"
New-Item -ItemType Directory -Force -Path $TmpDir | Out-Null

Write-Host "Downloading $Artifact..."
$ZipPath = Join-Path $TmpDir $Artifact
Invoke-WebRequest -Uri "$BaseUrl/$Artifact" -OutFile $ZipPath

Expand-Archive -Path $ZipPath -DestinationPath $TmpDir -Force

$InstallDir = "$env:LOCALAPPDATA\Programs\shai"
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Move-Item -Force "$TmpDir\shai.exe" "$InstallDir\shai.exe"

# Add to PATH if not already there
$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($UserPath -notlike "*$InstallDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$UserPath;$InstallDir", "User")
    Write-Host "Added $InstallDir to PATH (restart terminal to take effect)"
}

Remove-Item -Recurse -Force $TmpDir
Write-Host "shai installed to $InstallDir\shai.exe"
& "$InstallDir\shai.exe" --version
