# Server 模块

`src/server/` 封装本机共享状态、配置和 Tauri 调用；页面编排位于 `src/features/local-share/`。实际 VNC Server、用户后台和 Windows Service 由 Rust 管理。

整体关系见 [Architecture.md](../../Architecture.md)，扩展协商及身份边界见 [Discovery 协议](../../docs/design/server-discovery-protocol.md)。

## 职责与入口

| 范围 | 入口 |
| --- | --- |
| 前端状态与调用 | [localShare.ts](localShare.ts)、[backgroundShare.ts](backgroundShare.ts)、[windowsService.ts](windowsService.ts) |
| 页面动作与实例路由 | [useLocalShareWorkbench.ts](../features/local-share/composables/useLocalShareWorkbench.ts) |
| 启动、权限引导与取消 | [useLocalShareStartFlow.ts](../features/local-share/composables/useLocalShareStartFlow.ts) |
| Windows 适用网络类型、防火墙状态与设置入口 | [windowsFirewall.ts](windowsFirewall.ts)、[权限卡片](../features/local-share/components/LocalShareWindowsFirewall.vue)、[Rust 防火墙模块](../../src-tauri/src/windows_firewall/) |
| 维护与设置 | [WindowsServiceSettings.vue](../features/settings/components/WindowsServiceSettings.vue)、[MacSharingSettings.vue](../features/settings/components/MacSharingSettings.vue) |
| Rust 运行实例 | [用户后台](../../src-tauri/src/background_share/)、[Windows Service](../../src-tauri/src/windows_service/)、[Server 生命周期](../../src-tauri/src/local_vnc_server/) |
| 协议、平台与增强 | [RFB Server](../../src-tauri/src/local_vnc_server_protocol/)、[平台采集与输入](../../src-tauri/src/local_vnc_server_platform/)、[Agent](../../src-tauri/src/local_agent_service/) |

## 运行与维护规则

- macOS 开发版与安装版均使用独立的 `LLRemote Service.app` 和 `.sharing` 应用标识。安装包内置于主应用的 `Contents/Helpers/`，随主应用一起签名；后台不占用 Dock 的主界面入口。Windows 使用已安装服务，未安装时使用用户后台。
- Windows 用户后台允许主应用以管理员身份发起共享。同用户、同交互会话的普通桌面可提供后台身份；桌面本身提权时，显式选择普通或受限用户令牌，并重建该用户的环境。进程先挂起创建，校验实际 SID、Session、权限与完整性后才运行；登录自启遇到高权限也先交接到普通后台。后台仍脱离主应用的 Job 生命周期，拒绝跨用户、Session 0 和 SYSTEM；不通过安装服务或修改 UAC 来绕过启动。
- 关闭 App 不停止独立共享。macOS 显式停止会等待共享后台进程退出并卸载当前 launchd 任务，保留密码和下次登录的自启设置；重复停止成功。重开 App、进入页面或检测权限不自动恢复监听。
- “开始共享”完成必要的配置或密码保存后再启动；失败或取消时不启动。自启单独在设置中管理，开启前校验配置和可读凭据。
- 用户后台自启状态同时展示保存的策略和系统注册状态；macOS 开启前由共享后台复查录屏及所需的辅助功能授权。系统拒绝或启动项缺失不会被配置布尔值伪装成已启用。登录后的启动尝试记录在独立的 `startup-attempt.json`，失败后即使后台退出仍可在设置中查看。Windows 服务也保存最近一次自动服务启动的监听结果；此记录只说明那次启动，不代表当前仍在共享。
- 残留启动项提醒仅在确认问题解除后重新启用；运行环境检测失败或系统注册状态未知时保留去重记录，避免轮询恢复时反复弹出同一提醒。
- 密码只保存在系统凭据存储；表单不读取或回填已保存的明文。macOS 钥匙串不可用时可设置本次临时密码，该分支不保存密码、监听配置或自启策略。
- 当前后台 IPC 为 6。新写入要求当前协议；仅空闲旧后台可以更新，活动旧实例保留并提示版本错误。状态读取和停止仍需兼容独立存活的旧后台。
- macOS 权限绑定实际共享后台。周期刷新读取权限快照，页面检测、启动和授权返回才主动检测；旧快照不能放行启动。“授权”统一校验后台签名、重置当前缺失的单项权限并申请系统授权；打包版录屏同时清理主应用和共享后台可能持有的旧记录，辅助功能只清理共享后台，正常授权保留。打开设置和重新检测不重置权限，运行中的共享不被自动重启。
- macOS 设置中的“故障排查”提供重启和更新修复，相关错误和版本差异显示在对应操作旁。后台签名变化且授权缺失时，共享页和共享设置共用一条短时提醒。共享页的修复跳转会展开并定位维护操作。共享状态、权限检测和连接设备在“本机共享”页查看。
- Windows Service 的持久 RFB broker 承载公共连接，SYSTEM worker 经独立认证的回环 RFB 通道提供桌面采集和服务专用输入。切换账号只替换桌面通道，公共连接保留并等待首帧恢复；文件和剪贴板由登录用户的普通权限 Agent 执行。broker 收到标准 RFB 剪贴板文本时经受限 IPC 转交 Agent；Agent 不可用时基础画面与键鼠仍可连接，但剪贴板写入不能保证。Agent 故障只降级增强能力，不能阻断基础 VNC。
- 服务安装、更新、卸载、网络配置和 SAS 策略保留管理员门禁；日常启停与独立密码修改使用各自授权，不扩大普通账户权限。
- Windows 主程序清单显式使用 `asInvoker`，普通启动保持当前权限。点击安装服务时使用独立 STA 和同步 Shell `runas` 按需请求 UAC，已提权进程直接执行。取消授权、策略拒绝、授权启动失败和文件复制、服务注册、账户授权失败分别反馈。GUI 单实例按登录会话和账户 SID 隔离。同一账户、同一会话的普通和管理员启动共用已有 GUI，后启动进程只请求恢复窗口并退出，原进程权限和连接保持不变。同步对象仅授予该用户所需权限并设置中完整性标签。
- Windows 设置提供一个“检测并修复”入口。先检查服务版本、当前操作账户的共享管理权限、SAS 策略及用户 Agent，需要管理员权限的项目共用一次授权。取消、拒绝或无法提权时保留逐项检测结果，并标记待管理员处理。使用其他管理员凭据时仍授权原操作账户。旧运行中服务更新后恢复运行，已停止的共享保持停止。显式修复可调整服务的 SAS 策略并保留 UIAccess 范围。
- 需要提权的服务操作会先检查 UAC，管道等待也会检查辅助进程是否提前退出。只有已具备管理员权限的主程序在用户点击“检测并修复”时会恢复关闭的 UAC，并在诊断中提示重启。普通账户可读取配置并完成可执行的检查。无关联普通令牌的管理员账户使用受限、中完整性且无启用管理员组的用户令牌。
- 修复入口不依赖服务已安装。未安装服务时调用普通后台的真实启动和 IPC 就绪检查，只准备空闲进程；空闲旧进程通过原子 Recover 更新，运行中的监听不重启。结果保留身份检查阶段和 Win32 错误码，文件与剪贴板能力在共享运行时核对，未运行时标记待检查。受限令牌需先复制为带 TOKEN_ADJUST_DEFAULT 的主令牌，再降低完整性级别；不能在只有 QUERY / DUPLICATE 的句柄上设置完整性。
- 降权不只校验令牌身份：新受限令牌的默认 DACL 必须显式允许该用户与 SYSTEM；普通后台和用户 Agent 创建时也显式设置进程、线程对象权限，避免沿用仅允许管理员的创建者 DACL。否则后台可以监听，但收到连接后 `TcpStream::try_clone` 的 Windows 套接字复制会失败并关闭连接。权限只作用于新建令牌/对象，不修改原登录令牌、UAC 或其他进程。Windows 后台 IPC 版本 8 用于在手动启动时更新空闲旧后台；运行中的旧共享先停止，再启动新版。

## 能力与验证边界

- 本机共享只展示实际实例、设备和 capability，不以历史结果、配置存在或 Agent 可达性推断当前可用。
- 采集失败不生成合成画面冒充成功；桌面受限时保留最后画面并暂停输入，恢复需等待新画面。未支持扩展的第三方 Viewer 不作同等状态提示承诺。
- `Ctrl+Alt+Delete` 的界面反馈覆盖失败、拒绝和不支持。服务模式按策略执行 SAS，普通后台可接收标准 RFB 按键序列。
- 复制连接信息不包含密码。系统凭据、权限申请、安装更新、锁屏恢复、多显示器和持续运行须按对应平台实测。

## 验证入口

从仓库根目录按改动选择前端回归：

```bash
node --test tests/frontend/server/backgroundShareRouting.test.ts tests/frontend/server/localShareStartFlow.test.ts
```

后端测试位于 `src-tauri/tests/`，Cargo 命令从 `src-tauri/` 执行。

| 改动范围 | 相关测试 |
| --- | --- |
| 配置、凭据与独立后台 | [后台配置](../../src-tauri/tests/support/background_share_config_tests.rs)、[后台生命周期](../../src-tauri/tests/support/background_share_lifecycle_tests.rs) |
| 权限与运行状态 | [权限状态](../../src-tauri/tests/support/background_share_status_tests.rs)、[签名与权限修复](../../src-tauri/tests/support/background_share_permission_identity_tests.rs) |
| Windows 服务授权与自启 | [密码管理](../../src-tauri/tests/support/windows_service_password_management_tests.rs)、[服务自启](../../src-tauri/tests/support/windows_service_auto_start_tests.rs) |
| Windows 桌面通道替换与连接保持 | [真实 RFB 生命周期、输入隔离与 SAS](../../src-tauri/tests/support/desktop_relay_tests.rs) |
| Server、采集与 Agent | [Server 交付](../../src-tauri/tests/server_delivery_tests.rs)、[采集](../../src-tauri/tests/support/server_capture_tests.rs)、[Agent](../../src-tauri/tests/local_agent_service_tests.rs) |

平台自动检查和合成协议测试不能替代真实 VNC / Tauri shell 验证。
