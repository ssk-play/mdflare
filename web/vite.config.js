import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { readFileSync } from 'fs';

const version = readFileSync('../VERSION', 'utf-8').trim();

export default defineConfig({
  define: {
    __BUILD_TIME__: JSON.stringify(new Date().toLocaleString('ko-KR', { timeZone: 'Asia/Seoul' })),
    __BUILD_VERSION__: JSON.stringify(version),
    __LAST_CHANGE__: JSON.stringify('Rust 에이전트 + 동적 다운로드'),
  },
  plugins: [react()],
  server: {
    port: 3000,
    proxy: {
      '/api': 'http://localhost:3001'
    }
  }
});
