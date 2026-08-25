# swCutter 应用图标生成：渐变圆角方块 + 白色网格(右下嵌套小方块=金字塔意象)
# 输出多尺寸 PNG 内嵌 ICO
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

function New-IconPng([int]$s) {
    $bmp = New-Object System.Drawing.Bitmap($s, $s)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = 'AntiAlias'

    # 圆角矩形路径
    $r = [Math]::Max(2, [int]($s * 0.22))
    function RoundedPath([single]$x,[single]$y,[single]$w,[single]$h,[single]$rad) {
        $p = New-Object System.Drawing.Drawing2D.GraphicsPath
        $d = $rad * 2
        $p.AddArc($x, $y, $d, $d, 180, 90)
        $p.AddArc($x + $w - $d, $y, $d, $d, 270, 90)
        $p.AddArc($x + $w - $d, $y + $h - $d, $d, $d, 0, 90)
        $p.AddArc($x, $y + $h - $d, $d, $d, 90, 90)
        $p.CloseFigure()
        return $p
    }

    # 背景：对角渐变
    $bgPath = RoundedPath 0 0 ($s - 1) ($s - 1) $r
    $brush = New-Object System.Drawing.Drawing2D.LinearGradientBrush(
        (New-Object System.Drawing.Point(0, 0)),
        (New-Object System.Drawing.Point($s, $s)),
        [System.Drawing.Color]::FromArgb(255, 79, 140, 255),
        [System.Drawing.Color]::FromArgb(255, 123, 92, 255))
    $g.FillPath($brush, $bgPath)

    # 白色 2x2 网格（右下格改为嵌套小方块）
    $m = [int]($s * 0.215); $gap = [Math]::Max(1, [int]($s * 0.075))
    $cell = [int](($s - 2 * $m - $gap) / 2)
    $cr = [Math]::Max(1, [int]($cell * 0.18))
    $white = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(236, 255, 255, 255))

    foreach ($pos in @(@(0,0), @(1,0), @(0,1))) {
        $x = $m + $pos[0] * ($cell + $gap); $y = $m + $pos[1] * ($cell + $gap)
        $cp = RoundedPath $x $y $cell $cell $cr
        $g.FillPath($white, $cp)
        $cp.Dispose()
    }
    # 右下：嵌套小方块（金字塔层级意象）
    $x2 = $m + ($cell + $gap); $y2 = $m + ($cell + $gap)
    $inner = [int]($cell * 0.52)
    $ix = $x2 + [int](($cell - $inner) / 2); $iy = $y2 + [int](($cell - $inner) / 2)
    $ip = RoundedPath $ix $iy $inner $inner ([Math]::Max(1,[int]($cr*0.7)))
    $g.FillPath($white, $ip)
    # 外框细线提示“缩放”
    $pen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(150, 255, 255, 255), [Math]::Max(1, $s / 64))
    $op = RoundedPath $x2 $y2 $cell $cell $cr
    $g.DrawPath($pen, $op)

    foreach ($d in @($bgPath, $op, $ip)) { $d.Dispose() }
    $pen.Dispose(); $white.Dispose(); $brush.Dispose()
    $g.Dispose()

    $ms = New-Object System.IO.MemoryStream
    $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    return $ms.ToArray()
}

$sizes = @(16, 24, 32, 48, 64, 128, 256)
$blobs = @{}
foreach ($sz in $sizes) { $blobs[$sz] = [byte[]](New-IconPng $sz) }

# 组装 ICO（PNG 条目）
$ms = New-Object System.IO.MemoryStream
$bw = New-Object System.IO.BinaryWriter($ms)
$bw.Write([uint16]0); $bw.Write([uint16]1); $bw.Write([uint16]$sizes.Count)
$offset = 6 + 16 * $sizes.Count
foreach ($sz in $sizes) {
    $data = $blobs[$sz]
    $wb = if ($sz -ge 256) { [byte]0 } else { [byte]$sz }
    $bw.Write($wb); $bw.Write($wb)          # width,height (0=256)
    $bw.Write([byte]0); $bw.Write([byte]0)  # colors,reserved
    $bw.Write([uint16]1); $bw.Write([uint16]32)
    $bw.Write([uint32]$data.Length)
    $bw.Write([uint32]$offset)
    $offset += $data.Length
}
foreach ($sz in $sizes) { $bw.Write([byte[]]$blobs[$sz]) }
$bw.Flush()
$total = $ms.Length
$out = Join-Path $PSScriptRoot '..\windows\runner\resources\app_icon.ico'
[System.IO.File]::WriteAllBytes((Resolve-Path (Split-Path $out)).Path + '\app_icon.ico', $ms.ToArray())
$bw.Dispose()
Write-Host "icon written -> $out ($total bytes)"
