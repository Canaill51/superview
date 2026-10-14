; Superview's Windows installer (Inno Setup 6), built by .github/workflows/release.yml.
;
;   iscc /DAppVersion=0.2.9 /DSourceDir=build packaging\windows\superview.iss
;       -> build\superview-setup.exe
;
; SourceDir holds the four files the installer carries: superview.exe, ffmpeg.exe,
; ffprobe.exe and THIRD_PARTY_NOTICES.md. ffmpeg and ffprobe go in the same folder as
; superview.exe on purpose -- it is the first place the application looks for them.
;
; Installs for the current user by default (no administrator rights), or for everyone if
; the user asks. Start menu entry always, desktop shortcut optional, removable from
; Windows' Settings.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceDir
  #define SourceDir "..\..\build"
#endif

[Setup]
; Never change this identifier: it is what lets a new version install over an old one
; instead of next to it.
AppId={{5DA85DA3-BBBB-412C-8100-6006ABE9DB5C}
AppName=Superview
AppVersion={#AppVersion}
AppVerName=Superview {#AppVersion}
AppPublisher=Canaill51
VersionInfoVersion={#AppVersion}
DefaultDirName={autopf}\Superview
DefaultGroupName=Superview
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#SourceDir}
OutputBaseFilename=superview-setup
SetupIconFile=superview.ico
UninstallDisplayIcon={app}\superview.exe
UninstallDisplayName=Superview
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
; Close Superview if it is open while an update is installed.
CloseApplications=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SourceDir}\superview.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\ffmpeg.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\ffprobe.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\THIRD_PARTY_NOTICES.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Superview"; Filename: "{app}\superview.exe"
Name: "{autodesktop}\Superview"; Filename: "{app}\superview.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\superview.exe"; Description: "{cm:LaunchProgram,Superview}"; Flags: nowait postinstall skipifsilent
