<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { Button, DatePicker, Message, ToggleSwitch } from "primevue";

const filePath = ref("");
const result = ref("");
const error = ref("");
const busy = ref(false);
const dragging = ref(false);
const autoRun = ref(true);
const configPath = ref("");
/** 上午/下午打卡点（默认 08:00 / 18:00） */
const morningTime = ref<Date>(atTime(8, 0));
const afternoonTime = ref<Date>(atTime(18, 0));
/** settings.json 的路径与落盘状态（无配置文件时用内置默认值运行） */
const settingsPath = ref("");
const saveState = ref<"idle" | "saved" | "error">("idle");
const saveError = ref("");

let unlisten: (() => void) | undefined;
/** 避免启动时回填配置触发一次无意义的落盘 */
let ready = false;

const fileName = computed(() => {
  const parts = filePath.value.split(/[\\/]/).filter(Boolean);
  return parts.length ? parts[parts.length - 1] : filePath.value;
});

const isExcel = (path: string) => /\.(xlsx|xls)$/i.test(path);

/** 构造“今天 HH:mm”的 Date 对象，供时间选择控件使用 */
function atTime(hour: number, minute: number): Date {
  const d = new Date();
  d.setHours(hour, minute, 0, 0);
  return d;
}

/** 控件值 → HH:MM 文本（同时兼容 Date 与字符串） */
function toHhMm(value: Date | string | null | undefined): string {
  if (!value) return "";
  if (value instanceof Date) {
    const hh = String(value.getHours()).padStart(2, "0");
    const mm = String(value.getMinutes()).padStart(2, "0");
    return `${hh}:${mm}`;
  }
  const m = String(value).match(/(\d{1,2}):(\d{2})/);
  return m ? `${m[1].padStart(2, "0")}:${m[2]}` : String(value);
}

/**
 * 把界面上的配置项写入 settings.json（文件不存在则创建）。
 * 只在用户**真的改了默认项**时调用，因此默认配置下不会生成多余文件。
 */
async function persistSettings() {
  if (!ready) return;
  try {
    settingsPath.value = await invoke<string>("save_settings", {
      autoRun: autoRun.value,
      morningTime: toHhMm(morningTime.value),
      afternoonTime: toHhMm(afternoonTime.value),
    });
    saveState.value = "saved";
    saveError.value = "";
  } catch (e) {
    saveState.value = "error";
    saveError.value = String(e);
  }
}

/** 用户修改“拖入后自动生成” */
function onAutoRunChange() {
  result.value = "";
  void persistSettings();
}

/** 用户修改打卡点 */
function onTimeChange() {
  result.value = "";
  void persistSettings();
}

/** 启动时读取界面配置；没有 settings.json 就用内置默认值 */
async function loadSettings() {
  try {
    const data = await invoke<{
      path: string;
      exists: boolean;
      settings: { auto_run: boolean; morning_time: string; afternoon_time: string };
    }>("load_settings");

    settingsPath.value = data.path;
    if (data.exists) {
      const { auto_run, morning_time, afternoon_time } = data.settings;
      autoRun.value = auto_run;
      applyTime(morningTime, morning_time, 8, 0);
      applyTime(afternoonTime, afternoon_time, 18, 0);
    }
  } catch (e) {
    // 读取失败不影响使用，继续用默认值
    console.warn("读取界面配置失败：", e);
  } finally {
    ready = true;
  }
}

/** 把 `HH:MM` 文本写入时间控件（非法时保持默认） */
function applyTime(target: { value: Date }, text: string, hour: number, minute: number) {
  const m = String(text ?? "").match(/(\d{1,2}):(\d{2})/);
  target.value = m ? atTime(Number(m[1]), Number(m[2])) : atTime(hour, minute);
}

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
      morningTime: toHhMm(morningTime.value),
      afternoonTime: toHhMm(afternoonTime.value),
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
  await loadSettings();

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

    <section class="punch-row">
      <span class="punch-item">
        <label for="morningTime">{{ $t("morning_time") }}</label>
        <DatePicker
          id="morningTime"
          v-model="morningTime"
          timeOnly
          hourFormat="24"
          showIcon
          iconDisplay="input"
          :manualInput="false"
          class="time-input"
          @date-change="onTimeChange"
        />
      </span>
      <span class="punch-item">
        <label for="afternoonTime">{{ $t("afternoon_time") }}</label>
        <DatePicker
          id="afternoonTime"
          v-model="afternoonTime"
          timeOnly
          hourFormat="24"
          showIcon
          iconDisplay="input"
          :manualInput="false"
          class="time-input"
          @date-change="onTimeChange"
        />
      </span>
      <span class="punch-tip">{{ $t("punch_tip") }}</span>
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
        <ToggleSwitch v-model="autoRun" inputId="autoRun" @change="onAutoRunChange" />
        <label for="autoRun">{{ $t("auto_run") }}</label>
      </span>
    </section>

    <section v-if="configPath" class="config-path">
      {{ $t("config_file") }}：{{ configPath }}
    </section>

    <section class="settings-state">
      <span v-if="saveState === 'saved'" class="ok">{{ $t("settings_saved") }}：{{ settingsPath }}</span>
      <span v-else-if="saveState === 'error'" class="bad">{{ saveError }}</span>
      <span v-else class="muted">{{ $t("settings_default") }}</span>
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

.punch-row {
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 8px 12px;
  background: #ffffff;
  border: 1px solid #e5e7eb;
  border-radius: 8px;
}

.punch-item {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  color: #374151;
}

.punch-item label {
  white-space: nowrap;
  user-select: none;
}

.punch-item .time-input {
  width: 116px;
}

.punch-tip {
  margin-left: auto;
  font-size: 12px;
  color: #6b7280;
}

.auto {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-left: auto;
  font-size: 13px;
  color: #4b5563;
}

.settings-state {
  font-size: 12px;
  color: #6b7280;
  word-break: break-all;
}

.settings-state .ok {
  color: #34a853;
}

.settings-state .bad {
  color: #d93025;
}

.settings-state .muted {
  color: #9aa4b2;
}

.config-path {
  font-size: 12px;
  color: #6b7280;
  word-break: break-all;
}
</style>
