import React, { useState, useEffect, useCallback } from 'react';
import { Routes, Route, useNavigate, useSearchParams } from 'react-router-dom';
import { useAppTitle, getAppName } from '@mdflare/common';
import Workspace from './pages/Workspace';

function PrivateLanding() {
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const [connectionToken, setConnectionToken] = useState('');
  const [connecting, setConnecting] = useState(false);
  const [error, setError] = useState('');

  const parseConnectionToken = (input) => {
    try {
      const decoded = atob(input);
      if (decoded.includes('|')) {
        const [serverUrl, token] = decoded.split('|');
        return { serverUrl, token };
      }
    } catch {}
    return { serverUrl: 'http://localhost:7779', token: input };
  };

  const connectPrivateVault = useCallback(async (inputToken) => {
    setError('');
    setConnecting(true);

    try {
      const { serverUrl, token } = parseConnectionToken(inputToken);
      const isExternal = !serverUrl.includes('localhost') && !serverUrl.includes('127.0.0.1');
      
      const testUrl = isExternal 
        ? `/_tunnel?server=${encodeURIComponent(serverUrl.replace(/^https?:\/\//, ''))}&path=/api/files`
        : `${serverUrl}/api/files`;
      
      const res = await fetch(testUrl, {
        headers: token ? { 'Authorization': `Bearer ${token}` } : {}
      });
      
      if (!res.ok) {
        const text = await res.text();
        throw new Error(`서버 응답 ${res.status}: ${text}`);
      }
      
      localStorage.setItem('mdflare_mode', 'private_vault');
      localStorage.setItem('mdflare_server_url', serverUrl);
      localStorage.setItem('mdflare_token', token);
      localStorage.setItem('mdflare_use_proxy', isExternal ? 'true' : 'false');
      
      navigate('/workspace');
    } catch (err) {
      setError(`연결 실패: ${err.message}`);
    } finally {
      setConnecting(false);
    }
  }, [navigate]);

  useEffect(() => {
    const savedMode = localStorage.getItem('mdflare_mode');
    if (savedMode === 'private_vault') {
      navigate('/workspace');
      return;
    }

    const pvtoken = searchParams.get('pvtoken');
    if (pvtoken) {
      connectPrivateVault(pvtoken);
    }
  }, [navigate, searchParams, connectPrivateVault]);

  return (
    <div className="landing">
      <nav className="landing-nav">
        <h1>🔐 {getAppName()} Private</h1>
        <div style={{ display: 'flex', gap: 12, alignItems: 'center' }}>
          <a href="https://mdflare.com" className="nav-link">홈</a>
        </div>
      </nav>

      <div className="hero">
        <h2>Private Vault</h2>
        <p className="hero-sub">
          내 PC에만 저장, 완전한 프라이버시.<br/>
          에이전트에서 복사한 연결 토큰을 붙여넣으세요.
        </p>
        
        <div className="private-vault-form" style={{ marginTop: 24 }}>
          <div className="form-group">
            <input
              type="text"
              value={connectionToken}
              onChange={(e) => setConnectionToken(e.target.value)}
              placeholder="연결 토큰 붙여넣기"
              autoFocus
              className="token-input"
            />
          </div>
          
          {error && <p className="form-error">{error}</p>}
          
          <div className="form-buttons">
            <button 
              className="cta-btn" 
              onClick={() => connectPrivateVault(connectionToken.trim())}
              disabled={connecting || !connectionToken.trim()}
            >
              {connecting ? '연결 중...' : '연결하기'}
            </button>
          </div>
        </div>
      </div>

      <footer className="landing-footer">
        <p>© 2026 MDFlare · Built with Cloudflare</p>
        <p style={{ marginTop: 4, fontSize: 11, color: '#30363d' }}>v{__BUILD_VERSION__} · {__BUILD_TIME__}</p>
      </footer>
    </div>
  );
}

export default function App() {
  useAppTitle('Private Vault');

  return (
    <Routes>
      <Route path="/" element={<PrivateLanding />} />
      <Route path="/workspace/*" element={<Workspace user={null} isPrivateVault={true} />} />
    </Routes>
  );
}
