# Server Discovery 协议

Server Discovery 在标准 VNC/RFB 连接建立后，交换 LL Remote Server 的身份声明、只读策略和可选 Agent 连接描述。它通过 RFB Fence 扩展承载；画面、鼠标和键盘继续使用原生 RFB 会话。

基础传输和模块关系见 [架构](../../Architecture.md)，Session 与共享行为分别见 [Session](../../src/session/README.md) 和 [Server](../../src/server/README.md)。

## Agent 会话与信任边界

Discovery 的 manifest 可携带短期 Agent 引导票据 `bootstrapKey` 及到期时间 `expiresAt`。客户端在独立 Agent 连接上使用票据创建会话，然后核对 Agent 返回的描述和 Server ID。Discovery 收到的远端声明保存在 `serverDeclaration`，身份确认结果单独保存在 `serverIdentity`。客户端忽略对方在 `identityVerification` 中自报的验证状态。

对远端而言，`managed/authenticated-agent` 表示当前 RFB 声明与配套 Agent 会话的 ID 一致。票据由当前 RFB 连接传递，两条连接均为明文。长期配对、设备密钥固定和 TLS 身份验证均未实现。

## 当前身份状态契约

- 收到有效 manifest 后，只记录非敏感的 ID、vendor、product 和 buildVersion 声明；远端保持 `unknown/none`，声明的能力不会直接生效。
- 本机 endpoint 与可信运行态匹配时，可保留 `managed/local-runtime`；对方传来的同名字段无法创建这种本机证据。
- 远端仅在 `agent.session.open` 获得会话、`agent.session.describe` 成功且 Server ID 与本次 RFB 声明一致后变为 `managed/authenticated-agent`。本机已知只读策略也必须一致。声明中的品牌和版本不复制为已认证产品元数据。
- Agent 描述必须包含非空 ID、布尔只读标志和字符串能力列表；有效能力最多 256 项，每项最多 256 字符。非法描述、ID/策略不匹配或认证失败清除远端认证证据与 Agent 能力，基础 VNC 不变。
- 重新发现及 Discovery 错误会撤销上一代远端认证、轮询和在途认证结果，重新完成认证前不保留其 `managed` 状态。撤回、过期、未知 Agent 协议和畸形 manifest 均不阻断基础 VNC。旧代的迟到结果不更新新模型。

## 协商与生命周期

1. Client 在 `SetEncodings` 中声明支持 Fence pseudo-encoding `-312`。
2. Server 在收到该声明后，发送 `VWSD` 格式的 `HELLO`。
3. Client 对 Fence Request 作标准回应，随后发送 `DISCOVER`；当前请求正文为 JSON `{"version":"1.0"}`。
4. Server 返回 `MANIFEST` 或 `ERROR`，需要时分片；Client 重组后再发布 Discovery 事件。
5. manifest 提供 Agent 描述时，Client 在独立 Agent 连接上执行 `agent.session.open` 与 `agent.session.describe`，随后按实际能力开放增强动作。

不支持本扩展的第三方 Server 继续使用标准 RFB。Agent 缺失、认证失败或增强操作失败只影响增强状态，不作为基础 VNC 会话的连接前置条件。重连和连接销毁会重新建立或释放增强状态。

标准 Fence 回应保持 payload 不变并清除 Request 标志；应用回复使用新的 Fence Request。当前实现不承诺 BlockBefore、BlockAfter 或 SyncNext 同步屏障，也不能将 Fence 回应当作 Canvas 已实际显示的证据。

## Discovery 帧

每个 Fence payload 最多 64 bytes，前 10 bytes 为 Discovery 头；多字节整数采用 big-endian。

| 偏移 | 长度 | 字段 | 当前含义 |
| --- | --- | --- | --- |
| 0 | 4 | magic | ASCII `VWSD`。 |
| 4 | 1 | major | 当前为 `1`。 |
| 5 | 1 | type | `1 HELLO`、`2 DISCOVER`、`3 MANIFEST`、`4 ERROR`。 |
| 6 | 1 | requestId | 标识同一次请求与回复。 |
| 7 | 1 | flags | bit 0 表示后续还有分片。 |
| 8 | 1 | sequence | 从 0 开始的分片序号。 |
| 9 | 1 | total | 分片总数，范围 1–255。 |
| 10 | 0–54 | body | JSON UTF-8 正文的一段。 |

重组要求各分片的 type、requestId、total 一致，且 sequence 连续；当前客户端同一时间只缓存一个未完成交换。正文还受 16 KiB 检查和最多 255 个分片的限制。非法序列或正文通过独立 Discovery 故障事件报告。

## Manifest 与 Agent

当前 manifest 的类型定义见 [serverDiscovery.ts](../../src/session/discovery/serverDiscovery.ts)，Server 生成逻辑见 [server_discovery.rs](../../src-tauri/src/server_discovery.rs)。

| 字段 | 用途 |
| --- | --- |
| `version` | manifest 版本。 |
| `server` | Server ID、vendor、product、buildVersion；兼容旧端保留的 `identityVerification` 仅是 wire 声明，不是客户端证据。 |
| `policy.viewOnly` | Server 的只读策略。 |
| `capabilities` | Server 声明的能力列表；Agent 认证后的能力由 describe 结果更新。 |
| `agent` | 可选 Agent 描述：`hostMode`、`port`、`protocolVersion`、`authenticationMethods`，以及短期 `bootstrapKey` 和 `expiresAt`。 |

Agent endpoint 使用当前 RFB peer 地址与声明端口，不通过 VNC 端口加偏移猜测。Client 检查票据期限，并将 bootstrapKey 用于独立 Agent Transport 的 session open；成功后通过 describe 获取身份和能力，认证失败时标记增强通道降级。

当前 VNC 认证不提供传输加密，Discovery 也不增加传输加密或独立服务端身份认证。bootstrapKey 与 Agent session 信息属于短期凭据，不写入连接模型、日志、历史、导出或诊断报告。未经认可的远端认证错误正文不复制到模型或诊断中，以免回显凭据。

## 实现入口

| 范围 | 文件 |
| --- | --- |
| 标准 Fence framing | [rfb_fence.rs](../../src-tauri/src/remote_desktop_core/rfb_fence.rs) |
| Discovery 编解码与重组 | [fence.rs](../../src-tauri/src/remote_desktop_core/fence.rs) |
| 原生 Client 接收与事件 | [rfb_client_session.rs](../../src-tauri/src/remote_desktop_core/rfb_client_session.rs) |
| Server HELLO、manifest 与分片回复 | [server_discovery.rs](../../src-tauri/src/server_discovery.rs) |
| Server 消息调度 | [client_loop.rs](../../src-tauri/src/local_vnc_server_protocol/client_loop.rs) |
| 前端 manifest 类型与不可信输入校验 | [serverDiscovery.ts](../../src/session/discovery/serverDiscovery.ts) |
| 连接身份与能力模型 | [connectionModel.ts](../../src/session/connection-model/connectionModel.ts) |
| Agent 建链与释放 | [nativeAgentRuntime.ts](../../src/session/protocol/protocolAdapter/nativeAgentRuntime.ts) |

## 验证范围

相关自动化回归见 [remote_desktop_core_tests.rs](../../src-tauri/tests/remote_desktop_core_tests.rs)、[protocolAdapter.test.ts](../../tests/frontend/session/protocolAdapter.test.ts) 和 [nativeAgentRuntime.test.ts](../../tests/frontend/session/nativeAgentRuntime.test.ts)。它们覆盖协议协商、分片和增强生命周期等代码行为。

第三方 Server 兼容性、Agent 重启恢复、显示器与剪贴板能力、真实画面和系统权限仍需对应平台的 VNC / Tauri shell 手测；自动化通过不代表这些桌面行为已完成验收。
