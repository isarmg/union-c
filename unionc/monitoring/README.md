# UnionC 标准时序监控栈

该目录提供可选的 OTLP → VictoriaMetrics → Grafana 链路。UnionC 自身仍保存主机最新
快照和有限历史，供 BackC 使用；本监控栈用于更长保留期、PromQL、Grafana 和后续告警。

```bash
cd unionc/monitoring
export GRAFANA_ADMIN_PASSWORD='replace-me'
docker compose up -d
```

默认所有端口只监听 Linux 服务端回环地址。远程 Agent 应通过反向代理的 443/mTLS
访问 Collector，不能直接公开 4318、8428 或 Grafana。Agent 配置中的 OTLP URL 应为
`https://telemetry.example.com/v1/metrics`。

生产环境必须：

- 固定镜像 digest，并按变更窗口升级；
- 为每台 Agent 签发独立客户端证书；
- 备份 VictoriaMetrics 数据卷和 Grafana 配置；
- 根据 `hosts × series_per_host ÷ interval` 实测调整保留期和资源；
- 在入口限制请求体、证书身份、时间戳偏移和速率。

该 OTLP 入口是可选的同一信任域长时序通道，不是多租户安全边界：客户端证书证明设备
属于这套部署，但标准 Collector 不会自动验证证书 SAN 是否等于 OTLP 的 `host.id`。因此
不要跨租户共享 Agent CA；需要强主机隔离时，应在入口增加能把证书身份绑定到资源属性的
认证网关，并限制速率、标签数和时序基数。BackC 展示以 UnionC 经每主机 token 验证的
JSON 报告为权威数据源。
