$ErrorActionPreference = 'Stop'
# CI-only scratch VHD. Resolve the disk solely from this image, never by scanning
# or selecting a host disk. Attach read-only; always detach in finally.
$scratch = Join-Path $env:RUNNER_TEMP ('memory-regainer-raw-' + [Guid]::NewGuid())
New-Item -ItemType Directory -Path $scratch | Out-Null
$image = Join-Path $scratch 'zero-test.vhd'
$create = Join-Path $scratch 'create.txt'
$detach = Join-Path $scratch 'detach.txt'
try {
    @("create vdisk file=`"$image`" maximum=32 type=fixed", "select vdisk file=`"$image`"", 'attach vdisk readonly') | Set-Content -Encoding ascii $create
    & diskpart.exe /s $create
    if ($LASTEXITCODE -ne 0) { throw 'Scratch VHD creation failed' }
    $diskImage = Get-DiskImage -ImagePath $image -ErrorAction Stop
    if (-not $diskImage.Attached) { throw 'Scratch VHD was not attached' }
    $disks = @($diskImage | Get-Disk -ErrorAction Stop)
    if ($disks.Count -ne 1 -or -not $disks[0].IsReadOnly) { throw 'Expected one read-only scratch disk' }
    $device = '\\.\PhysicalDrive' + $disks[0].Number
    & cargo run --locked -p recovery-core --example raw_device_probe -- $device 33554432
    if ($LASTEXITCODE -ne 0) { throw 'Raw-device regression probe failed' }
} finally {
    @("select vdisk file=`"$image`"", 'detach vdisk') | Set-Content -Encoding ascii $detach
    & diskpart.exe /s $detach
    $stillAttached = Test-Path $image
    if ($stillAttached) { $stillAttached = (Get-DiskImage -ImagePath $image -ErrorAction SilentlyContinue).Attached }
    if ($stillAttached) { throw "Scratch VHD still attached: $image" }
    Remove-Item -Recurse -Force $scratch
}
