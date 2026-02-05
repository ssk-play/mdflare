import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  define: {
    __BUILD_TIME__: JSON.stringify(new Date().toLocaleString('ko-KR', { timeZone: 'Asia/Seoul' })),
    __BUILD_VERSION__: JSON.stringify(process.env.npm_package_version || '0.1.0'),
    __LAST_CHANGE__: JSON.stringify('자기 폴더 드래그드롭 시 하이라이트/이동 완전 무시'),
  },
  plugins: [react()],
  server: {
    port: 3000,
    proxy: {
      '/api': 'http://localhost:3001'
    }
  }
});
