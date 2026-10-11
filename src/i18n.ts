import { createI18n } from 'vue-i18n';

// 引入语言包
const messages = {
  en: {
    generate: 'Generate',
    choose_file: 'Choose file',
    choose_config: 'Choose config',
    config_file: 'Config',
    auto_run: 'Generate right after dropping',
    drop_hint: 'Drag the attendance file (report.xls) here',
    drop_sub_hint: 'or click to pick it from the attendance machine export',
    no_file: 'No file selected',
    language: 'English'
  },
  zh: {
    generate: '生成报表',
    choose_file: '选择文件',
    choose_config: '选择配置',
    config_file: '配置文件',
    auto_run: '拖入后自动生成',
    drop_hint: '把考勤机导出的 report.xls 拖到这里',
    drop_sub_hint: '也可以点击此处 / 下方按钮手动选择文件',
    no_file: '尚未选择文件',
    language: '中文'
  }
};

export default createI18n({
  legacy: false,
  locale: 'zh', // 默认语言
  fallbackLocale: 'en',
  availableLocales: ['en', 'zh'],
  globalInjection: true,
  messages
});
