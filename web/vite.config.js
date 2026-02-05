import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { execSync } from 'child_process';

const gitMsg = (() => {
  try { return execSync('git log -1 --format=%s', { encoding: 'utf-8' }).trim(); }
  catch { return ''; }
})();

export default defineConfig({
  define: {
    __BUILD_TIME__: JSON.stringify(new Date().toLocaleString('ko-KR', { timeZone: 'Asia/Seoul' })),
    __BUILD_VERSION__: JSON.stringify(process.env.npm_package_version || '0.1.0'),
    __BUILD_COMMIT_MSG__: JSON.stringify(gitMsg),
  },
  plugins: [react()],
  server: {
    port: 3000,
    proxy: {
      '/api': 'http://localhost:3001'
    }
  }
});
