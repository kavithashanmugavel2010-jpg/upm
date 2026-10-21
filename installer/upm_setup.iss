; UPM (Universal Package Manager) Inno Setup Script
; Creates a native Windows GUI Setup Wizard installer (e.g. UPM-Setup-0.1.0.exe)
; Compile using Inno Setup Compiler (ISCC.exe)

#define MyAppName "UPM"
#define MyAppFullName "UPM - Universal Package Manager"
#define MyAppVersion "0.1.0"
#define MyAppPublisher "UPM Contributors"
#define MyAppURL "https://github.com/upm-pkg/upm"
#define MyAppExeName "upm.exe"

[Setup]
AppId={{D37E6B20-B589-4D88-999B-4DCE9F0D9F58}
AppName={#MyAppFullName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}
DefaultDirName={localappdata}\upm\bin
DisableDirPage=no
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputBaseFilename=UPM-Setup-{#MyAppVersion}
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
ArchitecturesInstallIn64BitMode=x64
ArchitecturesAllowed=x64

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "addtopath"; Description: "Add UPM directory to User PATH environment variable (Recommended)"; GroupDescription: "System Environment Configuration:"; Flags: checkedonce

[Files]
Source: "..\target\release\upm.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: isreadme ignoreversion

[Registry]
; Configure User PATH in HKCU\Environment if the addtopath task is selected
Root: HKCU; Subkey: "Environment"; ValueType: expandsz; ValueName: "Path"; ValueData: "{code:GetNewPath}"; Tasks: addtopath; Flags: preservestringtype

[Run]
Filename: "{app}\{#MyAppExeName}"; Parameters: "info"; Description: "Verify hardware acceleration & installation status"; Flags: postinstall nowait skipifsilent

[Code]
const
  WM_SETTINGCHANGE = $001A;
  SMTO_ABORTIFHUNG = $0002;

function SendMessageTimeout(hWnd: Integer; Msg: Cardinal; wParam: Cardinal; lParam: String; fuFlags: Cardinal; uTimeout: Cardinal; out lpdwResult: Cardinal): Integer;
  external 'SendMessageTimeoutW@user32.dll stdcall';

// Checks if path is already present in PATH string
function IsPathInEnv(const PathToFind, EnvPath: String): Boolean;
var
  P: Integer;
  UpperPath, UpperEnv: String;
begin
  UpperPath := Uppercase(Trim(PathToFind));
  UpperEnv := Uppercase(EnvPath);
  
  Result := (Pos(';' + UpperPath + ';', ';' + UpperEnv + ';') > 0) or
            (Pos(';' + UpperPath, ';' + UpperEnv) = (Length(UpperEnv) - Length(UpperPath) + 1));
end;

function GetNewPath(Param: String): String;
var
  CurrentPath: String;
  AppDir: String;
begin
  AppDir := ExpandConstant('{app}');
  if not RegQueryStringValue(HKCU, 'Environment', 'Path', CurrentPath) then
    CurrentPath := '';

  if IsPathInEnv(AppDir, CurrentPath) then
    Result := CurrentPath
  else if Length(CurrentPath) = 0 then
    Result := AppDir
  else if CurrentPath[Length(CurrentPath)] = ';' then
    Result := CurrentPath + AppDir
  else
    Result := CurrentPath + ';' + AppDir;
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  Dummy: Cardinal;
begin
  if CurStep = ssPostInstall then
  begin
    // Broadcast setting change so explorer, CMD, and PowerShell recognize the updated PATH
    SendMessageTimeout($FFFF, WM_SETTINGCHANGE, 0, 'Environment', SMTO_ABORTIFHUNG, 3000, Dummy);
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  CurrentPath, AppDir, CleanedPath: String;
  P, Dummy: Cardinal;
  Paths: TArrayOfString;
  I: Integer;
begin
  if CurUninstallStep = usPostUninstall then
  begin
    AppDir := ExpandConstant('{app}');
    if RegQueryStringValue(HKCU, 'Environment', 'Path', CurrentPath) then
    begin
      CleanedPath := '';
      // Remove AppDir from registry path
      while Pos(AppDir + ';', CurrentPath) > 0 do
        Delete(CurrentPath, Pos(AppDir + ';', CurrentPath), Length(AppDir) + 1);
      while Pos(';' + AppDir, CurrentPath) > 0 do
        Delete(CurrentPath, Pos(';' + AppDir, CurrentPath), Length(AppDir) + 1);
      if Pos(AppDir, CurrentPath) > 0 then
        Delete(CurrentPath, Pos(AppDir, CurrentPath), Length(AppDir));

      RegWriteStringValue(HKCU, 'Environment', 'Path', CurrentPath);
      SendMessageTimeout($FFFF, WM_SETTINGCHANGE, 0, 'Environment', SMTO_ABORTIFHUNG, 3000, Dummy);
    end;
  end;
end;
