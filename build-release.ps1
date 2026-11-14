<#
.SYNOPSIS
  Build Unity L10n Tool (Rust + Tauri) va xuat ban chay duoc ra thu muc Release nam NGOAI du an.
  Goi uber-apk-signer.jar vao libra.pak (nen + ma hoa) thay vi de jar roi canh .exe.

.EXAMPLE
  .\build-release.ps1
  .\build-release.ps1 -OutDir 'D:\MyTools\UnityL10n' -Zip -WithCli
  .\build-release.ps1 -SkipBuild        # chi dong goi lai, khong build
#>
[CmdletBinding()]
param(
    [string]$OutDir = '',                 # mac dinh: <thu muc cha cua du an>\Release
    [string]$Signer = '',                 # uber-apk-signer-*.jar; de trong thi tu tim trong ..\tools
    [switch]$SkipBuild,
    [switch]$Zip,
    [switch]$WithCli                       # kem theo l10n-cli.exe
)

$ErrorActionPreference = 'Stop'
$proj = if ($PSScriptRoot) { $PSScriptRoot } else { Split-Path -Parent $MyInvocation.MyCommand.Path }
if (-not $OutDir) { $OutDir = Join-Path (Split-Path -Parent $proj) 'Release' }
$OutDir = [IO.Path]::GetFullPath($OutDir)

function Step($msg) { Write-Host "==> $msg" -ForegroundColor Cyan }

if ($OutDir.TrimEnd('\').StartsWith($proj.TrimEnd('\'), [StringComparison]::OrdinalIgnoreCase)) {
    throw "OutDir ($OutDir) nam ben trong du an. Hay chon thu muc ngoai $proj."
}

# 1. Tat chuong trinh neu dang chay (de ghi de duoc file .exe)
Step 'Tat chuong trinh neu dang chay'
$killed = $false
foreach ($n in 'UnityL10nTool', 'unity-l10n-gui') {
    Get-Process -Name $n -ErrorAction SilentlyContinue | ForEach-Object {
        $_ | Stop-Process -Force
        $killed = $true
    }
}
if ($killed) { Start-Sleep -Milliseconds 800; Write-Host '   da tat ban dang chay' }

# 2. Kiem tra cong cu
Step 'Kiem tra cong cu'
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw 'Khong thay cargo. Cai Rust tai https://rustup.rs (toolchain MSVC).'
}
Write-Host ('   ' + (& cargo -V))
if (-not (Get-Command java -ErrorAction SilentlyContinue)) {
    Write-Warning 'Khong thay java: ung dung van chay, nhung buoc ky APK se khong dung duoc.'
}

# 3. Build
if (-not $SkipBuild) {
    Step 'cargo build --release (lan dau mat vai phut)'
    Push-Location $proj
    try {
        & cargo build --release --workspace
        if ($LASTEXITCODE -ne 0) { throw "cargo build that bai (ma $LASTEXITCODE)" }
    } finally { Pop-Location }
}

$target = Join-Path $proj 'target\release'
$gui = Join-Path $target 'unity-l10n-gui.exe'
$cli = Join-Path $target 'l10n-cli.exe'
foreach ($f in $gui, $cli) {
    if (-not (Test-Path -LiteralPath $f)) { throw "Thieu file: $f (bo -SkipBuild de build lai)" }
}

# 4. Tim uber-apk-signer de goi vao libra.pak
if (-not $Signer) {
    $cand = Get-ChildItem -Path (Join-Path (Split-Path -Parent $proj) 'tools'), $proj, $OutDir -Filter 'uber-apk-signer*.jar' -ErrorAction SilentlyContinue |
        Sort-Object Name -Descending | Select-Object -First 1
    if ($cand) { $Signer = $cand.FullName }
}

# 5. Chep ra Release (chi thay file cua minh, khong xoa file khac)
Step "Chep ra $OutDir"
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

$copy = @(@{ Src = $gui; Dst = 'UnityL10nTool.exe' }, @{ Src = Join-Path $proj 'README.md'; Dst = 'README.md' })
if ($WithCli) { $copy += @{ Src = $cli; Dst = 'l10n-cli.exe' } }
foreach ($f in $copy) {
    if (-not (Test-Path -LiteralPath $f.Src)) { continue }
    $dst = Join-Path $OutDir $f.Dst
    try { Copy-Item -LiteralPath $f.Src -Destination $dst -Force }
    catch { throw "Khong ghi duoc $dst - co the ung dung dang mo, hay dong no roi chay lai." }
    Write-Host ('   {0,-22} {1,8:N0} KB' -f $f.Dst, ((Get-Item -LiteralPath $dst).Length / 1KB))
}

# 6. Tao libra.pak (nen + ma hoa jar) bang l10n-cli mkpak
if ($Signer -and (Test-Path -LiteralPath $Signer)) {
    Step 'Tao libra.pak (nen + ma hoa cong cu ky)'
    $pak = Join-Path $OutDir 'libra.pak'
    & $cli mkpak $pak $Signer
    if ($LASTEXITCODE -ne 0) { throw 'mkpak that bai' }
    Write-Host ('   {0,-22} {1,8:N0} KB' -f 'libra.pak', ((Get-Item -LiteralPath $pak).Length / 1KB))
    # da goi vao pak -> bo jar roi (neu Release cu con) de khong de lo dependency
    Get-ChildItem -LiteralPath $OutDir -Filter 'uber-apk-signer*.jar' -ErrorAction SilentlyContinue | ForEach-Object {
        Remove-Item $_.FullName -Force
        Write-Host "   da bo jar roi: $($_.Name)"
    }
} else {
    Write-Warning 'Khong tim thay uber-apk-signer-*.jar -> khong tao libra.pak. Buoc ky APK se khong dung duoc.'
}

# 7. Nen zip (tuy chon)
if ($Zip) {
    $ver = (Select-String -Path (Join-Path $proj 'src-tauri\Cargo.toml') -Pattern '^version\s*=\s*"(.+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
    $zipPath = Join-Path (Split-Path -Parent $OutDir) "UnityL10nTool-$ver-win-x64.zip"
    Step "Nen $zipPath"
    $items = Get-ChildItem -LiteralPath $OutDir -File | Where-Object { $_.Extension -in '.exe', '.pak', '.md' }
    Compress-Archive -LiteralPath $items.FullName -DestinationPath $zipPath -Force
}

Step 'Xong'
Write-Host "   Chay: $(Join-Path $OutDir 'UnityL10nTool.exe')" -ForegroundColor Green
