# AI Paper Rewriter 优化计划

## Context

`ai-paper-rewriter` 是一个 Tauri 2 + Vue 3 + Rust 桌面应用，用于改写中文学术论文以降低 AIGC 检测率（PaperPass、维普）。当前版本 v0.1.1 已经实现了核心功能（文件解析、本地+AI混合检测、校准反馈循环、试跑改写、全文改写、diff对比、docx导出），但存在架构臃肿、检测精度待提升、UX 粗糙等问题。

---

## 不动的部分

- **parser/** 模块 — 干净、正确、skip逻辑完善
- **14个内置校准样本** — 真实数据，检测模型的基石
- **vip_structure_risk** — 维普"包装段"检测的领域知识，不能动
- **external_rewrite_guidance** — 维普报告反推改写指导，保留
- **stacked full rewrite** 流程 — 试跑→合并→全文的好设计
- **exporter.rs** — 简洁正确
- **diff-match-patch** 集成 — 够用

---

## P0 — 最高优先级（检测精度 + 改写质量）

### P0-1: 拆分 rewriter.rs（828行 → 4个模块）

**现状**: prompts、API调用、段落选择、质量检查全在一个文件
**目标**:
- `rewriter/prompts.rs` — 所有 prompt 文本常量（~250行）
- `rewriter/api.rs` — `chat_once`、`complete_once`、响应解析、重试逻辑
- `rewriter/selection.rs` — `select_paragraphs`、`select_sample_indices`、skip逻辑
- `rewriter/mod.rs` — `rewrite_paragraph`、质量检查、`build_success_result`

**改**: `src-tauri/src/rewriter.rs` → `src-tauri/src/rewriter/` 目录，更新 `lib.rs` 模块声明
**复杂度**: 中。纯代码搬迁，不改逻辑

### P0-2: API 调用加重试

**现状**: `chat_once` 单次调用，45秒超时，无重试。网络抖动或429直接失败
**目标**: 指数退避重试3次（2s/4s/8s），重试网络错误和429/500/502/503，不重试401/403
**改**: 新建的 `rewriter/api.rs` 中的 `chat_once`
**复杂度**: 低-中。`reqwest` client 可 clone，加 `tokio::time::sleep`

### P0-3: 校准从单条全局规则 → 分桶规则

**现状**: `calibration.rs` 的 `build_rules` 只生成一条 `global-error-correction`（所有反馈的平均误差）
**问题**: 如果低AI率文本被高估+8、高AI率文本被低估-5，全局平均+1.5，两边都不准
**目标**: 生成3条分桶规则：
- 低AI率桶（实测 < 22%）
- 中AI率桶（22%-45%）
- 高AI率桶（> 45%）
- 每桶独立 correction、confidence、sample_count

**改**: `src-tauri/src/calibration.rs` 的 `build_rules` 函数
**注意**: `aigc_detector.rs` 的 `blended_correction` 已支持多规则加权平均，无需改

### P0-4: 前端检测调用去重

**现状**: 4个 action 都调 `analyze_aigc_file_ai`，状态管理略有不同
- `analyzeAigcFileAi` → 设 originalAigcAnalysis + 跳转 detect 页
- `analyzeCurrentFileAi` → 设 originalAigcAnalysis + 不跳转
- `analyzeExportedDocx` → 设 rewrittenAigcAnalysis
- `startUnifiedTask` → parseFile + analyzeCurrentFileAi

**目标**: 提取 `_runAiDetection(filePath, target: "original" | "rewritten")` 公共方法
**改**: `src/stores/app.ts` 186-527行

---

## P1 — 高优先级（UX + 可维护性）

### P1-1: Compare 页面虚拟滚动

**现状**: `v-for` 渲染所有 ParagraphCard，100段论文 = 100个 diff 计算 + DOM 节点
**目标**: IntersectionObserver 懒渲染，只渲染可见区域 + 上下各5个 buffer
**改**: `src/pages/Compare.vue`、`src/components/ParagraphCard.vue`，可能加 `useVirtualScroll` composable

### P1-2: accept/reject 撤销

**现状**: `setAccepted` 立即生效，无法撤销
**目标**: 20条撤销栈 + toast提示"撤销"按钮
**改**: `src/stores/app.ts`（加 `decisionHistory` + `undoLastDecision`）、`src/pages/Compare.vue`（toast）

### P1-3: 进度条加 ETA

**现状**: 只显示"已完成第15/80段"
**目标**: 显示预计剩余时间、每段耗时、段落级迷你进度条（绿=成功/红=失败/灰=等待）
**改**: `src/components/ProgressBar.vue`、`src/stores/app.ts`（加 `rewriteStartTime`）

### P1-4: 检测快照自动清理

**现状**: `aigc-detection-snapshots.json` 永久积累，无清理
**目标**: 初始化时清理30天以上且未被反馈记录引用的快照，上限200条
**改**: `src-tauri/src/config.rs`（加清理函数）、`commands.rs`（加命令）、`Calibration.vue`（加按钮）

### P1-5: 拆分单体 store（可选，视前4项改动后 store 膨胀程度决定）

**现状**: 719行、33个状态字段、25个 action
**目标**: 拆为 `useRewriteStore`（改写相关）、`useDetectionStore`（检测相关）、`useAppStore`（导航+配置）
**改**: `src/stores/app.ts` → 3个文件，所有页面 import 更新

---

## P2 — 中优先级（代码质量 + 小UX）

### P2-1: 提取共享文本工具函数

**现状**: `cjk_count` 在4个文件中有4份拷贝，`truncate_text` 3份，`is_body_candidate` 2份（且实现有微妙差异）
**目标**: 创建 `src-tauri/src/text_utils.rs`，统一实现
**改**: 新建文件，更新4个消费方

### P2-2: session 文件大小管理

**现状**: session 内嵌完整 paragraphs + results，几个 session 后 sessions.json 可达5-10MB
**目标**: 7天以上的 session 截断文本为200字符（最近3个保留完整），`load_sessions` 时自动执行
**改**: `src-tauri/src/config.rs`

### P2-3: 校准页面视觉层级优化

**现状**: 6个统计卡片 + bin网格 + 表单 + 规则 + 反馈记录，同一视觉层级
**目标**: 分组卡片、折叠反馈记录（默认只显示最近5条）、bin网格加颜色编码（绿/黄/红）
**改**: `src/pages/Calibration.vue`、`src/styles.css`

### P2-4: chat_once 响应解析重构

**现状**: `extract_chat_content` 试9个JSON路径，`extract_text_from_json` 再试5个，ad-hoc
**目标**: 统一为有序 fallback 链 + 日志记录哪条路径命中
**改**: `rewriter/api.rs`（拆分后）

### P2-5: 校准数据导出/导入

**现状**: 校准数据绑定本机，无法备份或迁移
**目标**: 新增 `export_calibration_data` / `import_calibration_data` 命令 + Calibration 页面按钮
**改**: `commands.rs`、`config.rs`、`lib.rs`、`Calibration.vue`、`app.ts`

---

## P3 — 锦上添花（暂不推荐）

| 项目 | 说明 | 为什么不急 |
|------|------|-----------|
| Vue Router | 替换 v-if 为路由 | 桌面 app 不需要 URL 导航，改动大收益小 |
| 批量文件处理 | 多文件队列 | 小众需求，改动 session 模型 |
| Prompt A/B 测试 | 试跑分两组对比 | 翻倍 API 成本，UI 复杂度高 |
| SQLite 替代 JSON | 换数据库 | 当前数据量小，JSON 够用，迁移成本高 |

---

## 实施顺序

### Phase 1：检测精度 + 核心质量（1-2周）
1. P0-1: 拆分 rewriter.rs
2. P0-2: 加重试逻辑
3. P0-3: 分桶校准规则
4. P2-1: 提取共享 text_utils

### Phase 2：UX 打磨（1周）
5. P0-4: 检测调用去重
6. P1-3: 进度条加 ETA
7. P1-2: accept/reject 撤销
8. P2-3: 校准页视觉优化

### Phase 3：数据管理 + 性能（1周）
9. P1-1: Compare 虚拟滚动
10. P1-4: 快照自动清理
11. P2-2: session 文件管理
12. P2-5: 校准数据导出/导入

### Phase 4：结构优化（可选）
13. P1-3: 拆分 store（视需要）
14. P2-4: 响应解析重构
15. P3-* 项根据用户反馈决定

---

## 验证方式

每个改动完成后：
1. `cargo build` / `cargo check` 确保 Rust 编译通过
2. `npm run build` 确保前端类型检查通过
3. `npm run tauri:dev` 启动应用，手动走一遍核心流程：
   - 上传 .docx/.doc/.pdf/.txt/.rtf/.md → 检测 → 试跑20段 → 全文改写 → 对比 → 导出
4. P0-3 的验证：在 Calibration 页面录几条反馈，观察校准规则是否按桶生成
5. P0-2 的验证：断网/限速环境下观察重试行为
