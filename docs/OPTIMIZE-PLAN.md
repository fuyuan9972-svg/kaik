# AI Paper Rewriter 优化计划

> 生成时间：2026-05-21
> 目标：提升启动性能、减少冗余写盘、消除代码重复、改善可维护性

---

## Task 1：初始化并行化（前端）

**文件**：`src/stores/app.ts` 第 136-162 行

**现状**：`initialize()` 里 6 个 `invoke` 串行调用，启动慢。

**改动**：
```typescript
// 改前：串行
this.sessions = await invoke("load_sessions");
this.aigcCalibrations = await invoke("load_aigc_calibrations");
this.detectionSnapshots = await invoke("load_detection_snapshots");
this.feedbackRecords = await invoke("load_feedback_records");
this.calibrationRules = await invoke("load_calibration_rules");

// 改后：并行
const [sessions, calibrations, snapshots, feedback, rules] = await Promise.all([
  invoke<RewriteSession[]>("load_sessions"),
  invoke<AigcCalibrationSample[]>("load_aigc_calibrations"),
  invoke<AigcDetectionSnapshot[]>("load_detection_snapshots"),
  invoke<AigcFeedbackRecord[]>("load_feedback_records"),
  invoke<AigcCalibrationRule[]>("load_calibration_rules"),
]);
this.sessions = sessions;
this.aigcCalibrations = calibrations;
this.detectionSnapshots = snapshots;
this.feedbackRecords = feedback;
this.calibrationRules = rules;
```

**验收**：`npm run tauri dev` 启动后，设置页/校准页/历史页数据正常加载。

---

## Task 2：setAccepted debounce（前端）

**文件**：`src/stores/app.ts` 第 507-513 行

**现状**：每点一次勾选就调 `saveResults()`，批量操作时频繁写盘。

**改动**：
1. 在 store 里加一个 `_saveResultsTimer` 字段（初始 `null`）
2. `setAccepted` 里改成 debounce 800ms 后再调 `saveResults()`
3. 在 `rewrite()`、`exportDocx()` 等关键路径开头，先 flush 一次（如果 timer 存在就 clearTimeout + 立即 save）

```typescript
// 新增字段
_saveResultsTimer: null as ReturnType<typeof setTimeout> | null,

// 修改 setAccepted
setAccepted(index: number, accepted: boolean) {
  const item = this.results.find((result) => result.index === index);
  if (item) {
    item.accepted = accepted;
    if (this._saveResultsTimer) clearTimeout(this._saveResultsTimer);
    this._saveResultsTimer = setTimeout(() => {
      this._saveResultsTimer = null;
      void this.saveResults();
    }, 800);
  }
},
```

**验收**：快速连续点 5 个勾选，network/IPC 里只看到 1 次 save_session 调用。

---

## Task 3：减少 rewrite 循环写盘次数（后端）

**文件**：`src-tauri/src/commands.rs` 第 83-133 行（`rewrite_paragraphs` 函数）

**现状**：每处理 1 段就 `sort_by_key` + `upsert_session`。

**改动**：
1. 去掉循环内的 `results.sort_by_key`，改用 `BTreeMap<usize, RewriteResult>` 收集结果，天然按 index 排序
2. session 持久化改成每 5 段存一次，最后再存一次
3. 前端 `rewrite-progress` 事件里已经有实时更新，不需要后端每段都写盘

```rust
// 循环内改成：
let position_in_batch = position % 5;
if position_in_batch == 4 || current == total {
    // 每 5 段或最后一段才写盘
    if let Some(session) = active_session.as_mut() {
        session.results = results.clone();
        session.updated_at = current_timestamp();
        crate::config::upsert_session(&app, session.clone())
            .map_err(|error| error.to_string())?;
    }
}
```

**验收**：改写 100 段时，`upsert_session` 调用次数从 ~100 降到 ~20。

---

## Task 4：系统提示词只生成一次（后端）

**文件**：`src-tauri/src/rewriter.rs` 第 263-318 行（`rewrite_paragraph` 函数）

**现状**：每段都调 `system_prompt()` 重新生成字符串，而同一次任务里 config/options 不变。

**改动**：
1. 在 `rewrite_paragraphs`（commands.rs）里，循环前算一次 `system_prompt`
2. 把 `&str` 传给 `rewrite_paragraph`，而不是让它自己算
3. `rewrite_paragraph` 签名改为接收 `system_prompt: &str`

```rust
// commands.rs 循环前：
let prompt = crate::rewriter::system_prompt(
    &config.language,
    &config.prompt_profile,
    &rewrite_options,
    rewrite_options.external_report.as_ref(),
);

// 循环内：
let result = crate::rewriter::rewrite_paragraph(&client, &config, &rewrite_options, paragraph, &prompt).await;
```

**注意**：`system_prompt` 函数需要从 `fn` 改为 `pub fn`。

**验收**：改写功能正常，日志或断点确认 prompt 只构建一次。

---

## Task 5：提取 Rust 公共工具函数

**新建文件**：`src-tauri/src/utils.rs`

**需要迁移的函数**：
| 函数 | 来源文件 | 说明 |
|------|---------|------|
| `cjk_count` | aigc_detector.rs:963, ai_aigc_detector.rs:352, trial_evaluator.rs:234 | 计算 CJK 字符数 |
| `truncate_text` | aigc_detector.rs:975, ai_aigc_detector.rs:358, trial_evaluator.rs:222 | 截断文本 |
| `is_body_candidate` | aigc_detector.rs:925, ai_aigc_detector.rs:344 | 判断是否正文段落 |

**改动步骤**：
1. 创建 `src-tauri/src/utils.rs`，把这三个函数放进去
2. 在 `src-tauri/src/lib.rs` 里加 `mod utils;`
3. 在 aigc_detector.rs、ai_aigc_detector.rs、trial_evaluator.rs 里删掉各自的实现，改为 `use crate::utils::{cjk_count, truncate_text, is_body_candidate};`
4. `is_body_candidate` 两个版本略有不同：
   - aigc_detector 版：检查 `compact.len() < 45` + 关键词/参考文献过滤 + `cjk_count >= 35`
   - ai_aigc_detector 版：只检查 `cjk >= 35 && chars >= 45`
   - **合并方案**：保留 aigc_detector 的完整版（更严格），ai_aigc_detector 调用同一个

**验收**：`cargo build` 通过，`cargo test` 通过。

---

## Task 6：提取 JSON 解析公共函数（后端）

**新建文件**：同 Task 5 的 `src-tauri/src/utils.rs`，或单独放

**需要迁移的函数**：
| 函数 | 来源 |
|------|------|
| `extract_json_object` | ai_aigc_detector.rs:280-288, trial_evaluator.rs:100-108（相同的 find('{')..rfind('}') 模式）|

**改动**：
```rust
// utils.rs 新增
pub fn extract_json_object(text: &str) -> anyhow::Result<&str> {
    let trimmed = text.trim();
    if let Some(start) = trimmed.find('{') {
        let end = trimmed.rfind('}').context("AI 返回内容不是 JSON 对象")?;
        Ok(&trimmed[start..=end])
    } else {
        Ok(trimmed)
    }
}
```

然后 ai_aigc_detector 和 trial_evaluator 里删掉各自的实现，调用 `crate::utils::extract_json_object`。

**验收**：`cargo build` + `cargo test` 通过。

---

## Task 7：合并 buildTrialDraftParagraphs 和 mergeRewriteResults（前端）

**文件**：`src/stores/app.ts` 第 730-751 行

**现状**：两个方法逻辑几乎一样，只是 results 来源不同。

**改动**：
```typescript
// 删掉 buildTrialDraftParagraphs，统一用 mergeRewriteResults
buildTrialDraftParagraphs() {
  return this.mergeRewriteResults(
    this.paragraphs,
    this.results,  // 已经是 filter accepted 的结果
  );
},

// 修改 mergeRewriteResults，默认 filter accepted
mergeRewriteResults(paragraphs: Paragraph[], results: RewriteResult[]) {
  const replacements = new Map(
    results
      .filter((item) => item.accepted && !item.failed && !item.skipped)
      .map((item) => [item.index, item.rewritten]),
  );
  return paragraphs.map((paragraph) => ({
    ...paragraph,
    text: replacements.get(paragraph.index) ?? paragraph.text,
  }));
},
```

调用方 `rewrite()` 第 206 行从 `this.buildTrialDraftParagraphs()` 改成 `this.mergeRewriteResults(this.paragraphs, this.results)`。

**验收**：叠加跑全文和直接全文改写功能正常。

---

## Task 8：export_docx 不再 reload 全部 sessions（前端）

**文件**：`src/stores/app.ts` 第 407-438 行

**现状**：导出后 `load_sessions` 拉回所有 session。

**改动**：删掉 `this.sessions = await invoke("load_sessions")` 和后面的 `find`/`applySession`，直接用 `updateCurrentSession({ exportedPath })` 更新本地状态即可。

```typescript
// 改后：
if (this.currentSessionId) {
  const exportedPath = await invoke<string>("export_session_docx", {
    sessionId: this.currentSessionId,
    outputPath,
  });
  this.updateCurrentSession({ exportedPath });
  await this.analyzeExportedDocx(exportedPath);
} else {
  const exportedPath = await invoke<string>("export_docx", {
    results: this.results,
    outputPath,
  });
  await this.analyzeExportedDocx(exportedPath);
}
this.status = "导出完成，已自动检测导出稿";
```

**验收**：导出 docx 后，历史记录里 exportedPath 正确显示。

---

## Task 9：清理类型定义（前端）

**文件**：`src/types.ts` 第 58 行

**改动**：
```typescript
// 改前
taskType?: "sampleTrial" | "guidedRewrite" | "fullRewrite" | string | null;

// 改后（加上遗漏的 stackedFullRewrite，去掉 | string）
taskType?: "sampleTrial" | "guidedRewrite" | "fullRewrite" | "stackedFullRewrite" | null;
```

同步检查 `app.ts` 和 `Calibration.vue` 里引用到 taskType 的地方，确保类型匹配。

**验收**：`npm run build` 类型检查通过。

---

## Task 10：抽取 resetRewriteState（前端）

**文件**：`src/stores/app.ts` 第 164-195 行（parseFile 里大段重置）

**改动**：把重置逻辑抽成一个 action：
```typescript
resetRewriteState() {
  this.results = [];
  this.aigcAnalysis = null;
  this.originalSnapshotId = "";
  this.originalAigcAnalysis = null;
  this.rewrittenSnapshotId = "";
  this.rewrittenAigcAnalysis = null;
  this.trialDraftAigcAnalysis = null;
  this.stackedFullAigcAnalysis = null;
  this.directFullAigcAnalysis = null;
  this.sampleIndices = [];
  this.trialEvaluation = null;
  this.taskStage = "idle";
  this.progressCurrent = 0;
  this.progressTotal = 0;
  this.currentSessionId = "";
},
```

`parseFile` 里改成 `this.resetRewriteState()`。

**验收**：上传新文件后，旧状态正确清除。

---

## Task 11：activePromptProfiles 简化（前端）

**文件**：`src/stores/app.ts` 第 72 行

**改动**：删掉 `activePromptProfiles` Set，改成内联判断：
```typescript
// 改前
if (!activePromptProfiles.has(loadedConfig.promptProfile)) {

// 改后
if (loadedConfig.promptProfile !== "sample_calibrated_17_v2"
    && loadedConfig.promptProfile !== "sample_calibrated_17_success") {
```

**验收**：启动后 promptProfile 正确回退到 v2。

---

## 执行顺序建议

| 优先级 | Task | 风险 | 预计耗时 |
|--------|------|------|---------|
| P0 | Task 1（初始化并行） | 低 | 10 min |
| P0 | Task 3（减少写盘） | 低 | 20 min |
| P0 | Task 5（提取公共函数） | 低 | 30 min |
| P0 | Task 6（JSON 解析提取） | 低 | 10 min |
| P1 | Task 4（prompt 只算一次） | 中 | 20 min |
| P1 | Task 2（debounce） | 低 | 15 min |
| P1 | Task 7（合并重复方法） | 低 | 10 min |
| P1 | Task 8（export 不 reload） | 低 | 10 min |
| P2 | Task 9（类型清理） | 低 | 5 min |
| P2 | Task 10（resetRewriteState） | 低 | 5 min |
| P2 | Task 11（Set 简化） | 低 | 5 min |

**总计约 2.5 小时**。每完成一个 Task 就 `cargo build && cargo test && npm run build` 验证一次。
