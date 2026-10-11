import { createApp } from 'vue'
import i18n from './i18n'
import App from './App.vue'
import PrimeVue from 'primevue/config';
import Aura from '@primeuix/themes/aura';
import { getCurrentWindow } from '@tauri-apps/api/window';

createApp(App).use(i18n).use(PrimeVue, {
    theme: {
        preset: Aura,
        options: {
            prefix: 'p',
            darkModeSelector: 'system',
            cssLayer: {
                name: 'primevue',
                order: 'tailwind-base, primevue, tailwind-utilities',
            },
        },
    },
}).mount('#app').$nextTick(() => {
    getCurrentWindow().show();
});
