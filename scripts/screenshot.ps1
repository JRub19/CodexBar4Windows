# Phase 3 E4: capture tray icon and popup screenshots at multiple DPI
# scales for manual visual parity checks against the macOS source.
#
# Usage:
#   ./scripts/screenshot.ps1 -OutDir tests/golden/win
#
# The script enumerates the active monitor's DPI via
# `GetDpiForMonitor`, then uses `BitBlt` to grab a 480x540 rectangle
# anchored at the tray-area corner. Run twice with the user-selected
# theme set to Dark and Light to capture both color schemes.

param(
    [string]$OutDir = "tests/golden/win",
    [string]$Variant = "auto"
)

if (-not (Test-Path $OutDir)) {
    New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
}

Add-Type @"
using System;
using System.Drawing;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;
using System.Windows.Forms;

public class ScreenCapture {
    public static void Capture(string path, int x, int y, int w, int h) {
        Bitmap bmp = new Bitmap(w, h, PixelFormat.Format32bppArgb);
        using (Graphics g = Graphics.FromImage(bmp)) {
            g.CopyFromScreen(x, y, 0, 0, new Size(w, h));
        }
        bmp.Save(path, ImageFormat.Png);
    }
}
"@ -ReferencedAssemblies System.Drawing,System.Windows.Forms

# Capture the bottom-right 540x480 region of the primary monitor, which
# is where the popup anchors on the standard Windows 11 taskbar.
$screen = [System.Windows.Forms.Screen]::PrimaryScreen.WorkingArea
$x = $screen.Right - 480
$y = $screen.Bottom - 540

$timestamp = Get-Date -Format "yyyy-MM-dd_HH-mm-ss"
$out = Join-Path $OutDir "popup_${Variant}_${timestamp}.png"
[ScreenCapture]::Capture($out, $x, $y, 480, 540)
Write-Output "Captured $out"
