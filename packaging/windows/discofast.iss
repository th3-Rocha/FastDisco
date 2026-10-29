; Discofast's Windows installer, built by the release workflow with Inno
; Setup: iscc /DVersion=<version> /DSource=<folder with discofast.exe>
; /O<output folder> packaging\windows\discofast.iss
; Installed for the current user, so it needs no administrator and the
; app can update itself in place.

#ifndef Version
  #define Version "0.0.0"
#endif

[Setup]
AppId={{6F0B8E52-0E4B-4D6C-9C8B-2D1A9C3E7F11}
AppName=Discofast
AppVersion={#Version}
AppPublisher=th3-Rocha
AppPublisherURL=https://github.com/th3-Rocha/FastDisco
DefaultDirName={localappdata}\Programs\Discofast
DefaultGroupName=Discofast
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputBaseFilename=discofast-{#Version}-x86_64-setup
Compression=lzma2
SolidCompression=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
UninstallDisplayIcon={app}\discofast.exe
WizardStyle=modern

[Languages]
Name: "brazilianportuguese"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "{#Source}\discofast.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Source}\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Source}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
; Lets the installed copy update itself, as the unpacked archive does.
Source: "{#Source}\discofast-portable.txt"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Discofast"; Filename: "{app}\discofast.exe"
Name: "{autodesktop}\Discofast"; Filename: "{app}\discofast.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\discofast.exe"; Description: "{cm:LaunchProgram,Discofast}"; Flags: nowait postinstall skipifsilent
