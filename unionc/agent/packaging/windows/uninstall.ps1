$ErrorActionPreference = "Stop"
Stop-ScheduledTask -TaskName "UnionC Agent" -ErrorAction SilentlyContinue
Unregister-ScheduledTask -TaskName "UnionC Agent" -Confirm:$false -ErrorAction SilentlyContinue
# 有意保留 ProgramData 下的 host-id、配置和有界 spool，防止重装后身份漂移。

