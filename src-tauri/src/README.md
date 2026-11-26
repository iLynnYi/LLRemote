# Rust 后端模块

`src-tauri/src/` 是 Tauri 应用的 Rust 后端核心，负责原生 RFB/Framebuffer/Surface、本地存储、凭据、本机共享、Agent、文件传输辅助、Session artifact 写入和本机文件访问授权。

下文模块简写相对于 `src-tauri/src/`；完整源码和测试路径相对于仓库根目录。Cargo 命令从 `src-tauri/` 执行。

Cargo 目标名称统一为：主程序 `LLRemote`、库 `LLRemoteLib`、服务 `LLRemoteService`。跨 crate 引用使用 `LLRemoteLib::...`，服务入口为 `src-tauri/src/bin/LLRemoteService.rs`，单独构建服务使用 `cargo build --bin LLRemoteService`。

## 主要职责

- `lib.rs`：Tauri command、plugin 和应用状态注册。
- `main.rs`：Tauri 可执行入口。
- `remote_session`：后端会话和 Surface 的注册、连接及释放，承载 VNC base transport、Agent enhanced channel、display/input source、Surface 归属模型和 snapshot command/event 边界；`lib.rs` 同时发出全局 `remote-session-status` / `remote-session-surface` 事件，供前端刷新 Core snapshot。
- `remote_desktop_core`：Client/Server framebuffer、RFB 握手与消息循环、编码解码、Dirty Rectangle 和 Surface packet。
- `host_store`、`host_store_storage`：Host、Group、凭据引用、最近使用时间和 SQLite 存储；使用 schema 1。Repository 方法、类型、读写辅助、建库和时间格式化分文件维护。
- `credential_vault.rs`：系统凭据存储封装。
- `file_access`：本机文件访问授权目录持久化、实时可用性检查、授权检查和安全打开/定位文件边界；内置留痕目录固定置顶且不可撤销授权，支持修改位置并保留旧位置的普通授权。仅内置目录自动创建/重建，其他授权目录缺失或不可写时保留记录并返回 `missing` / `not_writable` 状态。
- `files`：本机文件/目录混选和上传源展开，把目录递归转换为文件任务并生成相对路径展示信息。
- `local_vnc_server`、`local_vnc_server_platform`、`local_vnc_server_protocol`：嵌入式 VNC Server 的生命周期、认证、RFB 协议、帧缓冲、权限、平台能力和类型；连接注册表、帧缓存、平台输入/捕获、协议实现和协议单元测试保留在目录化模块内。
- `local_agent_service`：轻量 Agent 控制通道和增强能力，包括剪贴板、文件上传、上传进度、暂停/取消、运行期续传、权限、显示器和安全输入；客户端通过 `commands/agent.rs` 的通用请求和带进度上传调用，协议响应与上传类型在 `types.rs`。
- `session_artifacts`：截图/录制文件写入。
- `server`：嵌入式 Server 和托盘复用的本机地址候选及地址类型；共享状态由实际 Server /后台实例提供。

## 边界规则

- Rust 后端返回结构化结果和可解释错误，前端负责展示。
- 密码、token、凭据解析结果和完整连接串不得写入日志。
- SQLite 只保存业务数据和凭据引用，不保存明文密码。
- 平台采集、输入和服务能力由对应平台模块负责；单平台编译与自动检查不代表其他平台或真实桌面行为已经验证。
- 本地 socket 相关测试在受限沙箱里可能因权限失败，需要按环境说明区分。

## 相关测试

Rust 测试位于 `src-tauri/tests/` 和模块内单元测试。运行前根据改动范围选择 `cargo test <name>`、`cargo test` 或 `cargo check`。
