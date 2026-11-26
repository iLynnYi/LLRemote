# Session 模块

`src/session/` 负责远程会话、连接模型、协议适配和会话动作。页面流程放在 `src/features/sessions/composables/`，UI 只接收状态和动作，不直接操作协议或系统能力。

整体关系见 [Architecture.md](../../Architecture.md)，身份与认证边界见 [Discovery 协议](../../docs/design/server-discovery-protocol.md)。

## 职责与入口

| 范围 | 入口 |
| --- | --- |
| 会话状态、激活与关闭 | [sessionManager.ts](core/sessionManager.ts)、[连接注册](core/sessionConnectionRegistry.ts) |
| 连接身份与能力 | [connectionModel.ts](connection-model/connectionModel.ts)、[Discovery 类型](discovery/serverDiscovery.ts) |
| 连接、取消与资源释放 | [连接流程](../features/sessions/composables/useSessionConnectionLifecycle.ts)、[启动尝试](../features/sessions/composables/sessionConnectionLifecycle/connectAttempts.ts) |
| 原生 RFB 与 Agent 适配 | [protocolAdapter.ts](protocol/protocolAdapter.ts)、[Agent runtime](protocol/protocolAdapter/nativeAgentRuntime.ts) |
| 画面呈现 | [coreSurfaceSession.ts](renderer/coreSurfaceSession.ts)、[coreSurfaceCanvas.ts](renderer/coreSurfaceCanvas.ts)；视口模型在 `viewport/` |
| 画面缩放与本地平移 | [边缘滚动](viewport/viewportScroll.ts)、[视口事件绑定](../features/sessions/composables/useSessionViewportNavigation.ts)；主窗口与独立窗口共用，浮层定位在滚动区域外 |
| 独立窗口 | `external-window/`；[窗口协调器](../features/sessions/composables/useSessionExternalWindowCoordinator.ts) |
| 输入与剪贴板 | `input/`、`clipboard/`；动作按会话 capability 执行 |
| 远端消息弹窗 | `messages/`；通过已认证的 `message.show` 能力发送纯文本，独立窗口等待对应回执 |
| 文件上传与任务 | [上传编排](transfers/sessionFileUploads.ts)、[任务与偏好存储](transfers/fileTransferStore.ts) |
| 截图、无声录制与诊断 | `artifacts/`、`diagnostics/`；连接设置在 `settings/` |

模块内部使用明确的领域文件导入，不建立覆盖整个 Session 的大 `index.ts`。

## 修改时保留的约束

- 标准 VNC/RFB 承载基础画面和输入；Agent 失败只降级增强能力。显示器、剪贴板、文件等动作按实际 capability 开放。
- “发送消息”由被控端原生窗口显示并提供复制/关闭，与剪贴板同步独立；不支持 `message.show` 时禁用。消息最多 16 KiB UTF-8，不持久化、不写日志；接收端后台入队后确认，界面分批增量追加，不等待排版完成。发送成功关闭输入框并清空草稿，不显示成功提示；失败保留草稿并提示。仅控制会话且用户桌面可用时接收；未关闭的消息窗口复用同一窗口，按被控端接收时间追加。收到新消息会恢复窗口、置前并请求焦点；Windows 若限制后台激活则闪烁任务栏。灰色小字时间与正文从同一行开始，正文较长时自动换行并逐步放大窗口，含标题栏的宽高各不超过主窗口的一半（主界面未打开时按默认 1280×768 计算），同时限制在屏幕工作区内；达到上限后显示纵向滚动条，保留正文原有换行；复制时包含全部条目，关闭清空已显示记录，已接收但尚未显示的消息保留到下一次显示。已显示与待显示内容合计最多 128 KiB，达到上限会拒绝新消息并保留原有内容。
- 主工作区、独立窗口和录制复用同一条 RFB 连接；窗口只持有自己的呈现和输入资源。发送消息框支持 Ctrl+Alt+M（macOS：Control+Option+M），同一套定义用于工具箱提示及设置中的快捷键列表。发送框中的 Esc 与关闭按钮效果一致（发送期间禁用，输入法组词期间不触发）；接收端复制使用系统剪贴板，Windows 由常驻接收窗口持有所有权，关闭消息框不影响已复制内容，也不覆盖之后复制的新内容。
- 会话轨道决定主窗口呈现；键鼠输入和远端剪贴板接收跟随获得焦点的窗口，独立窗口始终指向自己的会话。切换焦点时释放旧会话已按下的输入。
- 固定显示比例溢出时，滚动条和鼠标边缘悬停用于本地平移；角落支持双轴移动，悬停越靠边越快。工具箱与 HUD 以可视窗口为定位边界，不随画面平移或缩放；进入浮层、离开视口、失焦或断开时停止边缘滚动。滚动条轨道不转发为远端鼠标输入，平移时同步远端指针坐标和当前按键状态。
- 取消、断开、关闭和重连按所属尝试回收资源，迟到回调不能复活旧连接或修改替代会话。失败与断开的首个终态只报告一次。
- Surface 失效后重新建立完整画面基线，不能继续确认未成功呈现的增量。远端桌面受限时暂停输入，新画面提交后才恢复；第三方 Server 未声明状态时不推测锁屏。
- 密码、票据和剪贴板正文仅在业务需要的短生命周期内持有，不进入持久化、诊断或日志。错误展示使用结构化类别和本地化文案。
- 设置和传输存储损坏时明确报错，不静默改为空缓存。截图与录制的写入、打开和定位均由 Rust 检查目录授权。
- 桌面传输任务保存在应用数据目录的 `file_transfer_tasks.sqlite3`。新增任务先提交数据库再启动上传，状态变更合并写入；目录扫描每次最多返回 200 个文件，传输列表每页显示 100 项。历史记录延续现有会话关闭与手动清理规则，不按数量自动删除。

## 功能边界

- 文件传输当前提供上传，不提供下载。主窗口、独立窗口、选择和拖放复用上传编排；取消选择不创建任务，远端路径动作只使用已完成任务的真实路径。
- 无声录制按会话持有任务，目标帧率为 20 fps，输出长边最多 1920 像素且不放大小画面。画布尺寸固定为首帧尺寸，远端尺寸变化时缩放补边。录制失败不关闭 RFB；保存失败可重试。编码依赖 WebView，后台调度和长时间录制需单独验收。
- “远程结束后锁定本机”归被控端所有，位于“设置 → 本机共享”，默认关闭并保存到当前被控用户的后台共享目录。Client 不保存或决定远端锁屏策略；主动断开或关闭已连接会话时向支持的认证 Agent 发送 `remote.session.end`，被控端按自己的设置请求锁屏，只读连接同样遵循被控端策略。“本机共享 → 断开连接”及托盘断开也会在后台主机或 Windows 用户 Agent 中执行同一策略。网络中断、连接失败、重连清理和应用退出不触发。未提供此通知能力的旧版或第三方 Client 无法区分主动退出和网络中断，仅被控端主动断开可触发。确认表示请求已提交，不是锁屏状态证明；macOS 需辅助功能权限，Windows 锁屏在用户 Agent 中执行，实际锁屏与恢复需实机验收。
- 剪贴板同步只写远端剪贴板，不自动粘贴；发送消息使用独立消息窗口。快捷键中的系统分组不推断或保存远端操作系统。
- `Ctrl+Alt+Delete` 按 Server 能力使用专用请求或标准 RFB 按键序列。界面提示明确的失败、拒绝或不支持，任务管理器动作独立。
- 本地快捷键：`Ctrl+Alt+Enter` 全屏、`Ctrl+Alt+W` 独立窗口切换、`Ctrl+Alt+U` 上传、`Ctrl+Alt+T` 传输任务；macOS 的 Alt 显示为 Option。

## 验证入口

从仓库根目录按改动选择测试；示例：

```bash
node --test tests/frontend/session/protocolAdapter.test.ts tests/frontend/session/sessionConnectAttempts.test.ts
```

| 改动范围 | 相关测试 |
| --- | --- |
| 会话与连接 | [sessionManager](../../tests/frontend/session/sessionManager.test.ts)、[连接输入](../../tests/frontend/session/sessionConnectionInput.test.ts)、[Agent runtime](../../tests/frontend/session/nativeAgentRuntime.test.ts) |
| 画面与独立窗口 | [Surface](../../tests/frontend/session/coreSurfaceSession.test.ts)、[窗口协调](../../tests/frontend/session/sessionExternalWindowCoordinator.test.ts)、[远端桌面状态](../../tests/frontend/session/serverRuntime.test.ts) |
| 视口导航 | [边缘滚动](../../tests/frontend/session/viewportScroll.test.ts)、[事件与生命周期](../../tests/frontend/session/sessionViewportNavigation.test.ts) |
| 上传、存储与录制 | [上传](../../tests/frontend/session/sessionFileUploads.test.ts)、[传输存储](../../tests/frontend/session/fileTransferStore.test.ts)、[录制](../../tests/frontend/session/sessionRecordingTask.test.ts) |
| 原生协议与生命周期 | [Rust Core](../../src-tauri/tests/remote_desktop_core_tests.rs)、[Rust Session](../../src-tauri/tests/remote_session_tests.rs) |

真实 VNC 兼容性、Tauri 窗口与焦点、系统权限、后台录制和长时间运行需对应环境手测；上述入口不是实机通过记录。
