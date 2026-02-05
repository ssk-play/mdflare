import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  define: {
    __BUILD_TIME__: JSON.stringify(new Date().toLocaleString('ko-KR', { timeZone: 'Asia/Seoul' })),
    __BUILD_VERSION__: JSON.stringify(process.env.npm_package_version || '0.1.0'),
    __LAST_CHANGE__: JSON.stringify('키보드 단축키(Ctrl+S/B/Esc) + 글자/단어/줄 수 표시'),
  },
  plugins: [react()],
  server: {
    port: 3000,
    proxy: {
      '/api': 'http://localhost:3001'
    }
  }
});
