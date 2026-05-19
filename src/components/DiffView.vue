<script setup lang="ts">
import { computed } from "vue";
import { diffText } from "../utils/diff";

const props = defineProps<{
  original: string;
  rewritten: string;
}>();

const originalParts = computed(() =>
  diffText(props.original, props.rewritten).filter((part) => part.type !== "insert"),
);
const rewrittenParts = computed(() =>
  diffText(props.original, props.rewritten).filter((part) => part.type !== "delete"),
);
</script>

<template>
  <div class="diff-grid">
    <div class="diff-pane">
      <div class="pane-label">原文</div>
      <p>
        <template v-for="(part, index) in originalParts" :key="`o-${index}`">
          <mark v-if="part.type === 'delete'" class="diff-delete">{{ part.text }}</mark>
          <span v-else>{{ part.text }}</span>
        </template>
      </p>
    </div>
    <div class="diff-pane">
      <div class="pane-label">改写后</div>
      <p>
        <template v-for="(part, index) in rewrittenParts" :key="`r-${index}`">
          <mark v-if="part.type === 'insert'" class="diff-insert">{{ part.text }}</mark>
          <span v-else>{{ part.text }}</span>
        </template>
      </p>
    </div>
  </div>
</template>
