# Windows 存储目录

Windows 将个人应用文件存放在 `%LOCALAPPDATA%\LLRemote\`。显示名称为 LL Remote。后续升级沿用应用标识、WebView 页面来源、偏好键和系统凭据命名空间。

| 子目录 | 用途 | 卸载时勾选个人数据清理 |
| --- | --- | --- |
| `App` | 当前用户安装的程序、资源和卸载器 | 随程序卸载，保留未知文件 |
| `Data` | 主机、传输任务、目录授权、录像恢复索引和应用版本记录 | 清理 |
| `WebView` | WebView2 用户数据，包括界面偏好和缓存 | 清理 |
| `Logs` | 主应用日志，可通过 `VNC_LOG_DIR` 覆盖 | 清理默认目录 |
| `Sharing` | 用户共享配置、启动记录和后台程序副本 | 清理 |
| `Artifacts` | 默认截图、录像及录制中间文件 | 保留 |
| `ReceivedFiles` | 默认接收文件及接收中间文件 | 保留 |

目录按需创建。为所有人安装时，主程序位于 `%ProgramFiles%\LLRemote\App`，系统服务程序位于同一根下的 `Server`。服务受保护的配置位于 `%ProgramData%\LLRemote\Server`。每个用户使用自己的 Local 数据。安装器也支持自定义程序目录，用户可另选文件保存位置。

## 启动入口

主程序在构建 Tauri 应用前通过 Rust 的 `windows_storage::configure_directories` 配置数据、WebView 和日志目录。主窗口和动态会话窗口共用 WebView 目录。项目锁定的 Tauri CLI 不接受 JSON 中的 `appDirectoriesOverride`，因此在 Rust 运行时设置对应字段。

应用在打开业务数据和创建窗口前检查 `Data\.app-version`。小于已记录版本的构建会停止启动。用户共享后台和 Agent 通过 Windows Known Folder API 使用同一 Local 根目录。内置文件目录由应用启动时注入文件访问模块。

## 验证

Windows 原生存储检查使用 `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/windows/test-storage-layout.ps1`。它检查全目标编译，在隔离目录中验证数据清理、输出保留及目录授权。测试进程需要 Windows SDK 的 `mt.exe` 嵌入 Common Controls v6 manifest。
