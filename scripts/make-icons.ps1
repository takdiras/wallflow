# Generates src-tauri/icons/icon.png and icon.ico (PNG-compressed, multi-size).
Add-Type -AssemblyName System.Drawing
$out = Join-Path $PSScriptRoot '..\src-tauri\icons'
New-Item -ItemType Directory -Force $out | Out-Null

function New-IconBitmap([int]$size) {
    $bmp = New-Object System.Drawing.Bitmap $size, $size, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = 'AntiAlias'
    $g.Clear([System.Drawing.Color]::Transparent)
    $s = [float]$size
    $r = $s * 0.24
    $path = New-Object System.Drawing.Drawing2D.GraphicsPath
    $path.AddArc(0, 0, $r, $r, 180, 90); $path.AddArc($s - $r - 1, 0, $r, $r, 270, 90)
    $path.AddArc($s - $r - 1, $s - $r - 1, $r, $r, 0, 90); $path.AddArc(0, $s - $r - 1, $r, $r, 90, 90)
    $path.CloseFigure()
    $bg = New-Object System.Drawing.Drawing2D.LinearGradientBrush (New-Object System.Drawing.PointF 0, 0), (New-Object System.Drawing.PointF $s, $s), ([System.Drawing.Color]::FromArgb(255, 99, 102, 241)), ([System.Drawing.Color]::FromArgb(255, 236, 72, 153))
    $g.FillPath($bg, $path)
    $g.SetClip($path)
    # sun
    $sun = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(230, 255, 214, 120))
    $g.FillEllipse($sun, $s * 0.58, $s * 0.18, $s * 0.22, $s * 0.22)
    # back hill (glassy)
    $back = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(110, 255, 255, 255))
    $g.FillPolygon($back, @((New-Object System.Drawing.PointF 0, ($s * 0.78)), (New-Object System.Drawing.PointF ($s * 0.36), ($s * 0.38)), (New-Object System.Drawing.PointF ($s * 0.72), ($s * 0.80)), (New-Object System.Drawing.PointF 0, $s)))
    # front hill
    $front = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(235, 255, 255, 255))
    $g.FillPolygon($front, @((New-Object System.Drawing.PointF ($s * 0.2), $s), (New-Object System.Drawing.PointF ($s * 0.62), ($s * 0.50)), (New-Object System.Drawing.PointF $s, ($s * 0.86)), (New-Object System.Drawing.PointF $s, $s)))
    $g.Dispose()
    return $bmp
}

function Get-PngBytes($bmp) {
    $ms = New-Object System.IO.MemoryStream
    $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
    return , $ms.ToArray()
}

$big = New-IconBitmap 512
$big.Save((Join-Path $out 'icon.png'), [System.Drawing.Imaging.ImageFormat]::Png)

$sizes = 256, 64, 48, 32, 24, 16
$pngs = foreach ($sz in $sizes) { , (Get-PngBytes (New-IconBitmap $sz)) }
$ms = New-Object System.IO.MemoryStream
$w = New-Object System.IO.BinaryWriter $ms
$w.Write([uint16]0); $w.Write([uint16]1); $w.Write([uint16]$sizes.Count)
$offset = 6 + 16 * $sizes.Count
for ($i = 0; $i -lt $sizes.Count; $i++) {
    $sz = $sizes[$i]; $d = $pngs[$i]
    $w.Write([byte]($sz % 256)); $w.Write([byte]($sz % 256)); $w.Write([byte]0); $w.Write([byte]0)
    $w.Write([uint16]1); $w.Write([uint16]32); $w.Write([uint32]$d.Length); $w.Write([uint32]$offset)
    $offset += $d.Length
}
foreach ($d in $pngs) { $w.Write($d) }
$w.Flush()
[System.IO.File]::WriteAllBytes((Join-Path $out 'icon.ico'), $ms.ToArray())
Write-Host "Icons written to $out"
