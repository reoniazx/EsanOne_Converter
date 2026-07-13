$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
$resourceRoot = Join-Path $root 'src-tauri/resources/essentia'
$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) 'esanone-essentia-download'
$baseUrl = 'https://data.metabrainz.org/pub/musicbrainz/acousticbrainz/extractors'

$artifacts = @(
    @{ Platform = 'windows'; Archive = 'essentia-extractor-v2.1_beta2-1-ge3940c0-win-i686.zip'; Sha1 = '271230742cb92ee16347478c763ed2454a1d4e01'; Entry = 'streaming_extractor_music.exe'; Output = 'streaming_extractor_music.exe' },
    @{ Platform = 'linux'; Archive = 'essentia-extractor-v2.1_beta2-linux-x86_64.tar.gz'; Sha1 = '2413b637c49421fbb417db8987dc4badac443dab'; Entry = 'streaming_extractor_music'; Output = 'streaming_extractor_music' },
    @{ Platform = 'macos'; Archive = 'essentia-extractor-v2.1_beta2-2-gbb40004-osx.tar.gz'; Sha1 = '024d94e422a3303a93c31f82c23009f0dc9367d9'; Entry = 'streaming_extractor_music'; Output = 'streaming_extractor_music' }
)

New-Item -ItemType Directory -Force -Path $tempRoot | Out-Null

foreach ($artifact in $artifacts) {
    $archivePath = Join-Path $tempRoot $artifact.Archive
    $url = "$baseUrl/$($artifact.Archive)"
    Write-Host "Downloading $($artifact.Platform) Essentia extractor…"
    Invoke-WebRequest -Uri $url -OutFile $archivePath

    $hash = (Get-FileHash -Path $archivePath -Algorithm SHA1).Hash.ToLowerInvariant()
    if ($hash -ne $artifact.Sha1) {
        throw "SHA-1 mismatch for $($artifact.Archive): $hash"
    }

    $extractRoot = Join-Path $tempRoot $artifact.Platform
    if (Test-Path $extractRoot) {
        Remove-Item -Recurse -Force $extractRoot
    }
    New-Item -ItemType Directory -Force -Path $extractRoot | Out-Null
    if ($artifact.Archive.EndsWith('.zip')) {
        Expand-Archive -Path $archivePath -DestinationPath $extractRoot
    } else {
        tar -xzf $archivePath -C $extractRoot
        if ($LASTEXITCODE -ne 0) {
            throw "Could not extract $($artifact.Archive). Install tar first."
        }
    }

    $source = Join-Path $extractRoot $artifact.Entry
    if (-not (Test-Path $source)) {
        throw "The archive did not contain $($artifact.Entry)."
    }
    $destinationRoot = Join-Path $resourceRoot $artifact.Platform
    New-Item -ItemType Directory -Force -Path $destinationRoot | Out-Null
    Copy-Item -Force $source (Join-Path $destinationRoot $artifact.Output)
}

Write-Host 'Essentia extractors are ready under src-tauri/resources/essentia.'
