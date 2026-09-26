param(
    [string]$ProfilePath = 'crates/platscope-readonly-scan/src/spatial/profiles/current-v2.json'
)

$ErrorActionPreference = 'Stop'
$profile = [IO.Path]::GetFullPath($ProfilePath)
$signature = "$profile.sig"
if (-not [IO.File]::Exists($profile) -or -not [IO.File]::Exists($signature)) {
    throw 'Нужны JSON профиля и его подпись .sig.'
}

$payload = [Convert]::ToBase64String([IO.File]::ReadAllBytes($profile))
$signed = [IO.File]::ReadAllText($signature).Trim()
$envelope = @{ payload = $payload; signature = $signed } | ConvertTo-Json -Compress
$name = [IO.Path]::GetFileNameWithoutExtension($profile) + '.signed.json'
$destination = [IO.Path]::Combine([IO.Path]::GetDirectoryName($profile), $name)
[IO.File]::WriteAllText($destination, $envelope + "`n", [Text.UTF8Encoding]::new($false))
Write-Output $destination
