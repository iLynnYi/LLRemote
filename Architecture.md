# Architecture

本文档是 LL Remote 的高层架构入口，用于理解系统边界、模块关系和数据流。开发与验证命令见 [README.md](README.md)。

## 分层结构

```text
Vue / Naive UI 工作台
  |
  | composables / feature components
  v
前端应用编排层
  |
  | ProtocolAdapter / SessionManager / Host manager
  v
Tauri Commands / Events
  |
  v
Rust App Core
  |
  | native RFB Core / Surface / storage / credential vault / local server / agent / artifacts
  v
系统能力、SQLite、OS Credential Store、VNC Server、Agent 通道
```

## 模块职责边界

模块职责和测试入口见 [Hosts](src/hosts/README.md)、[Session](src/session/README.md)、[Server](src/server/README.md) 和 [Rust 后端](src-tauri/src/README.md)。

- 前端负责工作台 UI、用户交互、页面级流程和状态展示，只通过纯数据结构与领域动作访问后端；会话动作按当前 capability 的状态、来源和原因开放，不按产品身份分支。
- `Session` / `ProtocolAdapter` 负责会话生命周期和协议适配；UI 不接触 RFB TCP、消息分帧或解码状态。
- Rust App Core 负责 Tauri seam、原生 RFB Core、Surface、本机共享服务检查、存储、凭据、本机 Server、Agent 和 artifacts 等系统能力。
- Command 只做边界转换和服务调用，领域规则留在对应模块；结构化错误向上收敛，UI 不依赖原始异常字符串。
- Host/Group 保存非敏感业务数据和最近连接时间；Credential Vault 管理系统安全存储，数据库只保存凭据引用。
- WebView 生产 CSP 仅允许本地脚本、必要 IPC 和回环 Surface WebSocket；样式单独保留 Vue/Naive UI 所需的内联许可。开发 HMR 策略与生产分离，配置、依据和原生验证边界见 [WebView 内容来源约束](docs/design/webview-csp.md)。
- 连接目标只描述用户请求的 `host:port`，不证明产品身份、能力或信任。与本机已运行共享地址和端口的精确匹配只提供本机生命周期、显示器和只读策略上下文，不推断远端身份或 Agent endpoint。
- 标准 VNC/RFB 是基础传输，承载画面、鼠标和键盘；独立的可选 Agent sidecar 是增强通道，可由 Discovery 或本机运行态关联，缺失或失败不能阻断基础连接，也不等同于已确认身份。身份与 Agent 会话边界见 [Discovery 协议](docs/design/server-discovery-protocol.md#agent-会话与信任边界)。
- Discovery 元数据单独记录为声明，不能使用 wire 自报验证状态提升身份。远端 `managed` 仅在本次 Agent 会话认证、描述和 ID 匹配后成立；重新发现会撤销旧证据。此会话结果不表示独立的设备证书或网络身份保证。

## 核心数据流

### VNC 连接

```text
Session UI
  -> connection confirmation
  -> ProtocolAdapter
  -> Tauri connect_remote_session_rfb
  -> Rust RFB TCP session + ClientFramebufferCore
  -> token-protected binary Surface stream
  -> Core Surface Canvas renderer
  -> SessionManager 记录状态、结构化错误、断开原因、事件和诊断
```

连接直接进入 Native RFB 握手，不提供独立的连接前探测流程。

连接失败和断开原因通过 `ConnectionIssue` / `DisconnectReasonCode` 归一化，UI 只依赖错误枚举和本地化文案，不直接依赖底层异常字符串。原生 RFB Core 的解析、认证、TCP 和 Surface 错误经 `ProtocolAdapter` 收敛到同一模型；失败或断开后的手动重试必须重新输入密码，避免复用上一次失败尝试中的临时明文凭据。

### 主机和凭据

```text
Host UI
  -> host resource actions
  -> Tauri host commands
  -> SQLite host/group rows
  -> Credential Vault 保存或读取密码引用
```

密码不得进入 SQLite、JSON 导出、日志或测试 fixture。前端只在用户主动连接时短暂持有明文密码。

### 本机共享和 Agent

```text
Local Share UI
  -> automatic runtime: installed Windows Service / user background
  -> shared Rust VNC Server engine
  -> local framebuffer/input/permission diagnostics
  -> optional lightweight Agent listener
  -> Session enhanced actions when endpoint/token/capability are available
```

本机共享复用同一个 Server/Agent 引擎，不提供运行方式选择或应用内临时 Server。Mac 使用独立用户后台进程；Windows 使用已安装服务，未安装时自动使用用户后台，保留剪贴板、文件、显示器等实际增强 capability，但不提供登录、锁屏、UAC 专用控制链路。已有后台实例运行时继续管理该实例，停止后采用已安装服务；服务停止、故障或状态查询失败不会另起后台替代。页面保留日常启停和共享配置，服务安装/卸载/更新、维护启停、开机自启、SAS 策略和诊断位于“设置 → 本机共享服务”，两个入口共用状态和动作。

用户后台使用主程序 `--background-share` 无窗口入口，Mac 使用 launchd 和 AppKit 主循环，Win 使用用户程序副本和登录快捷方式。配置只保存非敏感值，密码经短生命周期 IPC 交给后台，显式授权保存到系统凭据存储；登录自动启动需要已保存凭据。权限申请由后台自身执行；退出 App 只回收客户端会话，共享须显式停止。Windows 安装入口先选择范围：当前用户安装无需提权且不自动注册服务；整机安装请求管理员授权后由 NSIS 钩子部署服务。设置页仍可单独请求管理员授权安装、更新或卸载服务。主程序在创建 WebView/加载 .env 前处理服务命令，再复制到受保护目录。首次安装默认手动启动，不自动监听；整机更新保留服务配置、凭据和启动类型，原服务运行时恢复运行。整机安装包 /UPDATE 保留服务，完整卸载清理服务及其凭据；当前用户卸载不触碰服务，手动安装服务后需先从设置页卸载。长期配对与企业部署另行规划。

Windows 服务共享通过受限本机 IPC 管理独立 LocalSystem Service。Service 的持久 RFB broker 持有公共监听端口、客户端连接和编码状态，控制台 Session 的 SYSTEM worker 负责平台采集输入；两者通过使用独立随机凭据的回环 RFB 通道连接。切换账号只替换 worker 和内部通道，公共连接保留，等待新桌面首帧后恢复输入；旧桌面的按键不会重放。SAS 请求经 worker 身份校验后由 Service 执行。Service 同时监督当前登录用户的非提权 Agent，文件与剪贴板动作不进入 SYSTEM worker；公共 RFB broker 收到标准剪贴板文本时经受限 IPC 转交用户 Agent。自家 Client 在 Agent 提供写入能力时直接调用并等待写入确认，否则按标准 RFB 发出且只报告未确认。认证后的公共 RFB 连接经受控 IPC 获取短期 Agent 票据，显示选择和采集频率回传 worker，分辨率缩放由公共 broker 执行。增强进程或输入桌面状态变化触发重新发现，Agent 故障只降级增强能力。Tauri/WebView 保持普通用户身份，两类服务进程均独立于 App 生命周期。

### 系统托盘

```text
Local Share / Session / Settings 状态
  -> 前端 TrayStateSnapshot
  -> Tauri update_tray_state
  -> Rust TrayManager 更新图标、tooltip、菜单
  -> 托盘菜单事件回传前端执行现有动作
```

托盘是后台状态中心，主图标只突出本机共享运行状态、被连接数量和异常。远程 Session 只作为菜单/tooltip 摘要，不进入主图标优先级。托盘菜单可展示本机共享活动连接设备，并通过现有本机共享 action 断开指定设备；通知差分由前端快照比较触发，覆盖新设备连入、异常停止、权限缺失和端口冲突。托盘层不理解 RFB Core、Host 存储或 Session 连接实现；启动本机共享可以走现有运行态参数直接尝试，参数不完整或失败时回到本机共享页面处理。

状态快照同时携带当前启停或等待用户处理的阶段；启停期间禁用重复操作，等待权限确认或输入密码时保留引导入口，不作为启动失败通知。快照内容没有变化时跳过同步，重叠同步按顺序读取最新状态，避免轮询反复替换展开中的原生菜单。共享运行时退出项显示“退出应用（共享继续）”，明确应用与后台共享的生命周期边界。

### 诊断事件

```text
Host / Session / System action
  -> structured diagnostic event
  -> global diagnostics state
  -> lightweight references in Host and Session
  -> Session diagnostics / copyable reports / runtime file log
```

诊断事件应按来源和上下文显式归属，不通过本地化文本推断所有权。
