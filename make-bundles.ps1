# make-bundles.ps1
# Builds TEXT bundles for the whole yandex_to_lastfm project.
# - A0: root meta + filtered project tree
# - B0: all text files from ym_bridge_ext
# - Rxx: one Rust source file -> one bundle from yandex_to_lastfm_rs/src
# Bundles in _packs are updated only if content really changed.

[CmdletBinding()]
param(
    [string]$ProjectRoot,
    [string]$OutDir
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# -------------------- PATHS --------------------

function Normalize-Path([string]$p) {
    return [System.IO.Path]::GetFullPath($p)
}

if (-not $ProjectRoot) {
    if ($PSScriptRoot) {
        $ProjectRoot = $PSScriptRoot
    }
    else {
        $ProjectRoot = (Get-Location).Path
    }
}

$ProjectRoot = Normalize-Path $ProjectRoot

if (-not (Test-Path -LiteralPath $ProjectRoot -PathType Container)) {
    throw "ProjectRoot not found: $ProjectRoot"
}

if (-not $OutDir) {
    $OutDir = Join-Path $ProjectRoot "_packs"
}

$OutDir = Normalize-Path $OutDir
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

$RustRoot = Join-Path $ProjectRoot "yandex_to_lastfm_rs"
$ExtRoot = Join-Path $ProjectRoot "ym_bridge_ext"
$LegacyRoot = Join-Path $ProjectRoot "legacy"

Write-Verbose "ProjectRoot: $ProjectRoot"
Write-Verbose "OutDir     : $OutDir"
Write-Verbose "RustRoot   : $RustRoot"
Write-Verbose "ExtRoot    : $ExtRoot"

$script:ChangedBundles = @()

# -------------------- FILTERS --------------------

$script:TreeSkipDirs = @(
    ".git",
    ".idea",
    ".vscode",
    ".vs",
    "node_modules",
    "target",
    "_packs",
    "__pycache__",
    "dist",
    "build",
    "out",
    "bin",
    "obj",
    "coverage"
)

$script:TextFileExtensions = @(
    ".js",
    ".mjs",
    ".cjs",
    ".ts",
    ".tsx",
    ".jsx",
    ".json",
    ".html",
    ".css",
    ".md",
    ".txt",
    ".ps1",
    ".toml",
    ".yml",
    ".yaml",
    ".xml",
    ".svg",
    ".rs",
    ".py"
)

$script:TextFileNames = @(
    ".gitignore",
    ".env"
)

# -------------------- HELPERS --------------------

function Bundle-Name {
    param(
        [Parameter(Mandatory)][string]$Prefix,
        [Parameter(Mandatory)][string]$Location,
        [Parameter(Mandatory)][string]$What
    )

    $loc = ($Location -replace '[\\/:\s]+', '__').Trim('_')
    $what = ($What -replace '[\\/:\s]+', '__').Trim('_')
    return ("{0}__{1}__{2}.txt" -f $Prefix, $loc, $what)
}

function Get-IfExists([string]$p) {
    if (Test-Path -LiteralPath $p -PathType Leaf) { Get-Item -LiteralPath $p } else { $null }
}

function Get-ExistingFiles {
    param(
        [Parameter(Mandatory)][string]$BaseDir,
        [Parameter(Mandatory)][string[]]$RelativePaths
    )

    $files = New-Object System.Collections.Generic.List[System.IO.FileInfo]

    foreach ($rel in $RelativePaths) {
        $fi = Get-IfExists (Join-Path $BaseDir $rel)
        if ($fi) {
            [void]$files.Add($fi)
        }
    }

    return , ([System.IO.FileInfo[]]$files.ToArray())
}

function Write-NamedBundle {
    param(
        [Parameter(Mandatory)][string]$Prefix,
        [Parameter(Mandatory)][string]$Location,
        [Parameter(Mandatory)][string]$What,
        [Parameter(Mandatory)][string]$RootPath,
        [Parameter(Mandatory)][System.IO.FileInfo[]]$Files,
        [string]$Title = $null
    )

    Write-Bundle `
        -RootPath $RootPath `
        -OutName (Bundle-Name -Prefix $Prefix -Location $Location -What $What) `
        -Files $Files `
        -Title $Title
}

function RelPath {
    param(
        [Parameter(Mandatory)][string]$FullPath,
        [Parameter(Mandatory)][string]$RootPath
    )

    $full = Normalize-Path $FullPath
    $root = Normalize-Path $RootPath

    if ($full.StartsWith($root, [System.StringComparison]::OrdinalIgnoreCase)) {
        $n = $root.Length
        if ($full.Length -gt $n -and $full[$n] -in '\', '/') {
            return $full.Substring($n + 1)
        }
        return $full.Substring($n)
    }

    return $full
}

function Should-SkipDir {
    param(
        [Parameter(Mandatory)][System.IO.DirectoryInfo]$Dir
    )

    if ($script:TreeSkipDirs -contains $Dir.Name) {
        return $true
    }

    $attrs = $Dir.Attributes
    if (($attrs -band [IO.FileAttributes]::System) -or ($attrs -band [IO.FileAttributes]::Hidden)) {
        return $true
    }

    return $false
}

function Is-TextFile {
    param(
        [Parameter(Mandatory)][System.IO.FileInfo]$File
    )

    if ($script:TextFileNames -contains $File.Name) {
        return $true
    }

    return ($script:TextFileExtensions -contains $File.Extension.ToLowerInvariant())
}

function Get-TreeLines {
    param(
        [Parameter(Mandatory)][string]$RootPath
    )

    $rootItem = Get-Item -LiteralPath $RootPath
    $lines = New-Object System.Collections.Generic.List[string]
    $lines.Add($rootItem.Name)

    function Add-TreeNode {
        param(
            [Parameter(Mandatory)][string]$DirPath,
            [string]$Prefix = ""
        )

        $items = Get-ChildItem -LiteralPath $DirPath -Force |
        Where-Object {
            if ($_.PSIsContainer) {
                return -not (Should-SkipDir -Dir $_)
            }
            return $true
        } |
        Sort-Object @{ Expression = { if ($_.PSIsContainer) { 0 } else { 1 } } }, Name

        for ($i = 0; $i -lt $items.Count; $i++) {
            $item = $items[$i]
            $isLast = ($i -eq ($items.Count - 1))

            $branch = if ($isLast) { "└─ " } else { "├─ " }
            $lines.Add($Prefix + $branch + $item.Name)

            if ($item.PSIsContainer) {
                $nextPrefix = if ($isLast) { $Prefix + "   " } else { $Prefix + "│  " }
                Add-TreeNode -DirPath $item.FullName -Prefix $nextPrefix
            }
        }
    }

    Add-TreeNode -DirPath $rootItem.FullName -Prefix ""
    return , $lines.ToArray()
}

function Emit-ProjectTree {
    param(
        [Parameter(Mandatory)][string]$OutPath,
        [Parameter(Mandatory)][string]$TreeRoot
    )

    Add-Content -LiteralPath $OutPath -Encoding UTF8 -Value "`r`n# Project tree (filtered):"

    $treeLines = Get-TreeLines -RootPath $TreeRoot
    foreach ($line in $treeLines) {
        Add-Content -LiteralPath $OutPath -Encoding UTF8 -Value ("# {0}" -f $line)
    }

    Add-Content -LiteralPath $OutPath -Encoding UTF8 -Value ""
}

function Emit-Manifest {
    param(
        [Parameter(Mandatory)][string]$OutPath,
        [Parameter(Mandatory)][System.IO.FileInfo[]]$Files,
        [Parameter(Mandatory)][string]$RootPath
    )

    Add-Content -LiteralPath $OutPath -Encoding UTF8 -Value "`r`n# Contains files:"
    foreach ($f in @($Files)) {
        $rel = RelPath -FullPath $f.FullName -RootPath $RootPath
        Add-Content -LiteralPath $OutPath -Encoding UTF8 -Value ("# - {0}" -f $rel)
    }
    Add-Content -LiteralPath $OutPath -Encoding UTF8 -Value ""
}

function Append-File {
    param(
        [Parameter(Mandatory)][string]$OutPath,
        [Parameter(Mandatory)][System.IO.FileInfo]$File,
        [Parameter(Mandatory)][string]$RootPath
    )

    $rel = RelPath -FullPath $File.FullName -RootPath $RootPath

    Add-Content -LiteralPath $OutPath -Encoding UTF8 -Value ("`r`n===== BEGIN FILE: {0} =====" -f $rel)

    try {
        $content = Get-Content -LiteralPath $File.FullName -Raw -Encoding UTF8
    }
    catch {
        $bytes = [System.IO.File]::ReadAllBytes($File.FullName)
        $content = [System.Text.Encoding]::UTF8.GetString($bytes)
    }

    Add-Content -LiteralPath $OutPath -Encoding UTF8 -Value $content
    Add-Content -LiteralPath $OutPath -Encoding UTF8 -Value ("`r`n===== END FILE: {0} =====`r`n" -f $rel)
}

function Write-Bundle {
    param(
        [Parameter(Mandatory)][string]$RootPath,
        [Parameter(Mandatory)][string]$OutName,
        [Parameter(Mandatory)][System.IO.FileInfo[]]$Files,
        [string]$Title = $null,
        [switch]$IncludeProjectTree,
        [string]$ProjectTreeRoot = $null
    )

    $Files = @($Files)

    if ($Files.Count -eq 0 -and -not $IncludeProjectTree) {
        Write-Verbose "Bundle '$OutName': empty file list, skip."
        return
    }

    $OutPath = Join-Path $OutDir $OutName
    $existed = Test-Path -LiteralPath $OutPath -PathType Leaf
    $tempPath = [System.IO.Path]::GetTempFileName()

    try {
        $titleText = ("# Bundle: {0}" -f $OutName)
        if ($Title) { $titleText += "`r`n# " + $Title }

        Set-Content -LiteralPath $tempPath -Encoding UTF8 -Value $titleText

        if ($IncludeProjectTree) {
            if (-not $ProjectTreeRoot) {
                throw "ProjectTreeRoot is required when IncludeProjectTree is used."
            }
            Emit-ProjectTree -OutPath $tempPath -TreeRoot $ProjectTreeRoot
        }

        if ($Files.Count -gt 0) {
            Emit-Manifest -OutPath $tempPath -Files $Files -RootPath $RootPath

            foreach ($f in $Files) {
                Append-File -OutPath $tempPath -File $f -RootPath $RootPath
            }
        }

        $shouldWrite = $true
        if ($existed) {
            $oldContent = Get-Content -LiteralPath $OutPath -Raw -Encoding UTF8
            $newContent = Get-Content -LiteralPath $tempPath -Raw -Encoding UTF8
            if ($oldContent -eq $newContent) {
                $shouldWrite = $false
            }
        }

        if ($shouldWrite) {
            Move-Item -LiteralPath $tempPath -Destination $OutPath -Force
            $verb = if ($existed) { "Updated" } else { "Created" }

            Write-Host ("{0}: {1}  (files: {2})" -f $verb, $OutPath, $Files.Count)

            $script:ChangedBundles += [pscustomobject]@{
                Action = $verb
                Path   = $OutPath
                Files  = $Files.Count
            }
        }
        else {
            Write-Verbose "Bundle '$OutName': unchanged."
        }
    }
    finally {
        if (Test-Path -LiteralPath $tempPath -PathType Leaf) {
            Remove-Item -LiteralPath $tempPath -ErrorAction SilentlyContinue
        }
    }
}

# -------------------- FILE COLLECTION --------------------

# A0: root project meta + filtered project tree
$rootFiles = @()
foreach ($name in @("README.md", "make-bundles.ps1")) {
    $fi = Get-IfExists (Join-Path $ProjectRoot $name)
    if ($fi) { $rootFiles += $fi }
}

if (Test-Path -LiteralPath $RustRoot -PathType Container) {
    foreach ($name in @("Cargo.toml", "build.rs", ".gitignore", ".env")) {
        $fi = Get-IfExists (Join-Path $RustRoot $name)
        if ($fi) { $rootFiles += $fi }
    }
}

if ($rootFiles.Count -gt 0) {
    Write-Bundle `
        -RootPath $ProjectRoot `
        -OutName (Bundle-Name -Prefix "A0" -Location "root" -What "project_meta") `
        -Files $rootFiles `
        -Title "root project meta files + filtered project tree" `
        -IncludeProjectTree `
        -ProjectTreeRoot $ProjectRoot
}

# Bxx: browser extension bundles
if (Test-Path -LiteralPath $ExtRoot -PathType Container) {
    $b01 = Get-ExistingFiles $ExtRoot @("background.js")
    if ($b01.Count -gt 0) {
        Write-NamedBundle `
            -Prefix "B01" `
            -Location "ym_bridge_ext" `
            -What "background" `
            -RootPath $ProjectRoot `
            -Files $b01 `
            -Title "browser extension: background.js"
    }

    $b02 = Get-ExistingFiles $ExtRoot @("delivery.js")
    if ($b02.Count -gt 0) {
        Write-NamedBundle `
            -Prefix "B02" `
            -Location "ym_bridge_ext" `
            -What "delivery" `
            -RootPath $ProjectRoot `
            -Files $b02 `
            -Title "browser extension: delivery.js"
    }

    $b03 = Get-ExistingFiles $ExtRoot @("lastfm_api.js")
    if ($b03.Count -gt 0) {
        Write-NamedBundle `
            -Prefix "B03" `
            -Location "ym_bridge_ext" `
            -What "lastfm_api" `
            -RootPath $ProjectRoot `
            -Files $b03 `
            -Title "browser extension: lastfm_api.js"
    }

    $b04 = Get-ExistingFiles $ExtRoot @("page.js")
    if ($b04.Count -gt 0) {
        Write-NamedBundle `
            -Prefix "B04" `
            -Location "ym_bridge_ext" `
            -What "page" `
            -RootPath $ProjectRoot `
            -Files $b04 `
            -Title "browser extension: page.js"
    }

    $b05 = Get-ExistingFiles $ExtRoot @("panel.js")
    if ($b05.Count -gt 0) {
        Write-NamedBundle `
            -Prefix "B05" `
            -Location "ym_bridge_ext" `
            -What "panel" `
            -RootPath $ProjectRoot `
            -Files $b05 `
            -Title "browser extension: panel.js"
    }

    $b06 = Get-ExistingFiles $ExtRoot @("sidepanel.html")
    if ($b06.Count -gt 0) {
        Write-NamedBundle `
            -Prefix "B06" `
            -Location "ym_bridge_ext" `
            -What "sidepanel_html" `
            -RootPath $ProjectRoot `
            -Files $b06 `
            -Title "browser extension: sidepanel.html"
    }

    $b07 = Get-ExistingFiles $ExtRoot @("options.html", "options.js")
    if ($b07.Count -gt 0) {
        Write-NamedBundle `
            -Prefix "B07" `
            -Location "ym_bridge_ext" `
            -What "options" `
            -RootPath $ProjectRoot `
            -Files $b07 `
            -Title "browser extension: options.html + options.js"
    }

    # Остаточный bundle для файлов, которые ты не просил дробить отдельно
    $b08 = Get-ExistingFiles $ExtRoot @("content.js", "manifest.json", "mode.js", "sidepanel.css")
    if ($b08.Count -gt 0) {
        Write-NamedBundle `
            -Prefix "B08" `
            -Location "ym_bridge_ext" `
            -What "core" `
            -RootPath $ProjectRoot `
            -Files $b08 `
            -Title "browser extension: content.js + manifest.json + mode.js + sidepanel.css"
    }
}

# Optional: legacy bundle
if (Test-Path -LiteralPath $LegacyRoot -PathType Container) {
    $legacyFiles = Get-ChildItem -LiteralPath $LegacyRoot -Recurse -File -Force |
    Where-Object { Is-TextFile -File $_ } |
    Sort-Object FullName

    if ($legacyFiles.Count -gt 0) {
        Write-Bundle `
            -RootPath $ProjectRoot `
            -OutName (Bundle-Name -Prefix "L0" -Location "root" -What "legacy") `
            -Files $legacyFiles `
            -Title "legacy source bundle"
    }
}

# Rxx: rust bundles with explicit grouping
$srcRoot = Join-Path $RustRoot "src"
$uiRoot = Join-Path $srcRoot "ui"

if (Test-Path -LiteralPath $srcRoot -PathType Container) {
    $r01 = Get-ExistingFiles $srcRoot @(
        "app_config.rs",
        "autostart.rs",
        "config.rs",
        "lastfm.rs",
        "main.rs",
        "models.rs"
    )
    if ($r01.Count -gt 0) {
        Write-NamedBundle `
            -Prefix "R01" `
            -Location "yandex_to_lastfm_rs\src" `
            -What "core" `
            -RootPath $ProjectRoot `
            -Files $r01 `
            -Title "rust source: app_config + autostart + config + lastfm + main + models"
    }

    $r07 = Get-ExistingFiles $srcRoot @("server.rs")
    if ($r07.Count -gt 0) {
        Write-NamedBundle `
            -Prefix "R07" `
            -Location "yandex_to_lastfm_rs\src" `
            -What "server" `
            -RootPath $ProjectRoot `
            -Files $r07 `
            -Title "rust source: yandex_to_lastfm_rs\src\server.rs"
    }
}

if (Test-Path -LiteralPath $uiRoot -PathType Container) {
    $r08 = Get-ExistingFiles $uiRoot @(
        "anim.rs",
        "mod.rs",
        "state.rs",
        "text.rs",
        "tray.rs"
    )
    if ($r08.Count -gt 0) {
        Write-NamedBundle `
            -Prefix "R08" `
            -Location "yandex_to_lastfm_rs\src\ui" `
            -What "core" `
            -RootPath $ProjectRoot `
            -Files $r08 `
            -Title "rust ui source: anim + mod + state + text + tray"
    }

    $r10 = Get-ExistingFiles $uiRoot @("raster.rs")
    if ($r10.Count -gt 0) {
        Write-NamedBundle `
            -Prefix "R10" `
            -Location "yandex_to_lastfm_rs\src\ui" `
            -What "raster" `
            -RootPath $ProjectRoot `
            -Files $r10 `
            -Title "rust source: yandex_to_lastfm_rs\src\ui\raster.rs"
    }

    $r11 = Get-ExistingFiles $uiRoot @("settings.rs")
    if ($r11.Count -gt 0) {
        Write-NamedBundle `
            -Prefix "R11" `
            -Location "yandex_to_lastfm_rs\src\ui" `
            -What "settings" `
            -RootPath $ProjectRoot `
            -Files $r11 `
            -Title "rust source: yandex_to_lastfm_rs\src\ui\settings.rs"
    }

    $r15 = Get-ExistingFiles $uiRoot @("window.rs")
    if ($r15.Count -gt 0) {
        Write-NamedBundle `
            -Prefix "R15" `
            -Location "yandex_to_lastfm_rs\src\ui" `
            -What "window" `
            -RootPath $ProjectRoot `
            -Files $r15 `
            -Title "rust source: yandex_to_lastfm_rs\src\ui\window.rs"
    }
}

# -------------------- SUMMARY --------------------

Write-Host ("Done. Output in: {0}" -f $OutDir)

if (@($script:ChangedBundles).Count -gt 0) {
    Write-Host "Changed bundles:"
    foreach ($b in $script:ChangedBundles) {
        Write-Host ("  {0}: {1} (files: {2})" -f $b.Action, $b.Path, $b.Files)
    }
}
else {
    Write-Host "All bundles are up to date, no changes written."
}