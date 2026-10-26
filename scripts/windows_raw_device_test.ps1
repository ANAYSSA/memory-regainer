$ErrorActionPreference = 'Stop'
# CI-only scratch VHDs. All device identities come from each freshly-created
# image path. No host disks are enumerated or selected by number independently.
if (-not $env:RUNNER_TEMP) { throw 'RUNNER_TEMP is required for disposable CI fixtures' }
$scratch = Join-Path $env:RUNNER_TEMP ('memory-regainer-raw-' + [Guid]::NewGuid())
New-Item -ItemType Directory -Path $scratch | Out-Null
$zeroImage = Join-Path $scratch 'zero-test.vhd'
$volumeImage = Join-Path $scratch 'fat32-test.vhd'
$images = @($zeroImage, $volumeImage)
function Get-ScratchDisk([string]$Image, [UInt64]$ExpectedSize, [bool]$RequireReadOnly) {
    $actualImage = Get-DiskImage -ImagePath $Image -ErrorAction Stop
    if (-not $actualImage.Attached -or [IO.Path]::GetFullPath($actualImage.ImagePath) -ne [IO.Path]::GetFullPath($Image)) { throw 'Scratch image identity mismatch' }
    $resolved = @($actualImage | Get-Disk -ErrorAction Stop)
    if ($resolved.Count -ne 1) { throw 'Expected exactly one disk from the scratch image' }
    $disk = $resolved[0]
    # STORAGE_BUS_TYPE 15 is FileBackedVirtual. Physical USB/SATA/NVMe disks
    # categorically fail this guard before initialization or formatting.
    if ([int]$disk.BusType -ne 15 -or $disk.IsBoot -or $disk.IsSystem -or [UInt64]$disk.Size -ne $ExpectedSize) { throw 'Target is not the expected non-system scratch VHD' }
    if ($RequireReadOnly -and -not $disk.IsReadOnly) { throw 'Probe source must be attached read-only' }
    return $disk
}
function New-ScratchVhd([string]$Image, [int]$Megabytes, [bool]$ReadOnly) {
    if (Test-Path -LiteralPath $Image) { throw 'Refusing to replace an existing image' }
    if ($Image.Contains('"')) { throw 'Unexpected quote in scratch image path' }
    $script = Join-Path $scratch ('create-' + [Guid]::NewGuid() + '.txt')
    $attach = if ($ReadOnly) { 'attach vdisk readonly' } else { 'attach vdisk' }
    @("create vdisk file=`"$Image`" maximum=$Megabytes type=fixed", "select vdisk file=`"$Image`"", $attach) | Set-Content -Encoding ascii $script
    & diskpart.exe /s $script
    if ($LASTEXITCODE -ne 0) { throw 'Scratch VHD creation failed' }
}
try {
    New-ScratchVhd $zeroImage 32 $true
    $disk = Get-ScratchDisk $zeroImage 33554432 $true
    $device = '\\.\PhysicalDrive' + $disk.Number
    & cargo run --locked -p recovery-core --example raw_device_probe -- $device 33554432
    if ($LASTEXITCODE -ne 0) { throw 'Raw-device regression probe failed' }

    New-ScratchVhd $volumeImage 128 $false
    $disk = Get-ScratchDisk $volumeImage 134217728 $false
    if ($disk.PartitionStyle -ne 'RAW' -or $disk.IsReadOnly) { throw 'Only a new empty writable scratch disk may be initialized' }
    # Every mutation uses only the object obtained through the image-path guard.
    $disk | Initialize-Disk -PartitionStyle MBR -PassThru | Out-Null
    $disk = Get-ScratchDisk $volumeImage 134217728 $false
    $partition = New-Partition -InputObject $disk -UseMaximumSize -AssignDriveLetter
    $disk = Get-ScratchDisk $volumeImage 134217728 $false
    $parts = @($disk | Get-Partition -ErrorAction Stop)
    if ($parts.Count -ne 1 -or $parts[0].DiskNumber -ne $partition.DiskNumber -or $parts[0].PartitionNumber -ne $partition.PartitionNumber) { throw 'Scratch partition identity mismatch before format' }
    $partition = $parts[0]
    Format-Volume -Partition $partition -FileSystem FAT32 -AllocationUnitSize 512 -NewFileSystemLabel MRPROBE -Confirm:$false | Out-Null
    $volume = $partition | Get-Volume -ErrorAction Stop
    if ($volume.FileSystem -ne 'FAT32' -or $volume.FileSystemLabel -ne 'MRPROBE' -or [string]$volume.DriveLetter -notmatch '^[A-Za-z]$') { throw 'Scratch FAT32 volume verification failed' }
    $root = [string]$volume.DriveLetter + ':\'
    $repo = Split-Path -Parent $PSScriptRoot
    $video = Join-Path $repo 'crates\recovery-core\tests\fixtures\video\original-tail.mp4'
    $photo = Join-Path $repo 'crates\recovery-core\tests\fixtures\original.jpg'
    New-Item -ItemType Directory -Path (Join-Path $root 'LIVE\A'), (Join-Path $root 'LIVE\B'), (Join-Path $root 'PHOTOS') | Out-Null
    $expected = @()
    for ($i = 1; $i -le 42; $i++) {
        $folder = if ($i -le 21) { 'A' } else { 'B' }
        $relative = 'LIVE/' + $folder + ('/CLIP{0:D4}.MP4' -f $i)
        Copy-Item -LiteralPath $video -Destination (Join-Path $root ($relative.Replace('/', '\')))
        $expected += [pscustomobject]@{ relative_path = $relative; source_path = $video }
    }
    foreach ($relative in @('PHOTOS/PHOTO001.JPG', 'PHOTOS/PHOTO002.JPG')) {
        Copy-Item -LiteralPath $photo -Destination (Join-Path $root ($relative.Replace('/', '\')))
        $expected += [pscustomobject]@{ relative_path = $relative; source_path = $photo }
    }
    $marker = Join-Path $scratch 'marker.bin'
    [IO.File]::WriteAllBytes($marker, [Text.Encoding]::ASCII.GetBytes('Memory Regainer disposable VHD ' + [Guid]::NewGuid()))
    Copy-Item -LiteralPath $marker -Destination (Join-Path $root 'MRPROBE.ID')
    $expected += [pscustomobject]@{ relative_path = 'MRPROBE.ID'; source_path = $marker }
    $manifest = Join-Path $scratch 'expected.json'
    [IO.File]::WriteAllText($manifest, (ConvertTo-Json -InputObject $expected -Depth 4), [Text.UTF8Encoding]::new($false))

    # Flush/detach the created filesystem, then mount it read-only for every
    # scanner and recovery read. No format command is issued after this point.
    Dismount-DiskImage -ImagePath $volumeImage -ErrorAction Stop
    Mount-DiskImage -ImagePath $volumeImage -Access ReadOnly -ErrorAction Stop | Out-Null
    $disk = Get-ScratchDisk $volumeImage 134217728 $true
    $parts = @($disk | Get-Partition -ErrorAction Stop | Where-Object { [string]$_.DriveLetter -match '^[A-Za-z]$' })
    if ($parts.Count -ne 1) { throw 'Expected one mounted scratch FAT32 partition' }
    $partition = $parts[0]
    $volume = $partition | Get-Volume -ErrorAction Stop
    if ($volume.FileSystemLabel -ne 'MRPROBE' -or $volume.FileSystem -ne 'FAT32') { throw 'Read-only scratch volume label/filesystem mismatch' }
    $rawVolume = '\\.\' + [string]$partition.DriveLetter + ':'
    & cargo run --locked -p recovery-core --example mounted_volume_probe -- $rawVolume ([string]$partition.Size) $manifest
    if ($LASTEXITCODE -ne 0) { throw 'Mounted FAT32 raw-volume regression probe failed' }
} finally {
    $attached = @()
    foreach ($image in $images) {
        if (-not (Test-Path -LiteralPath $image)) { continue }
        $info = Get-DiskImage -ImagePath $image -ErrorAction SilentlyContinue
        if ($info.Attached) { Dismount-DiskImage -ImagePath $image -ErrorAction SilentlyContinue }
        if ((Get-DiskImage -ImagePath $image -ErrorAction SilentlyContinue).Attached) { $attached += $image }
    }
    if ($attached.Count -gt 0) { throw ('Scratch VHD still attached: ' + ($attached -join ', ')) }
    Remove-Item -LiteralPath $scratch -Recurse -Force
}
