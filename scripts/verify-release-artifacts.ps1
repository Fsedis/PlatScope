[CmdletBinding()]
param(
    [ValidateSet("Qa", "Trusted")]
    [string]$Mode = "Qa",

    [string]$WorkspaceRoot = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($WorkspaceRoot)) {
    $WorkspaceRoot = Split-Path -Parent $PSScriptRoot
}
$resolvedWorkspace = (Resolve-Path -LiteralPath $WorkspaceRoot).Path
$script:PreflightResults = [System.Collections.Generic.List[object]]::new()

function Add-PreflightResult {
    param(
        [Parameter(Mandatory)]
        [string]$Check,

        [Parameter(Mandatory)]
        [ValidateSet("PASS", "WARN", "FAIL")]
        [string]$Status,

        [Parameter(Mandatory)]
        [string]$Evidence
    )

    $script:PreflightResults.Add([pscustomobject]@{
        Check = $Check
        Status = $Status
        Evidence = $Evidence
    })
}

function Test-RequiredFile {
    param(
        [Parameter(Mandatory)]
        [string]$Check,

        [Parameter(Mandatory)]
        [string]$Path
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        Add-PreflightResult -Check $Check -Status "FAIL" -Evidence "Файл отсутствует: $Path"
        return $false
    }
    $item = Get-Item -LiteralPath $Path
    if ($item.Length -le 0) {
        Add-PreflightResult -Check $Check -Status "FAIL" -Evidence "Файл пуст: $Path"
        return $false
    }
    Add-PreflightResult -Check $Check -Status "PASS" -Evidence "$($item.Name), $($item.Length) bytes"
    return $true
}

function Test-ChecksumManifest {
    param(
        [Parameter(Mandatory)]
        [string]$Check,

        [Parameter(Mandatory)]
        [string]$Directory,

        [Parameter(Mandatory)]
        [string]$ManifestPath
    )

    if (-not (Test-Path -LiteralPath $ManifestPath -PathType Leaf)) {
        Add-PreflightResult -Check $Check -Status "FAIL" -Evidence "Checksum manifest отсутствует: $ManifestPath"
        return $false
    }

    $resolvedDirectory = (Resolve-Path -LiteralPath $Directory).Path
    $directoryPrefix = $resolvedDirectory.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
    $lines = @(Get-Content -LiteralPath $ManifestPath -Encoding ASCII | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    if ($lines.Count -eq 0) {
        Add-PreflightResult -Check $Check -Status "FAIL" -Evidence "Checksum manifest пуст"
        return $false
    }

    $seen = [System.Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach ($line in $lines) {
        if ($line -notmatch '^(?<hash>[0-9a-fA-F]{64}) [ *](?<file>.+)$') {
            Add-PreflightResult -Check $Check -Status "FAIL" -Evidence "Некорректная строка manifest: $line"
            return $false
        }
        $relativePath = $Matches.file.Replace('/', [IO.Path]::DirectorySeparatorChar)
        if ([IO.Path]::IsPathRooted($relativePath)) {
            Add-PreflightResult -Check $Check -Status "FAIL" -Evidence "Абсолютный путь запрещён в manifest: $relativePath"
            return $false
        }
        $artifactPath = [IO.Path]::GetFullPath((Join-Path $resolvedDirectory $relativePath))
        if (-not $artifactPath.StartsWith($directoryPrefix, [StringComparison]::OrdinalIgnoreCase)) {
            Add-PreflightResult -Check $Check -Status "FAIL" -Evidence "Путь выходит за каталог artifact: $relativePath"
            return $false
        }
        if (-not $seen.Add($artifactPath)) {
            Add-PreflightResult -Check $Check -Status "FAIL" -Evidence "Дубликат в checksum manifest: $relativePath"
            return $false
        }
        if (-not (Test-Path -LiteralPath $artifactPath -PathType Leaf)) {
            Add-PreflightResult -Check $Check -Status "FAIL" -Evidence "Artifact из manifest отсутствует: $relativePath"
            return $false
        }
        $actual = (Get-FileHash -LiteralPath $artifactPath -Algorithm SHA256).Hash
        if (-not $actual.Equals($Matches.hash, [StringComparison]::OrdinalIgnoreCase)) {
            Add-PreflightResult -Check $Check -Status "FAIL" -Evidence "SHA-256 не совпадает: $relativePath"
            return $false
        }
    }

    Add-PreflightResult -Check $Check -Status "PASS" -Evidence "$($seen.Count) checksum entries подтверждены"
    return $true
}

function Test-CompanionManifest {
    param(
        [Parameter(Mandatory)]
        [string]$ManifestPath,

        [Parameter(Mandatory)]
        [string]$IndexPath
    )

    try {
        $manifest = Get-Content -LiteralPath $ManifestPath -Raw -Encoding UTF8 | ConvertFrom-Json
        $permissions = @($manifest.permissions | Sort-Object)
        $gameIds = @($manifest.data.game_targeting.game_ids)
        $valid =
            $manifest.manifest_version -eq 1 -and
            $manifest.type -eq "WebApp" -and
            ($permissions -join ',') -eq "FileSystem,GameInfo" -and
            $manifest.data.game_targeting.type -eq "dedicated" -and
            $gameIds.Count -eq 1 -and
            $gameIds[0] -eq 8954 -and
            $manifest.data.windows.desktop.desktop_only -eq $true -and
            $manifest.data.windows.desktop.show_in_taskbar -eq $true
        if (-not $valid) {
            Add-PreflightResult -Check "companion_manifest" -Status "FAIL" -Evidence "Manifest нарушает минимальную permission/window boundary"
            return $false
        }
        $index = Get-Content -LiteralPath $IndexPath -Raw -Encoding UTF8
        if ($index -notmatch "connect-src 'none'") {
            Add-PreflightResult -Check "companion_manifest" -Status "FAIL" -Evidence "CSP не блокирует outbound connections"
            return $false
        }
        Add-PreflightResult -Check "companion_manifest" -Status "PASS" -Evidence "WebApp, Warframe 8954, GameInfo/FileSystem, visible desktop window, connect-src none"
        return $true
    }
    catch {
        Add-PreflightResult -Check "companion_manifest" -Status "FAIL" -Evidence "Manifest не разобран: $($_.Exception.Message)"
        return $false
    }
}

$desktopExe = Join-Path $resolvedWorkspace "target/release/platscope.exe"
$nsisDirectory = Join-Path $resolvedWorkspace "target/release/bundle/nsis"
$nsisChecksums = Join-Path $nsisDirectory "SHA256SUMS.txt"
$linuxDirectory = Join-Path $resolvedWorkspace "target/release/bundle/appimage"

$null = Test-RequiredFile -Check "desktop_executable" -Path $desktopExe
$installerCandidates = @(if (Test-Path -LiteralPath $nsisDirectory -PathType Container) {
    Get-ChildItem -LiteralPath $nsisDirectory -Filter "PlatScope_*_x64-setup.exe" -File
})
$installerExists = $false
if ($installerCandidates.Count -eq 1) {
    $installer = $installerCandidates[0].FullName
    $installerExists = Test-RequiredFile -Check "windows_installer" -Path $installer
}
elseif ($installerCandidates.Count -eq 0) {
    Add-PreflightResult -Check "windows_installer" -Status "FAIL" -Evidence "NSIS installer отсутствует"
}
else {
    Add-PreflightResult -Check "windows_installer" -Status "FAIL" -Evidence "Ожидался один NSIS installer, найдено $($installerCandidates.Count)"
}
if ($installerExists) {
    $null = Test-ChecksumManifest -Check "windows_checksums" -Directory $nsisDirectory -ManifestPath $nsisChecksums
    $signature = Get-AuthenticodeSignature -LiteralPath $installer
    if ($Mode -eq "Qa") {
        if ($signature.Status -eq "NotSigned") {
            Add-PreflightResult -Check "windows_authenticode" -Status "WARN" -Evidence "NotSigned: допустимо только для внутреннего QA"
        }
        elseif ($signature.Status -eq "Valid") {
            Add-PreflightResult -Check "windows_authenticode" -Status "PASS" -Evidence "Valid Authenticode"
        }
        else {
            Add-PreflightResult -Check "windows_authenticode" -Status "FAIL" -Evidence "Недопустимый статус: $($signature.Status)"
        }
    }
    else {
        if ($signature.Status -ne "Valid") {
            Add-PreflightResult -Check "windows_authenticode" -Status "FAIL" -Evidence "Trusted mode требует Valid, получено $($signature.Status)"
        }
        elseif ($null -eq $signature.SignerCertificate) {
            Add-PreflightResult -Check "windows_authenticode" -Status "FAIL" -Evidence "Отсутствует signer certificate"
        }
        elseif ($null -eq $signature.TimeStamperCertificate) {
            Add-PreflightResult -Check "windows_authenticode" -Status "FAIL" -Evidence "Отсутствует trusted timestamp certificate"
        }
        else {
            Add-PreflightResult -Check "windows_authenticode" -Status "PASS" -Evidence "Valid signer и timestamp certificates"
        }
    }
}

$appImages = @(if (Test-Path -LiteralPath $linuxDirectory -PathType Container) {
    Get-ChildItem -LiteralPath $linuxDirectory -Filter "*.AppImage" -File
})
if ($appImages.Count -eq 0) {
    $linuxStatus = if ($Mode -eq "Trusted") { "FAIL" } else { "WARN" }
    Add-PreflightResult -Check "linux_appimage" -Status $linuxStatus -Evidence "AppImage отсутствует; Linux build не доказан"
}
elseif ($appImages.Count -ne 1) {
    Add-PreflightResult -Check "linux_appimage" -Status "FAIL" -Evidence "Ожидался один AppImage, найдено $($appImages.Count)"
}
else {
    Add-PreflightResult -Check "linux_appimage" -Status "PASS" -Evidence "$($appImages[0].Name), $($appImages[0].Length) bytes"
    $linuxChecksums = Join-Path $linuxDirectory "SHA256SUMS.txt"
    $null = Test-ChecksumManifest -Check "linux_checksums" -Directory $linuxDirectory -ManifestPath $linuxChecksums

    if ($Mode -eq "Trusted") {
        $signaturePath = "$($appImages[0].FullName).sig"
        $keyringPath = Join-Path $linuxDirectory "release-signing-keyring.gpg"
        $gpgv = Get-Command "gpgv" -ErrorAction SilentlyContinue
        if (-not (Test-Path -LiteralPath $signaturePath -PathType Leaf)) {
            Add-PreflightResult -Check "linux_detached_signature" -Status "FAIL" -Evidence "Detached signature отсутствует: $signaturePath"
        }
        elseif (-not (Test-Path -LiteralPath $keyringPath -PathType Leaf)) {
            Add-PreflightResult -Check "linux_detached_signature" -Status "FAIL" -Evidence "Approved public keyring отсутствует: $keyringPath"
        }
        elseif ($null -eq $gpgv) {
            Add-PreflightResult -Check "linux_detached_signature" -Status "FAIL" -Evidence "gpgv недоступен"
        }
        else {
            & $gpgv.Source --keyring $keyringPath $signaturePath $appImages[0].FullName 2>&1 | Out-Null
            if ($LASTEXITCODE -eq 0) {
                Add-PreflightResult -Check "linux_detached_signature" -Status "PASS" -Evidence "gpgv подтвердил detached signature"
            }
            else {
                Add-PreflightResult -Check "linux_detached_signature" -Status "FAIL" -Evidence "gpgv отклонил detached signature"
            }
        }
    }
}

Write-Host "PlatScope artifact preflight · mode=$Mode"
$script:PreflightResults | Format-Table -AutoSize -Wrap | Out-String | Write-Host

$failures = @($script:PreflightResults | Where-Object Status -eq "FAIL")
$warnings = @($script:PreflightResults | Where-Object Status -eq "WARN")
$passed = @($script:PreflightResults | Where-Object Status -eq "PASS")
Write-Host "Итог: PASS=$($passed.Count) WARN=$($warnings.Count) FAIL=$($failures.Count)"
if ($failures.Count -gt 0) {
    $failedChecks = $failures.Check -join ", "
    throw "Artifact preflight failed: $failedChecks"
}
