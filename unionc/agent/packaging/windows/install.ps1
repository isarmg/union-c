param(
    [Parameter(Mandatory = $true)][string]$Binary,
    [Parameter(Mandatory = $true)][string]$Config,
    [switch]$ReplaceConfig
)

$ErrorActionPreference = "Stop"
$taskName = "UnionC Agent"
$root = Join-Path $env:ProgramData "UnionC Agent"

# Stop the old process before replacing its executable. Windows normally locks a
# running image, so upgrades must not copy over it first.
$existingTask = Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
if ($null -ne $existingTask) {
    Stop-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
    for ($attempt = 0; $attempt -lt 50; $attempt++) {
        if ((Get-ScheduledTask -TaskName $taskName).State -ne "Running") { break }
        Start-Sleep -Milliseconds 200
    }
    if ((Get-ScheduledTask -TaskName $taskName).State -eq "Running") {
        throw "The existing UnionC Agent task did not stop; the binary was not replaced."
    }
}

New-Item -ItemType Directory -Force -Path $root | Out-Null

# Use SID-based ACLs so installation is independent of the Windows display language.
# LOCAL SERVICE needs Modify for host-id, agent-token, and the bounded spool; no
# interactive user receives access to the deployment credential in config.json.
$inherit = [System.Security.AccessControl.InheritanceFlags]::ContainerInherit -bor `
    [System.Security.AccessControl.InheritanceFlags]::ObjectInherit
$acl = New-Object System.Security.AccessControl.DirectorySecurity
$acl.SetAccessRuleProtection($true, $false)
foreach ($entry in @(
    @("S-1-5-18", [System.Security.AccessControl.FileSystemRights]::FullControl),
    @("S-1-5-32-544", [System.Security.AccessControl.FileSystemRights]::FullControl),
    @("S-1-5-19", [System.Security.AccessControl.FileSystemRights]::Modify)
)) {
    $sid = New-Object System.Security.Principal.SecurityIdentifier($entry[0])
    $rule = New-Object System.Security.AccessControl.FileSystemAccessRule(
        $sid, $entry[1], $inherit,
        [System.Security.AccessControl.PropagationFlags]::None,
        [System.Security.AccessControl.AccessControlType]::Allow
    )
    $acl.AddAccessRule($rule)
}
Set-Acl -Path $root -AclObject $acl

$binaryTarget = Join-Path $root "unionc-agent.exe"
$binaryTemporary = Join-Path $root ("unionc-agent-{0}.new" -f [guid]::NewGuid())
Copy-Item $Binary $binaryTemporary
Move-Item -Force $binaryTemporary $binaryTarget

$configTarget = Join-Path $root "config.json"
if ($ReplaceConfig -or -not (Test-Path $configTarget)) {
    $configTemporary = Join-Path $root ("config-{0}.new" -f [guid]::NewGuid())
    Copy-Item $Config $configTemporary
    Move-Item -Force $configTemporary $configTarget
}

$action = New-ScheduledTaskAction `
    -Execute $binaryTarget `
    -Argument ('run --config "{0}"' -f $configTarget)
$trigger = New-ScheduledTaskTrigger -AtStartup
$principal = New-ScheduledTaskPrincipal -UserId "S-1-5-19" -LogonType ServiceAccount -RunLevel Limited
$settings = New-ScheduledTaskSettingsSet `
    -ExecutionTimeLimit ([TimeSpan]::Zero) `
    -RestartCount 999 `
    -RestartInterval (New-TimeSpan -Minutes 1) `
    -MultipleInstances IgnoreNew `
    -StartWhenAvailable `
    -AllowStartIfOnBatteries `
    -DontStopIfGoingOnBatteries
Register-ScheduledTask -TaskName $taskName -Action $action -Trigger $trigger `
    -Principal $principal -Settings $settings -Force | Out-Null
Start-ScheduledTask -TaskName $taskName
