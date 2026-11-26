# WebView 内容来源约束

生产策略定义于 `src-tauri/tauri.conf.json` 的 `app.security.csp`，默认拒绝未列明的来源，不允许远程脚本、执行内联脚本、`eval`、嵌入 frame、object、表单提交和修改 base URL。没有关闭 Tauri 的资源 CSP 修改；打包时由 Tauri 对自身注入脚本补充 nonce/hash。

## 必要来源

| 来源 | 用途与边界 |
| --- | --- |
| 脚本 `'self'` | 本地构建的 Vue 应用和按需文档模块；不使用 CDN 或远程代码。 |
| 连接 `'self'`、`ipc:`、`http://ipc.localhost` | Tauri 原生 IPC 与本地资源；普通浏览器不能证明原生 IPC 可用。 |
| `ws://127.0.0.1:*` | Rust Surface listener 在 IPv4 回环地址上分配动态端口；由后端会话、路径及 token 校验控制访问。不是任意远端 WebSocket 来源。 |
| 样式 `'self' 'unsafe-inline'` | Naive UI 的 CSS-in-JS、Vue 动态 style 和本地样式。只放开样式，不放开内联脚本或 eval。 |
| 图片 `'self' data:` | 应用图标和 Canvas 生成的 PNG 光标；不允许外部图片。 |
| 字体 `'self'` | 本地资源；系统字体不需要额外网络来源。 |

Vue I18n 11 当前使用消息 AST/JIT 路径，不通过放开 `unsafe-eval` 解决国际化。若后续新增 worker、媒体、外部资源或自定义协议，应先确认真实数据流并进行相应回归，不能把策略直接改回 null。

开发策略 `devCsp` 单独允许 `ws:`，以支持 Vite HMR 与显式设置的 `TAURI_DEV_HOST`（可能为开发者指定的局域网地址）。此许可不进入生产策略；开发环境本身应仅暴露在受控开发网络。

## 验证边界

- `tests/frontend/app/webviewCsp.test.ts` 锁定必要来源与禁止脚本逃逸的配置契约；它不冒充浏览器执行测试。
- 构建验证应确认 Tauri 能解析策略并生成生产应用。浏览器使用同一生产策略时，应验证应用样式、中文文案、按需文档、本地 WebSocket，以及外部脚本/网络与 eval 的实际拒绝。
- 在 macOS WKWebView 与 Windows WebView2 的 Release 应用中分别验证主窗口、独立会话窗口、IPC、Surface 画面和数据光标、权限对话框、剪贴板、文件及“关于”页。分别记录浏览器检查和原生桌面验收结果。

CSP 用于限制前端内容来源；后端参数验证、会话授权和 capability 仍须独立成立。相关边界见 [Tauri CSP 文档](https://v2.tauri.app/security/csp/) 与 [Vue I18n 优化说明](https://vue-i18n.intlify.dev/guide/advanced/optimization)。
