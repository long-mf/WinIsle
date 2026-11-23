; WinIsle — Inno Setup 安装脚本（按用户安装，无需管理员）
; CI 用法：iscc /DAppVersion=0.2.0 installer\WinIsle.iss
#ifndef AppVersion
  #define AppVersion "0.2.0"
#endif
#define AppName "WinIsle"
#define AppExe "WinIsle.exe"

[Setup]
AppId={{C2E5A7B4-9D31-4F68-8A1E-3B7C9D2F5A61}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher=WinIsle
; 数据默认存放在安装目录的 data\ 下（跟随安装目录，不写用户目录）；可按需改选安装盘。
DefaultDirName={localappdata}\Programs\WinIsle
DefaultGroupName=WinIsle
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
OutputDir=..\dist
OutputBaseFilename=WinIsle-Setup-{#AppVersion}
SetupIconFile=..\assets\winisle.ico
UninstallDisplayIcon={app}\{#AppExe}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
CloseApplications=yes
RestartApplications=yes
MinVersion=10.0

[Languages]
Name: "chinesesimp"; MessagesFile: "ChineseSimplified.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "创建桌面快捷方式"; GroupDescription: "附加任务:"; Flags: unchecked
Name: "autostart"; Description: "开机自动启动 WinIsle"; GroupDescription: "附加任务:"; Flags: unchecked

[Files]
Source: "..\target\release\winisle.exe"; DestDir: "{app}"; DestName: "{#AppExe}"; Flags: ignoreversion

[Icons]
Name: "{group}\WinIsle"; Filename: "{app}\{#AppExe}"
Name: "{group}\卸载 WinIsle"; Filename: "{uninstallexe}"
Name: "{userdesktop}\WinIsle"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "WinIsle"; ValueData: """{app}\{#AppExe}"""; Flags: uninsdeletevalue; Tasks: autostart

[Run]
Filename: "{app}\{#AppExe}"; Description: "运行 WinIsle"; Flags: nowait postinstall skipifsilent

; 卸载时兜底清理快捷方式（即使安装日志缺失或用户曾手工创建同名快捷方式也能删干净）
[UninstallDelete]
Type: files; Name: "{userdesktop}\WinIsle.lnk"
Type: files; Name: "{group}\WinIsle.lnk"
Type: files; Name: "{group}\卸载 WinIsle.lnk"
Type: dirifempty; Name: "{group}"

[UninstallRun]
Filename: "taskkill.exe"; Parameters: "/IM {#AppExe} /F"; Flags: runhidden; RunOnceId: "KillWinIsle"
