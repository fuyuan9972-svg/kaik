<script setup lang="ts">
defineProps<{
  current: number;
  total: number;
  label: string;
}>();
</script>

<template>
  <div class="progress-block">
    <div class="progress-row">
      <span>{{ label }}</span>
      <strong>{{ current }} / {{ total }}</strong>
    </div>
    <div class="progress-track">
      <div
        class="progress-fill"
        :style="{ width: total === 0 ? '0%' : `${Math.round((current / total) * 100)}%` }"
      />
    </div>
  </div>
</template>
