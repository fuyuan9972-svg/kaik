<script setup lang="ts">
import { FileText, Trash2 } from "lucide-vue-next";
import { useAppStore } from "../stores/app";

const store = useAppStore();

function formatTime(value: string) {
  if (value === "legacy") {
    return "旧缓存";
  }
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return date.toLocaleString();
}

function profileName(value: string) {
  if (value === "sample_calibrated_17_v2") return "成功链路17 2.0";
  if (value === "sample_calibrated_17_success") return "成功链路17（基线）";
  if (value === "sample_calibrated_17") return "样本校准17%（旧版）";
  if (value === "sample_calibrated_28") return "旧-样本校准28%";
  if (value === "directive_aigc_reduce") return "旧-指令降AIGC";
  if (value === "directive_aigc_reduce_legacy") return "旧-指令降AIGC";
  if (value === "doubao_plain_humanize") return "豆包降AI";
  if (value === "plain_spoken_humanize") return "大白话降AI";
  if (value === "local_light_rewrite") return "局部轻改";
  if (value === "local_depattern") return "旧-局部";
  if (value === "paperpass_restructure") return "旧-PP重构";
  if (value === "conservative_rewrite") return "旧-保守";
  if (value === "light_rewrite") return "旧-轻改";
  if (value === "academic_humanizer") return "旧-学术";
  return value || "未知策略";
}

function aiRateLabel(session: { currentAiRate?: number | null; targetAiRate?: number | null }) {
  if (session.currentAiRate == null || session.targetAiRate == null) {
    return "";
  }
  return ` · AI ${session.currentAiRate} -> ${session.targetAiRate}`;
}
</script>

<template>
  <main class="page">
    <section class="toolbar">
      <div>
        <h1>历史记录</h1>
        <p>本地保存的改写和测试记录。</p>
      </div>
    </section>

    <div v-if="store.historySessions.length === 0" class="empty-state">还没有历史记录。</div>
    <div v-else class="history-list">
      <article
        v-for="session in store.historySessions"
        :key="session.id"
        class="history-item"
        :class="{ active: session.id === store.currentSessionId }"
      >
        <button class="history-main" type="button" @click="store.openSession(session)">
          <FileText :size="18" />
          <span>
            <strong>{{ session.fileName }}</strong>
            <small>
              {{ formatTime(session.updatedAt) }} · {{ profileName(session.promptProfile) }}
              <template v-if="session.isSample"> · 测试 {{ session.sampleLimit }} 段</template>
              {{ aiRateLabel(session) }}
              · 结果 {{ session.results.filter((item) => !item.skipped).length }} 段
            </small>
          </span>
        </button>
        <button class="icon-button" type="button" title="删除记录" @click="store.deleteSession(session.id)">
          <Trash2 :size="16" />
        </button>
      </article>
    </div>
  </main>
</template>
