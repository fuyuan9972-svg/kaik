# AI 论文降重工具 — 设计文档

> **目标：** 本地桌面 App，上传论文文件，调 LLM API 轻度改写，降低国内 AIGC 检测率（维普/知网/PaperPass）。

## 技术栈

| 层 | 技术 | 说明 |
|---|---|---|
| 框架 | Tauri 2.0 | 桌面壳，单 .dmg 安装 |
| 前端 | Vue 3 + Vite | UI |
| 对比 | diff-match-patch (js) | 段落级 diff 高亮 |
| 文件解析 | zip + quick-xml (docx), pdf-extract | Rust 解析 |
| HTTP | reqwest | 调 LLM API |
| 存储 | tauri-plugin-store | 本地存配置 |
| 导出 | docx-rs | 生成 .docx |

**目标平台：** macOS (Apple Silicon) 优先

---

## 整体架构

```
┌─────────────────────────────────────────────┐
│              Tauri 桌面 App                   │
│  ┌──────────────┐   ┌─────────────────────┐  │
│  │   Vue3 前端   │←→│     Rust 后端        │  │
│  │              │   │                     │  │
│  │ • 文件上传    │   │ • docx/pdf/txt 解析  │  │
│  │ • 对比视图    │   │ • 逐段调 API 改写    │  │
│  │ • 导出 .docx  │   │ • 改写编排与重组     │  │
│  │ • 设置页      │   │                     │  │
│  └──────────────┘   └─────────────────────┘  │
└─────────────────────────────────────────────┘
```

---

## 页面设计

### 1. 主页（处理页）

- 顶部：文件上传区（拖拽 / 点击选择 .docx / .pdf / .txt）
- 中间：处理状态（当前段落 / 总段落数、进度条、预估剩余时间）
- 底部：「开始改写」按钮
- 处理完成后自动跳转对比页

### 2. 对比页

- 左右分栏：原文 | 改写后
- 段落级高亮差异（绿色=新增/改写，红色=删除）
- 可逐段「接受」/「拒绝」改写（拒绝则保留原文）
- 右上角：「导出 .docx」按钮
- 改写结果本地缓存，关闭 app 不丢失

### 3. 设置页

- API 地址（中转站 URL，如 `https://api.example.com/v1`）
- API Key
- 模型名（如 `gpt-5.5`、`mimo` 等）
- 「测试连通」按钮：发一个简单请求验证 API 能通
- 改写语言选择（中文 / 英文）
- 配置本地持久化存储

---

## 文件解析

### .docx 解析流程

1. 读取 .docx 文件（本质是 ZIP 包）
2. 解压 `word/document.xml`
3. 解析 XML 中的 `<w:p>` 段落节点
4. 提取每个段落的纯文本内容
5. 记录段落的样式信息（粗体、斜体、字号等）用于重组

**关键依赖：** zip crate + quick-xml crate

### .pdf 解析流程

1. 使用 pdf-extract crate 提取文本
2. 按页面拆分，合并为段落
3. 中文 PDF 如果提取质量差，提示用户转换为 .docx

### .txt 解析流程

直接按换行符拆分为段落。

---

## 改写流程

```
上传文件 → Rust 解析为段落列表 → 过滤不改写的段落 → 逐段调 API → 组装结果 → 返回前端
```

### 段落过滤规则（不改写的段落）

- 引用段落：以 `[数字]` 开头或包含 `[参考文献]`、`[References]`
- 公式段落：包含 LaTeX 标记（`$...$`、`\begin{equation}`）
- 图表标题：以「图 X」「表 X」「Figure X」「Table X」开头
- 空段落

### API 调用 Prompt 设计

**中文论文 prompt：**

```
你是学术论文润色专家。请对以下段落进行轻度改写：

要求：
1. 换用同义词替换常见表述
2. 调整句子结构（主动变被动、拆分长句、合并短句）
3. 保持段落整体意思不变
4. 保留所有专业术语、人名、地名、数据、公式、引用标记（如[1][2]）
5. 保持学术语气，不要口语化
6. 不要添加任何解释，只输出改写后的段落

原文：
{paragraph}
```

**英文论文 prompt：**

```
You are an academic paper polishing expert. Lightly rewrite the following paragraph:

Requirements:
1. Replace common expressions with synonyms
2. Restructure sentences (active→passive, split long sentences, merge short ones)
3. Preserve the original meaning
4. Keep all technical terms, names, data, formulas, citation markers (e.g. [1][2])
5. Maintain academic tone
6. Output ONLY the rewritten paragraph, no explanations

Original:
{paragraph}
```

### API 调用配置

- **请求格式：** OpenAI Chat Completions API 兼容格式
  ```
  POST {api_base}/chat/completions
  Authorization: Bearer {api_key}
  ```
- **请求体：**
  ```json
  {
    "model": "{model_name}",
    "messages": [
      {"role": "system", "content": "{system_prompt}"},
      {"role": "user", "content": "{paragraph}"}
    ],
    "temperature": 0.7,
    "max_tokens": 2000
  }
  ```
- **temperature 设为 0.7：** 保证改写有一定变化度，但不至于离谱

### 错误处理

- API 超时（30s）：重试 1 次，仍失败则标记该段落为「改写失败」，保留原文
- API 返回错误：显示错误信息，用户可修改配置重试
- 单段落失败不影响其他段落继续处理

---

## 对比视图实现

- 使用 diff-match-patch 库做段落级文本 diff
- 前端左右两栏渲染，逐段对应
- 差异部分高亮：新增文字绿色背景，删除文字红色背景+删除线
- 每段下方两个按钮：「接受改写」（默认）/「拒绝改写」
- 拒绝的段落恢复为原文

---

## 导出 .docx

- 使用 docx-rs crate 生成 .docx 文件
- 每个段落写入 `<w:p>` 节点
- 保留基本样式（粗体、斜体）
- 文件名：`{原文件名}_改写.docx`

---

## 数据流

```
[用户上传文件]
      ↓
[Rust 解析为 Paragraph[]]
      ↓
[过滤：跳过引用/公式/图表段落]
      ↓
[逐段调 API，更新进度]
      ↓
[返回 RewriteResult[] 到前端]
      ↓
[前端渲染对比视图]
      ↓
[用户逐段接受/拒绝]
      ↓
[导出 .docx]
```

---

## Tauri IPC 命令

| 命令 | 输入 | 输出 | 说明 |
|---|---|---|---|
| `parse_file` | file_path: String | Vec<Paragraph> | 解析文件为段落列表 |
| `rewrite_paragraphs` | paragraphs: Vec<String>, config: ApiConfig | Vec<RewriteResult> | 逐段改写 |
| `test_connection` | config: ApiConfig | bool | 测试 API 连通性 |
| `export_docx` | results: Vec<RewriteResult>, output_path: String | String | 导出 .docx |
| `save_config` | config: ApiConfig | () | 保存配置 |
| `load_config` | () | ApiConfig | 加载配置 |

### 数据结构

```rust
struct Paragraph {
    index: usize,
    text: String,
    style: ParagraphStyle,  // 粗体、斜体等
    skip: bool,             // 是否跳过改写
    skip_reason: Option<String>,  // 跳过原因
}

struct RewriteResult {
    index: usize,
    original: String,
    rewritten: String,
    accepted: bool,  // 用户是否接受
}

struct ApiConfig {
    api_base: String,    // API 地址
    api_key: String,     // API Key
    model: String,       // 模型名
    language: String,    // "zh" 或 "en"
}

struct ParagraphStyle {
    bold: bool,
    italic: bool,
    font_size: Option<u32>,
}
```

---

## 项目结构

```
ai-paper-rewriter/
├── src-tauri/
│   ├── src/
│   │   ├── main.rs          # Tauri 入口
│   │   ├── lib.rs           # 模块导出
│   │   ├── commands.rs      # IPC 命令
│   │   ├── parser/
│   │   │   ├── mod.rs
│   │   │   ├── docx.rs      # .docx 解析
│   │   │   ├── pdf.rs       # .pdf 解析
│   │   │   └── txt.rs       # .txt 解析
│   │   ├── rewriter.rs      # API 调用 + prompt
│   │   ├── exporter.rs      # .docx 导出
│   │   └── config.rs        # 配置管理
│   ├── Cargo.toml
│   └── tauri.conf.json
├── src/
│   ├── App.vue
│   ├── main.ts
│   ├── pages/
│   │   ├── Home.vue         # 主页：上传+处理
│   │   ├── Compare.vue      # 对比页
│   │   └── Settings.vue     # 设置页
│   ├── components/
│   │   ├── FileUpload.vue   # 文件上传组件
│   │   ├── ProgressBar.vue  # 进度条
│   │   ├── DiffView.vue     # 对比视图组件
│   │   └── ParagraphCard.vue # 单段落对比卡片
│   ├── stores/
│   │   └── app.ts           # Pinia store
│   └── utils/
│       └── diff.ts          # diff-match-patch 封装
├── package.json
├── vite.config.ts
└── tsconfig.json
```

---

## 验证方案

1. **功能测试：** 准备一篇 AI 生成的中文论文，分别在维普和 PaperPass 检测 AI 率，改写后再测
2. **文件格式测试：** .docx / .pdf / .txt 各上传一个，验证解析正确
3. **API 测试：** 分别用 GPT-5.5 和小米模型测试改写质量
4. **导出测试：** 导出的 .docx 用 Word/WPS 打开，检查格式是否正常
5. **边界测试：** 空文件、超大文件（>10MB）、纯公式段落
