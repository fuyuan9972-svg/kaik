<script setup lang="ts">
import { open } from "@tauri-apps/plugin-dialog";
import { FileUp } from "lucide-vue-next";

const emit = defineEmits<{
  selected: [path: string];
}>();

async function chooseFile() {
  const selected = await open({
    multiple: false,
    filters: [
      {
        name: "Paper",
        extensions: ["docx", "pdf", "txt"],
      },
    ],
  });
  if (typeof selected === "string") {
    emit("selected", selected);
  }
}
</script>

<template>
  <button class="upload-zone" type="button" @click="chooseFile">
    <FileUp :size="34" />
    <span>选择论文文件</span>
    <small>.docx / .pdf / .txt</small>
  </button>
</template>
