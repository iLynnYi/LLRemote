# Hosts 模块

`src/hosts/` 承载 Host 资源的前端业务模型。它关注主机、分组、搜索、凭据引用和连接入口，不直接保存明文密码。

## 主要职责

- `hostManager.ts`：Host、Group、连接信息、凭据状态和资源管理规则。

Host 页面 UI 位于 `src/features/hosts/`，后端持久化位于 `src-tauri/src/host_store/` 和 `src-tauri/src/host_store_storage/`。当前没有独立历史记录页面。

## 边界规则

- Host 数据库只能保存凭据引用，不能保存明文密码。
- 保存、替换、删除凭据必须走系统 Credential Vault 边界。
- 连接时已保存密码不可读取（包括钥匙串授权被拒绝），只记录诊断并转入手动密码弹窗，不显示主机保存场景的错误 Message。输入仅用于本次连接，不覆盖已保存凭据；主动取消、关闭或 Esc 时只警告一次“用户取消了连接”，不发起连接或更新最近连接时间。弹窗被新请求替换或请求被中止时保持安静。
- 临时连接是否保存到 Host 列表必须是显式用户意图。
- 主机页只展示资源信息和“最近连接”时间，不把历史连接结果当作当前可用性，也不提供基于历史结果的状态筛选。
- “最近连接”记录每次实际发起连接的时间，成功和失败不作区分；取消密码输入、选中或编辑主机不更新，后续断开也不覆盖。
- 主机页只提供连接入口，不提供独立检测；连接失败原因由本次操作反馈、Session 故障详情和诊断日志承载。首次启动使用 [schema 1](../../src-tauri/src/host_store_storage/schema.rs) 初始化主机数据库。

## 相关测试

优先查看 `tests/frontend/hosts/`。涉及 SQLite 或凭据边界时，再查看 `src-tauri/tests/host_store_tests.rs` 和 Rust 单元测试。
