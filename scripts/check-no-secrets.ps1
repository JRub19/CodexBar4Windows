# Secret leakage heuristic. Run by CI on every push.
#
# Fails the job if any file under the source roots matches a known
# commercial credential pattern. Test fixtures that intentionally
# include these patterns must be listed in $allowlist.
#
# The script is a heuristic, not a guarantee. The primary defense is
# `SensitiveString` plus the tracing redaction layer; this is the safety
# net for "someone pasted a real token into a code comment".

$ErrorActionPreference = "Stop"

$roots = @("rust\src", "apps\desktop-tauri\src", "apps\desktop-tauri\src-tauri\src")
$includeExt = @("*.rs", "*.ts", "*.tsx", "*.json", "*.md")

# Files known to intentionally include credential-shaped strings for
# tests or documentation. Add new entries here when CI flags a new
# false positive that has been reviewed.
$allowlist = @(
    "rust\src\redact\mod.rs",
    "rust\src\redact\tracing_layer.rs"
)

# Patterns that are very unlikely to appear outside real credentials.
$patterns = @(
    "ghp_[A-Za-z0-9]{30,}",
    "gho_[A-Za-z0-9]{30,}",
    "ghu_[A-Za-z0-9]{30,}",
    "github_pat_[A-Za-z0-9_]{60,}",
    "AKIA[0-9A-Z]{16}",
    "AIza[0-9A-Za-z\-_]{30,}",
    "ya29\.[0-9A-Za-z\-_]{30,}"
)

$hits = New-Object System.Collections.ArrayList

foreach ($root in $roots) {
    if (-not (Test-Path $root)) { continue }
    $files = Get-ChildItem -Path $root -Recurse -Include $includeExt -File -ErrorAction SilentlyContinue |
        Where-Object {
            $rel = $_.FullName.Substring($PWD.Path.Length).TrimStart('\','/')
            -not ($allowlist | Where-Object { $rel -like "*$_" })
        }
    foreach ($pat in $patterns) {
        $matches = Select-String -Path $files.FullName -Pattern $pat -ErrorAction SilentlyContinue
        foreach ($m in $matches) {
            [void]$hits.Add("$($m.RelativePath($PWD.Path)):$($m.LineNumber): $($m.Line.Trim())")
        }
    }
}

if ($hits.Count -gt 0) {
    Write-Output ""
    Write-Output "Potential secret leak detected:"
    Write-Output ""
    foreach ($h in $hits) { Write-Output "  $h" }
    Write-Output ""
    Write-Output "Either remove the literal secret, or if this is a"
    Write-Output "deliberate test fixture, add the file to `$allowlist in"
    Write-Output "scripts\check-no-secrets.ps1."
    exit 1
}

Write-Output "Secret scan: no hits across $($roots.Count) source roots."
