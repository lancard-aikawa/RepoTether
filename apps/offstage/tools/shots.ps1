# Take the README screenshots of Offstage from a fictional demo (no real file names).
#   python -I apps/offstage/tools/make_demo.py <demo>
#   powershell -NoProfile -ExecutionPolicy Bypass -File apps/offstage/tools/shots.ps1 -Demo <demo> -Exe <offstage.exe> -OutDir docs/offstage/images
# 1st run records everything, then one memo is deleted and one is moved, and the 2nd..4th runs take the pictures.
param([string]$Demo, [string]$Exe, [string]$OutDir)

$shoot = Join-Path $PSScriptRoot "shoot.ps1"
$env:OFFSTAGE_DB = Join-Path $Demo "index.sqlite"
$env:OFFSTAGE_ROOTS = Join-Path $Demo "roots"
$env:OFFSTAGE_PROJECTS = Join-Path $Demo "projects"
$env:OFFSTAGE_AUTOREFRESH = "1"
New-Item -ItemType Directory -Force $OutDir | Out-Null

function Shot([string]$name, [string]$select, [string]$view, [string]$scene) {
  $env:OFFSTAGE_SELECT = $select
  $env:OFFSTAGE_VIEW = $view
  $env:OFFSTAGE_OPEN_SCENE = $scene
  $p = Start-Process $Exe -PassThru
  if ($name) {
    & powershell -NoProfile -ExecutionPolicy Bypass -File $shoot -Out (Join-Path $OutDir $name) -WaitSec 6 -ProcessId $p.Id
  } else {
    Start-Sleep -Seconds 6
  }
  Stop-Process -Id $p.Id
  Start-Sleep -Seconds 1
}

# 1st run: record (no picture)
Shot "" "" "table" ""

$na = Join-Path $Demo "roots\notes-app"
Remove-Item (Join-Path $na "sandbox\old-draft.md")
New-Item -ItemType Directory (Join-Path $na "sandbox\archive") | Out-Null
Move-Item (Join-Path $na "sandbox\interview-memo.md") (Join-Path $na "sandbox\archive\interview-memo.md")

Shot "list.png" "notes-app/.work/demo_api_token.json" "table" ""
Shot "tree.png" "notes-app/sandbox/old-draft.md" "tree" ""
Shot "scene.png" "notes-app/sandbox/old-draft.md" "table" "1"
