param([Parameter(Mandatory=$true)][string]$Executable)
$ErrorActionPreference = 'Stop'
$mt = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\mt.exe" | Sort-Object FullName | Select-Object -Last 1
if (-not $mt) { throw 'Windows SDK manifest tool missing' }
$manifest = Join-Path $env:RUNNER_TEMP ('memory-regainer-manifest-' + [Guid]::NewGuid() + '.xml')
try {
    & $mt.FullName "-inputresource:$Executable;#1" "-out:$manifest"
    if ($LASTEXITCODE -ne 0) { throw 'Cannot extract embedded executable manifest' }
    [xml]$xml = Get-Content $manifest
    $level = $xml.SelectSingleNode("//*[local-name()='requestedExecutionLevel']")
    if ($level.level -ne 'requireAdministrator' -or $level.uiAccess -ne 'false') {
        throw 'Application must request standard Windows administrator consent'
    }
    Write-Output 'Verified embedded requireAdministrator manifest with uiAccess=false'
} finally {
    Remove-Item $manifest -ErrorAction SilentlyContinue
}
