import React, { useState, useEffect, useCallback } from 'react';
import { auth } from '../firebase';

function getPrivateVaultConfig() {
  return {
    serverUrl: localStorage.getItem('mdflare_server_url') || 'http://localhost:7779',
    token: localStorage.getItem('mdflare_token') || '',
    useProxy: localStorage.getItem('mdflare_use_proxy') === 'true',
  };
}

export default function AgentStatus({ userId, isPrivateVault = false, onConnectionChange }) {
  const [connected, setConnected] = useState(null); // null = loading
  const [minutesAgo, setMinutesAgo] = useState(null);

  const check = useCallback(async () => {
    try {
      if (isPrivateVault) {
        // Private Vault: 직접 에이전트 서버에 ping
        const pv = getPrivateVaultConfig();
        let url;
        if (pv.useProxy) {
          const server = pv.serverUrl.replace('http://', '').replace('https://', '');
          url = `/_tunnel?server=${encodeURIComponent(server)}&path=${encodeURIComponent('/api/files')}`;
        } else {
          url = `${pv.serverUrl}/api/files`;
        }
        const headers = {};
        if (pv.token) headers['Authorization'] = `Bearer ${pv.token}`;
        const controller = new AbortController();
        const timer = setTimeout(() => controller.abort(), 3000);
        const res = await fetch(url, { headers, signal: controller.signal });
        clearTimeout(timer);
        const ok = res.ok;
        setConnected(ok);
        setMinutesAgo(null);
        if (onConnectionChange) onConnectionChange(ok);
      } else {
        // Cloud: API에서 heartbeat 조회
        const headers = {};
        if (auth.currentUser) {
          try {
            const idToken = await auth.currentUser.getIdToken();
            headers['Authorization'] = `Bearer ${idToken}`;
          } catch {}
        }
        const res = await fetch(`/api/${userId}/agent-status`, { headers });
        if (res.ok) {
          const data = await res.json();
          setConnected(data.connected);
          setMinutesAgo(data.minutesAgo);
          if (onConnectionChange) onConnectionChange(data.connected);
        }
      }
    } catch {
      if (isPrivateVault) {
        setConnected(false);
        if (onConnectionChange) onConnectionChange(false);
      }
    }
  }, [userId, isPrivateVault]);

  useEffect(() => {
    check();
    const interval = setInterval(check, isPrivateVault ? 5000 : 60000);
    return () => clearInterval(interval);
  }, [check, isPrivateVault]);

  if (connected === null) return null;

  const tooltip = !connected && minutesAgo != null
    ? `마지막 동기화: ${minutesAgo}분 전`
    : !connected
    ? '에이전트 연결 끊김'
    : '에이전트 연결됨';

  return (
    <span className={`agent-status-dot ${connected ? 'connected' : 'disconnected'}`} title={tooltip} />
  );
}
