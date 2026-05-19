<script setup lang="ts">
import { Check, RotateCcw, TriangleAlert } from "lucide-vue-next";
import DiffView from "./DiffView.vue";
import type { RewriteResult } from "../types";

defineProps<{
  result: RewriteResult;
}>();

const emit = defineEmits<{
  accept: [index: number];
  reject: [index: number];
}>();
</script>

<template>
  <article class="paragraph-card" :class="{ rejected: !result.accepted }">
    <header class="paragraph-header">
      <div>
        <strong>段落 {{ result.index + 1 }}</strong>
        <span v-if="result.failed" class="status danger">
          <TriangleAlert :size="14" />
          改写失败
        </span>
        <span v-else-if="result.skipped" class="status muted">
          跳过{{ result.error ? `：${result.error}` : "" }}
        </span>
        <span v-else-if="!result.accepted" class="status muted">保留原文</span>
        <span v-else class="status ok">接受改写</span>
      </div>
      <div class="button-row">
        <button class="icon-button" type="button" title="接受改写" @click="emit('accept', result.index)">
          <Check :size="16" />
        </button>
        <button class="icon-button" type="button" title="拒绝改写" @click="emit('reject', result.index)">
          <RotateCcw :size="16" />
        </button>
      </div>
    </header>
    <DiffView :original="result.original" :rewritten="result.rewritten" />
    <footer v-if="result.error" class="paragraph-error">{{ result.error }}</footer>
  </article>
</template>
