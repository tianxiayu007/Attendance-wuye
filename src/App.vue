<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { Button, Message, ToggleSwitch } from "primevue";

const filePath = ref("");
const result = ref("");
const error = ref("");
const busy = ref(false);
const dragging = ref(false);
const autoRun = ref(true);
const configPath = ref("");

let unlisten: (() => void) | undefined;

const fileName = computed(() => {
  const parts = filePath.value.split(/[\\/]/).filter(Boolean);
  return parts.length ? parts[parts.length - 1] : filePath.value;
});

const isExcel = (path: string) => /\.(xlsx|xls)$/i.test(path);

/** 选择考勤机导出的 report.xls */
async function chooseFile() {
  const selected = await open({
    directory: false,
    multiple: false,
    filters: [{ name: "Excel Files", extensions: ["xlsx", "xls"] }],
    title: "选择考勤机导出的 report.xls",
    locale: "zh-CN",
  });
  if (selected) {
    setFile(selected as string);
  }
}

/** 选择可选的 baobiao.json（缺省时自动查找程序同目录的配置） */
async function chooseConfig() {
  const selected = await open({
    directory: false,
    multiple: false,
    filters: [{ name: "JSON", extensions: ["json"] }],
    title: "选择 baobiao.json（可选）",
    locale: "zh-CN",
  });
  if (selected) {
    configPath.value = selected as string;
  }
}

function setFile(path: string) {
  result.value = "";
  error.value = "";

  if (!isExcel(path)) {
    error.value = "仅支持考勤机导出的 .xls/.xlsx 文件";
    return;
  }

  filePath.value = path;
  if (autoRun.value) {
    void generateReport();
  }
}

/** 生成报表（直接读取考勤机导出文件，不再做 OA 中间转换） */
async function generateReport() {
  if (!filePath.value) {
    error.value = "请先选择或拖入考勤机导出的 report.xls";
    return;
  }

  busy.value = true;
  result.value = "";
  error.value = "";

  try {
    const message = await invoke<string>("attendance_report", {
      file: filePath.value,
      config: configPath.value || null,
    });
    if (message.startsWith("生成报表失败")) {
      error.value = message;
    } else {
      result.value = message;
    }
  } catch (e) {
    error.value = `生成报表失败：${e}`;
  } finally {
    busy.value = false;
  }
}

/**
 * Tauri 2 原生拖拽：webview 默认开启 dragDropEnabled，
 * 通过 onDragDropEvent 可以拿到被拖入文件的本地绝对路径。
 */
onMounted(async () => {
  unlisten = await getCurrentWebview().onDragDropEvent((event) => {
    const payload = event.payload;
    if (payload.type === "enter" || payload.type === "over") {
      dragging.value = true;
      return;
    }
    dragging.value = false;
    if (payload.type === "drop" && payload.paths.length > 0) {
      setFile(payload.paths[0]);
    }
  });
});

onUnmounted(() => {
  unlisten?.();
});
</script>

<template>
  <main class="page">
    <section
      class="drop-zone"
      :class="{ active: dragging, ready: !!filePath && !dragging }"
      @click="chooseFile"
    >
      <p class="drop-title">{{ $t("drop_hint") }}</p>
      <p class="drop-sub">{{ $t("drop_sub_hint") }}</p>
      <p v-if="filePath" class="file-name">{{ fileName }}</p>
      <p v-else class="file-name placeholder">{{ $t("no_file") }}</p>
    </section>

    <section class="row">
      <Button :label="$t('choose_file')" @click="chooseFile" />
      <Button :label="$t('choose_config')" severity="secondary" outlined @click="chooseConfig" />
      <Button
        :label="$t('generate')"
        :loading="busy"
        :disabled="!filePath || busy"
        @click="generateReport"
      />
      <span class="auto">
        <ToggleSwitch v-model="autoRun" inputId="autoRun" />
        <label for="autoRun">{{ $t("auto_run") }}</label>
      </span>
    </section>

    <section v-if="configPath" class="config-path">
      {{ $t("config_file") }}：{{ configPath }}
    </section>

    <Message v-if="error" severity="error" :closable="false">{{ error }}</Message>
    <Message v-else-if="result" severity="success" :closable="false">{{ result }}</Message>
  </main>
</template>

<style>
:root {
  font-family: Helvetica, Arial, sans-serif;
  font-size: 15px;
  line-height: 22px;
  font-weight: 400;

  color: #0f0f0f;
  background-color: #f6f6f6;

  font-synthesis: none;
  text-rendering: optimizeLegibility;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
  -webkit-text-size-adjust: 100%;
}

body {
  margin: 0;
  padding: 0;
}

.page {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 16px;
  box-sizing: border-box;
}

.drop-zone {
  border: 2px dashed #b9c2cf;
  border-radius: 10px;
  background: #ffffff;
  padding: 18px 16px;
  text-align: center;
  cursor: pointer;
  transition: all 0.15s ease-in-out;
  user-select: none;
}

.drop-zone.active {
  border-color: #2f6fed;
  background: #eef4ff;
  transform: scale(1.01);
}

.drop-zone.ready {
  border-style: solid;
  border-color: #34a853;
}

.drop-title {
  margin: 0;
  font-size: 15px;
  font-weight: 600;
}

.drop-sub {
  margin: 4px 0 0;
  font-size: 12px;
  color: #6b7280;
}

.file-name {
  margin: 10px 0 0;
  font-size: 13px;
  color: #34a853;
  word-break: break-all;
}

.file-name.placeholder {
  color: #9aa4b2;
}

.row {
  display: flex;
  align-items: center;
  gap: 8px;
}

.auto {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-left: auto;
  font-size: 13px;
  color: #4b5563;
}

.config-path {
  font-size: 12px;
  color: #6b7280;
  word-break: break-all;
}
</style>
