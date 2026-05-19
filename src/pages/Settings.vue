<script setup lang="ts">
import { ArrowLeft, PlugZap, Save } from "lucide-vue-next";
import { useAppStore } from "../stores/app";

const store = useAppStore();
</script>

<template>
  <main class="page settings-page">
    <section class="toolbar">
      <div>
        <h1>设置</h1>
        <p>配置 OpenAI Chat Completions 兼容接口。</p>
      </div>
      <button class="secondary-button" type="button" @click="store.page = store.results.length ? 'compare' : 'home'">
        <ArrowLeft :size="16" />
        返回
      </button>
    </section>

    <section class="panel form-panel">
      <h2>改写 API</h2>
      <label>
        <span>API 地址</span>
        <input v-model="store.config.apiBase" placeholder="https://api.example.com/v1" />
      </label>
      <label>
        <span>API Key</span>
        <input v-model="store.config.apiKey" type="password" placeholder="sk-..." />
      </label>
      <label>
        <span>模型名</span>
        <input v-model="store.config.model" placeholder="gpt-5.5" />
      </label>
      <label>
        <span>改写语言</span>
        <select v-model="store.config.language">
          <option value="zh">中文</option>
          <option value="en">英文</option>
        </select>
      </label>
      <p class="form-note">用于开始改写和测试 20 段。</p>
    </section>

    <section class="panel form-panel">
      <h2>检测 AI</h2>
      <label>
        <span>检测 API 地址</span>
        <input v-model="store.config.detectApiBase" placeholder="留空则使用改写 API 地址" />
      </label>
      <label>
        <span>检测 API Key</span>
        <input v-model="store.config.detectApiKey" type="password" placeholder="留空则使用改写 API Key" />
      </label>
      <label>
        <span>检测模型名</span>
        <input v-model="store.config.detectModel" placeholder="留空则使用改写模型" />
      </label>
      <p class="form-note">用于检测页的 AI 混合检测；可和改写模型分开。</p>
      <div class="action-row">
        <button class="secondary-button" type="button" :disabled="store.loading" @click="store.testConnection">
          <PlugZap :size="16" />
          测试改写 API
        </button>
        <button class="primary-button" type="button" :disabled="store.loading" @click="store.saveConfig">
          <Save :size="16" />
          保存配置
        </button>
      </div>
    </section>
  </main>
</template>
