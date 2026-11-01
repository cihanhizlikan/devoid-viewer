# Startup and footprint benchmark for the release build.
#
# Measures what CLAUDE.md names as the one goal: how fast an image opens and
# how small the process stays. No instrumentation in the binary — everything is
# observed from outside, so the numbers describe the shipped exe.
#
#   time to visible  launch → the window becomes visible. With a path argument
#                    the window starts hidden and is shown only after the first
#                    image has decoded (main.rs, `ensure_visible`), so this is
#                    time-to-image; the paint follows in the same frame.
#   working set      after the window is visible and the process has settled.
#   peak working set over the whole run, including the decode.
#   private bytes    committed memory the process owns.
#
# Each fixture sits alone in its own directory, so no sibling preloads run and
# runs are comparable. Usage, from PowerShell in the repository root:
#
#   ./scripts/bench.ps1               # builds release, 15 runs per case
#   ./scripts/bench.ps1 -Runs 30 -NoBuild

param(
    [int]$Runs = 15,
    [int]$SettleMs = 1500,
    [switch]$NoBuild
)

$ErrorActionPreference = 'Stop'
$env:PATH = "$HOME\.cargo\bin;$env:PATH"
$root = Split-Path $PSScriptRoot -Parent
$exe  = Join-Path $root 'target\release\DevoidViewer.exe'

if (-not $NoBuild) {
    Push-Location $root
    try { cargo build --release --locked --quiet; if ($LASTEXITCODE) { throw "cargo build failed ($LASTEXITCODE)" } }
    finally { Pop-Location }
}
if (-not (Test-Path $exe)) { throw "missing $exe — build without -NoBuild" }

# ── Fixtures ──────────────────────────────────────────────────────────────────
# Generated, not committed: a smooth gradient with noise so the encoders cannot
# collapse it to nothing, at sizes a camera or screenshot really produces.
Add-Type -AssemblyName System.Drawing
$fixtures = Join-Path ([IO.Path]::GetTempPath()) 'devoid-bench'

function New-Fixture([string]$name, [int]$w, [int]$h, [string]$format) {
    $dir  = Join-Path $fixtures $name
    $file = Join-Path $dir "$name.$format"
    if (Test-Path $file) { return $file }
    New-Item -ItemType Directory -Force $dir | Out-Null
    $bmp  = [Drawing.Bitmap]::new($w, $h)
    $rect = [Drawing.Rectangle]::new(0, 0, $w, $h)
    $data = $bmp.LockBits($rect, 'WriteOnly', [Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $px   = [byte[]]::new($data.Stride * $h)
    $rng  = [Random]::new(1)
    for ($y = 0; $y -lt $h; $y++) {
        $row = $y * $data.Stride
        for ($x = 0; $x -lt $w; $x++) {
            $i = $row + $x * 4
            $n = $rng.Next(32)
            $px[$i]     = [byte]((($x * 255) / $w + $n) -band 255)
            $px[$i + 1] = [byte]((($y * 255) / $h + $n) -band 255)
            $px[$i + 2] = [byte](((($x + $y) * 127) / ($w + $h) + $n) -band 255)
            $px[$i + 3] = 255
        }
    }
    [Runtime.InteropServices.Marshal]::Copy($px, 0, $data.Scan0, $px.Length)
    $bmp.UnlockBits($data)
    $fmt = if ($format -eq 'png') { [Drawing.Imaging.ImageFormat]::Png } else { [Drawing.Imaging.ImageFormat]::Jpeg }
    $bmp.Save($file, $fmt)
    $bmp.Dispose()
    $file
}

$cases = [ordered]@{
    'no image (empty window)' = $null
    'PNG 1920x1080'           = New-Fixture 'png-1080p' 1920 1080 'png'
    'JPEG 4000x3000 (12 MP)'  = New-Fixture 'jpeg-12mp' 4000 3000 'jpg'
}

# ── Measurement ───────────────────────────────────────────────────────────────
# Process.MainWindowHandle is the wrong probe: winit's hidden helper window
# (class "Winit Thread Event Target") reports WS_VISIBLE from the first
# milliseconds, long before the viewer window is shown. Wait instead for a
# visible top-level window of the process that is not that helper. Polled in
# C# so the loop itself adds well under a millisecond.
Add-Type -TypeDefinition @'
using System;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;
public static class ViewerWindow {
    delegate bool EnumProc(IntPtr h, IntPtr l);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc f, IntPtr l);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassNameW(IntPtr h, StringBuilder s, int n);

    static bool IsShown(uint pid) {
        bool found = false;
        EnumWindows((h, l) => {
            uint owner;
            GetWindowThreadProcessId(h, out owner);
            if (owner != pid || !IsWindowVisible(h)) return true;
            var cls = new StringBuilder(64);
            GetClassNameW(h, cls, cls.Capacity);
            if (cls.ToString() == "Winit Thread Event Target") return true;
            found = true;
            return false;
        }, IntPtr.Zero);
        return found;
    }

    // Milliseconds from `sw` start until the viewer window is visible.
    public static double WaitShown(Process p, Stopwatch sw, int timeoutMs) {
        while (!IsShown((uint)p.Id)) {
            if (p.HasExited) throw new Exception("exited with " + p.ExitCode + " before showing a window");
            if (sw.ElapsedMilliseconds > timeoutMs) throw new Exception("no window after " + timeoutMs + " ms");
        }
        return sw.Elapsed.TotalMilliseconds;
    }
}
'@

function Measure-Run([string]$image) {
    $psi = [Diagnostics.ProcessStartInfo]::new($exe)
    if ($image) { $psi.ArgumentList.Add($image) }
    $psi.UseShellExecute = $false
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $p  = [Diagnostics.Process]::Start($psi)
    try {
        $visible = [ViewerWindow]::WaitShown($p, $sw, 10000)
        Start-Sleep -Milliseconds $SettleMs
        $p.Refresh()
        [pscustomobject]@{
            VisibleMs = $visible
            WsMB      = $p.WorkingSet64        / 1MB
            PeakMB    = $p.PeakWorkingSet64    / 1MB
            PrivateMB = $p.PrivateMemorySize64 / 1MB
        }
    }
    finally { if (-not $p.HasExited) { $p.Kill() }; $p.WaitForExit() }
}

function Median([double[]]$xs) {
    $s = $xs | Sort-Object
    if ($s.Count % 2) { $s[[int][Math]::Floor($s.Count / 2)] } else { ($s[$s.Count / 2 - 1] + $s[$s.Count / 2]) / 2 }
}

$size = (Get-Item $exe).Length
"DevoidViewer.exe  {0:N0} bytes ({1:N2} MB)   runs per case: {2}" -f $size, ($size / 1MB), $Runs
''
$rows = foreach ($name in $cases.Keys) {
    $image = $cases[$name]
    Measure-Run $image | Out-Null   # warm the file cache; not counted
    $r = 1..$Runs | ForEach-Object { Measure-Run $image }
    [pscustomobject]@{
        'Case'              = $name
        'Visible ms (med)'  = '{0:N1}' -f (Median $r.VisibleMs)
        'Visible ms (min)'  = '{0:N1}' -f ($r.VisibleMs | Measure-Object -Minimum).Minimum
        'Working set MB'    = '{0:N1}' -f (Median $r.WsMB)
        'Peak WS MB'        = '{0:N1}' -f (Median $r.PeakMB)
        'Private MB'        = '{0:N1}' -f (Median $r.PrivateMB)
    }
}
$rows | Format-Table -AutoSize
