# Nexious Quick

Nexious Quick 是一款 Windows 桌面快速启动工具。按下全局快捷键，输入关键词即可快速启动电脑应用、网站链接、文件与文件夹，并支持对已安装应用、浏览器收藏夹和本地目录进行自动同步与检索。

## 应用截图

### 快速启动

![Nexious Quick 快速启动界面](docs/images/launcher1.png)

![Nexious Quick 快速启动界面](docs/images/launcher2.png)

### 启动数据
![Nexious Quick 启动数据界面](docs/images/data1.png)
![Nexious Quick 快速启动界面](docs/images/data2.png)
![Nexious Quick 快速启动界面](docs/images/data3.png)


### 外观设置

![Nexious Quick 外观设置界面](docs/images/appearance-settings.png)


### 应用设置
![Nexious Quick 应用设置界面](docs/images/settings.png)

## 功能特性

- **全局唤起**：自定义快捷键随时显示 / 隐藏搜索窗口，启动器可置顶显示
- **一键启动**：搜索应用、网站、文件与文件夹；无结果时可快速跳转搜索引擎
- **应用同步**：自动扫描开始菜单、桌面快捷方式、Microsoft Store 应用与系统注册的应用，同步时保留启动参数、补全图标，并支持“运行 / 控制面板 / 资源管理器 / steam:// 协议”等特殊启动方式
- **收藏夹同步**：支持 Chrome、Edge、Brave 浏览器收藏夹同步，自动补全网站图标
- **本地目录同步**：接入 Voidtools Everything 本地索引，自动同步所选目录的文件 / 文件夹，不再递归扫盘，也没有每目录 2 万项限制
- **数据管理**：集中管理启动数据，支持别名、描述、图标与重复项检测（添加重复项会给出提示）
- **个性化外观**：七套全局 UI 风格统一控制背景、侧栏、卡片、表格、弹窗、文字、圆角与间距，支持深浅模式；已移除动画库和界面动画
- **窗口与托盘**：支持启动器置顶、失去焦点自动隐藏、鼠标悬停唤起、关闭窗口驻留系统托盘
- **常用设置**：开机自启动、搜索框行为、默认搜索引擎与全局快捷键配置
- **检查更新**：在应用设置中一键检查 GitHub Releases 发布的新版本并跳转下载

## 自动同步说明

- 应用、Chrome / Edge / Brave 收藏夹与已选本地目录在启动后自动同步，每轮完成后间隔 15 秒再次检查；隐藏窗口或驻留托盘时仍然运行。应用同步可在「电脑应用」中关闭，网站和文件的显示开关只控制搜索结果。
- 同步同时处理新增、修改和删除：卸载应用、删除收藏、重命名或删除文件后，对应的自动导入条目会更新或移除，已打开的搜索结果也会刷新。多个浏览器或重叠目录仍包含同一条目时会保留。
- 手动添加的条目保留；旧版本能识别的自动导入条目会迁移为同步数据。扫描失败会保留原索引并在后续自动重试。Everything 尚未启动、索引未就绪或所选目录不在索引范围内时，保留已有文件数据并稍后重试。同步页面不展示内部扫描日志。
- 同步是从系统、浏览器和目录更新启动索引，不会删除原始软件、收藏或文件。手动同步会显示实际新增、更新、移除数量。

## Everything 文件索引

Windows 上请安装并运行 [Voidtools Everything](https://www.voidtools.com/)，等待索引就绪，并确保所选目录已包含在 Everything 索引中。程序通过官方 SDK 本地 IPC 查询文件名和路径，不需要开启 HTTP 服务或安装 Node.js，也不会安装 npm 中同名但用途不同的 `everything` 包。SDK 源码与许可证位于 `src-tauri/vendor/everything`。

索引范围遵循 Everything 的设置；隐藏文件、系统文件与排除规则由 Everything 管理。每轮同步读取完整结果，仅对所选目录的索引做增删对账。IPC 在独立后台进程中执行，超时或失败不提交空结果。

## 升级与数据保留

应用内「检查更新」只查询 GitHub Releases 的版本，不修改启动项、设置或历史记录。下载新版安装包后直接覆盖安装，无需先卸载。请使用与原安装相同的安装方式（EXE 或 MSI）。

数据始终存放于 `%APPDATA%\com.nexious.quick\nexious.db`，不随版本号变化，也不打包进安装程序。从 v0.1.8 或更早版本升级时，会在迁移前自动生成 `backups\nexious-before-0.1.9.db`，保留迁移前的完整数据库。从 v0.1.9 升级到 v0.1.10 沿用原数据库，不重置数据。迁移保留已有条目、别名、自定义描述/图标、启用状态、设置和搜索历史；后续自动同步仍会正常处理源数据的新增、修改和删除。

## 技术栈

- Tauri 2 + Rust（SQLite 本地存储）
- Vue 3 + TypeScript + Vite
- Naive UI + Ionicons
- pnpm 包管理

## 运行

```bash
pnpm install
pnpm tauri dev
```

构建安装包：

```bash
pnpm tauri build
```
