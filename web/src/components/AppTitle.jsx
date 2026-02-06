import { useEffect } from 'react';

// 환경 감지: dev.mdflare.com 또는 localhost면 dev
const isDev = () => {
  const host = window.location.hostname;
  return host.startsWith('dev.') || host === 'localhost' || host === '127.0.0.1';
};

// 앱 타이틀 설정 hook
export function useAppTitle(subtitle = '') {
  useEffect(() => {
    const prefix = isDev() ? 'dev.' : '';
    const base = `${prefix}MDFlare 🔥`;
    document.title = subtitle ? `${subtitle} | ${base}` : base;
  }, [subtitle]);
}

// 앱 이름 반환 (UI 표시용)
export function getAppName() {
  const prefix = isDev() ? 'dev.' : '';
  return `${prefix}MDFlare`;
}

export { isDev };
