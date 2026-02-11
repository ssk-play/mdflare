import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { readFileSync } from 'fs';

const version = readFileSync('../../VERSION', 'utf-8').trim();

export default defineConfig({
  define: {
    __BUILD_TIME__: JSON.stringify(new Date().toLocaleString('ko-KR', { timeZone: 'Asia/Seoul' })),
    __BUILD_VERSION__: JSON.stringify(version),
  },
  plugins: [react()],
  server: {
    port: 3000
  }
});
