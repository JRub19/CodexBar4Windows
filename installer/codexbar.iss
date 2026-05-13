; Inno Setup script for CodexBar4Windows. Phase 9 §E.
;
; Build with:
;   $env:CODEXBAR_VERSION = "1.0.0"
;   $env:CODEXBAR_ARCH    = "x64"
;   & "C:\Program Files (x86)\Inno Setup 6\iscc.exe" /Qp installer\codexbar.iss
;
; Two-stage Authenticode signing: inner EXEs are signed by
; `scripts/sign-binaries.ps1` before this script runs; the installer
; itself is signed by the SignTool= directive below at the end of
; compile. Together that gives signtool /pa verification on every
; artifact a user can launch.

#define MarketingVersion GetEnv("CODEXBAR_VERSION")
#define Arch             GetEnv("CODEXBAR_ARCH")
#if MarketingVersion == ""
  #define MarketingVersion "0.1.0-pre.0"
#endif
#if Arch == ""
  #define Arch "x64"
#endif

; Tauri release output lives next to the workspace's `target/` dir.
; The IS script is invoked from the repo root, so `..\target\release`
; is the right relative path from `installer/`.
#define SourceBase "..\target\release"
#define AssetsBase "..\apps\desktop-tauri\src-tauri\icons"
#define DistDir    "..\dist"

[Setup]
AppId={{B7C2A6A0-8C1D-4C0D-9F0C-9C0D5F0A1234}}
AppName=CodexBar4Windows
AppVersion={#MarketingVersion}
AppPublisher=CodexBar4Windows
AppPublisherURL=https://github.com/JRub19/CodexBar4Windows
AppSupportURL=https://github.com/JRub19/CodexBar4Windows/issues
AppUpdatesURL=https://github.com/JRub19/CodexBar4Windows/releases
DefaultDirName={localappdata}\Programs\CodexBar4Windows
DefaultGroupName=CodexBar4Windows
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog commandline
ArchitecturesInstallIn64BitMode=x64
ArchitecturesAllowed=x64
OutputDir={#DistDir}
OutputBaseFilename=CodexBar4Windows-{#MarketingVersion}-{#Arch}
SetupIconFile={#AssetsBase}\icon.ico
WizardStyle=modern
Compression=lzma2/ultra
SolidCompression=yes
DisableProgramGroupPage=yes
UninstallDisplayIcon={app}\CodexBar4Windows.exe
LicenseFile=..\LICENSE

; Authenticode signing applied to the final installer and to any
; entries marked with the `sign` flag in [Files]. The `signtool`
; configuration is registered in Inno's IDE / passed via /Sname on
; the iscc command line in CI.
SignTool=signtool sign /tr http://timestamp.digicert.com /td SHA256 /fd SHA256 /a $f
SignedUninstaller=yes

[Languages]
Name: "en";     MessagesFile: "compiler:Default.isl"
Name: "ptBR";   MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"
Name: "zhHans"; MessagesFile: "compiler:Languages\ChineseSimplified.isl"

[Files]
; Tauri produces the desktop shell as `<productName>.exe`; the
; auxiliary binaries land alongside it in target/release.
Source: "{#SourceBase}\CodexBar4Windows.exe";                  DestDir: "{app}"; Flags: ignoreversion sign
Source: "{#SourceBase}\codexbar4windows-claude-watchdog.exe";  DestDir: "{app}"; Flags: ignoreversion sign skipifsourcedoesntexist
Source: "{#AssetsBase}\icon.ico";                              DestDir: "{app}"; Flags: ignoreversion

[Tasks]
Name: "launchatlogin"; Description: "{cm:LaunchAtLogin}"; Flags: checkedonce
Name: "desktopicon";   Description: "{cm:DesktopIcon}";   Flags: unchecked

[Icons]
Name: "{group}\CodexBar4Windows"; Filename: "{app}\CodexBar4Windows.exe"
Name: "{group}\Uninstall CodexBar4Windows"; Filename: "{uninstallexe}"
Name: "{commondesktop}\CodexBar4Windows"; Filename: "{app}\CodexBar4Windows.exe"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run";
   ValueType: string; ValueName: "CodexBar4Windows";
   ValueData: """{app}\CodexBar4Windows.exe"" --minimized";
   Flags: uninsdeletevalue; Tasks: launchatlogin
Root: HKCU; Subkey: "Software\Classes\codexbar4windows"; ValueType: string;
   ValueName: ""; ValueData: "URL:CodexBar4Windows Protocol"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\codexbar4windows"; ValueType: string;
   ValueName: "URL Protocol"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\codexbar4windows\shell\open\command";
   ValueType: string; ValueName: "";
   ValueData: """{app}\CodexBar4Windows.exe"" ""--launch=%1"""

[Run]
; WebView2 evergreen bootstrap. Skipped silently on hosts that already
; have the runtime; the NeedsWebView2 check inspects the EdgeUpdate
; clients key.
Filename: "{tmp}\MicrosoftEdgeWebview2Setup.exe"; Parameters: "/silent /install";
   Flags: skipifsilent waituntilterminated; Check: NeedsWebView2;
   StatusMsg: "{cm:InstallingWebView2}"
Filename: "{app}\CodexBar4Windows.exe"; Description: "{cm:LaunchProgram,CodexBar4Windows}";
   Flags: nowait postinstall skipifsilent

[CustomMessages]
en.LaunchAtLogin=Start CodexBar4Windows when I sign in
en.DesktopIcon=Create a desktop shortcut
en.InstallingWebView2=Installing Microsoft Edge WebView2 runtime...
ptBR.LaunchAtLogin=Iniciar CodexBar4Windows quando eu fizer login
ptBR.DesktopIcon=Criar um atalho na area de trabalho
ptBR.InstallingWebView2=Instalando o runtime Microsoft Edge WebView2...
zhHans.LaunchAtLogin=登录时启动 CodexBar4Windows
zhHans.DesktopIcon=创建桌面快捷方式
zhHans.InstallingWebView2=正在安装 Microsoft Edge WebView2 运行时...

[Code]
function NeedsWebView2: Boolean;
var
  Value: String;
begin
  // Edge WebView2 Evergreen GUID. Present after the runtime is
  // installed system-wide. Inno's RegQueryStringValue returns False
  // when the value or the entire key is missing.
  Result := not RegQueryStringValue(
    HKLM,
    'SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}',
    'pv', Value);
end;
