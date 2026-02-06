import React, { useState, useEffect } from 'react';
import { useNavigate } from 'react-router-dom';
import { loginWithGoogle } from '../firebase';

export default function Landing({ user, username }) {
  const navigate = useNavigate();
  const [showPrivateVault, setShowPrivateVault] = useState(false);
  const [connectionToken, setConnectionToken] = useState('');
  const [connecting, setConnecting] = useState(false);
  const [error, setError] = useState('');

  // 연결 토큰 파싱: base64(serverUrl|token) 또는 plain token
  const parseConnectionToken = (input) => {
    try {
      const decoded = atob(input);
      if (decoded.includes('|')) {
        const [serverUrl, token] = decoded.split('|');
        return { serverUrl, token };
      }
    } catch {}
    // base64 아니면 기본 서버 + plain token
    return { serverUrl: 'http://localhost:7779', token: input };
  };

  const handleLogin = async () => {
    try {
      const result = await loginWithGoogle();
      const res = await fetch(`/api/username/resolve?uid=${result.user.uid}`);
      const data = await res.json();
      if (data.found) {
        navigate(`/${data.username}`);
      } else {
        navigate('/setup');
      }
    } catch (err) {
      console.error('Login failed:', err);
    }
  };

  const handlePrivateVaultConnect = async () => {
    setError('');
    setConnecting(true);
    
    try {
      console.log('[PV] 연결 시작, 토큰:', connectionToken.trim().substring(0, 20) + '...');
      
      const { serverUrl, token } = parseConnectionToken(connectionToken.trim());
      console.log('[PV] 파싱 결과:', { serverUrl, tokenLength: token?.length });
      
      // bore.pub 등 외부 터널은 프록시 통해 연결
      const isExternal = !serverUrl.includes('localhost') && !serverUrl.includes('127.0.0.1');
      console.log('[PV] 외부 서버 여부:', isExternal);
      
      const testUrl = isExternal 
        ? `/_tunnel?server=${encodeURIComponent(serverUrl.replace('http://', ''))}&path=/api/files`
        : `${serverUrl}/api/files`;
      console.log('[PV] 테스트 URL:', testUrl);
      
      // 서버 연결 테스트
      console.log('[PV] fetch 시작...');
      const res = await fetch(testUrl, {
        headers: token ? { 'Authorization': `Bearer ${token}` } : {}
      });
      console.log('[PV] fetch 응답:', res.status, res.statusText);
      
      if (!res.ok) {
        const text = await res.text();
        console.error('[PV] 응답 본문:', text);
        throw new Error(`서버 응답 ${res.status}: ${text}`);
      }
      
      const data = await res.json();
      console.log('[PV] 성공! 파일 수:', data.files?.length);
      
      // localStorage에 저장
      localStorage.setItem('mdflare_mode', 'private_vault');
      localStorage.setItem('mdflare_server_url', serverUrl);
      localStorage.setItem('mdflare_token', token);
      localStorage.setItem('mdflare_use_proxy', isExternal ? 'true' : 'false');
      
      // Private Vault 워크스페이스로 이동
      navigate('/local');
    } catch (err) {
      console.error('[PV] 연결 실패:', err);
      setError(`연결 실패: ${err.message}`);
    } finally {
      setConnecting(false);
    }
  };

  useEffect(() => {
    if (user && username) {
      navigate(`/${username}`);
    }
    
    // Private Vault 모드로 저장된 경우 자동 연결
    const savedMode = localStorage.getItem('mdflare_mode');
    if (savedMode === 'private_vault') {
      navigate('/local');
    }
  }, [user, username, navigate]);

  if (user && username) {
    return null;
  }

  return (
    <div className="landing">
      <nav className="landing-nav">
        <h1>🔥 MDFlare</h1>
        <div style={{ display: 'flex', gap: 12, alignItems: 'center' }}>
          <button className="nav-link" onClick={() => navigate('/download')}>다운로드</button>
          <button className="login-btn" onClick={handleLogin}>로그인</button>
        </div>
      </nav>

      <div className="hero">
        <h2>내 마크다운 폴더를<br/>웹에서 열다.</h2>
        <p className="hero-sub">
          로컬 마크다운 폴더가 곧 데이터베이스.<br/>
          별도 서버 없이, 어디서든 브라우저로 편집.
        </p>
        
        {!showPrivateVault ? (
          <div className="mode-buttons">
            <button className="cta-btn" onClick={handleLogin}>
              ☁️ Cloud로 시작하기
            </button>
            <button className="cta-btn secondary" onClick={() => setShowPrivateVault(true)}>
              🔐 Private Vault 연결
            </button>
          </div>
        ) : (
          <div className="private-vault-form">
            <h3>🔐 Private Vault 연결</h3>
            <p className="form-desc">에이전트에서 복사한 연결 토큰을 붙여넣으세요.</p>
            
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
                onClick={handlePrivateVaultConnect}
                disabled={connecting}
              >
                {connecting ? '연결 중...' : '연결하기'}
              </button>
              <button 
                className="cta-btn secondary" 
                onClick={() => setShowPrivateVault(false)}
              >
                취소
              </button>
            </div>
          </div>
        )}
      </div>

      <div className="features">
        <div className="feature-card">
          <span className="feature-icon">☁️</span>
          <h3>Cloud</h3>
          <p>Cloudflare에 저장, 어디서든 접속</p>
        </div>
        <div className="feature-card">
          <span className="feature-icon">🔐</span>
          <h3>Private Vault</h3>
          <p>내 PC에만 저장, 완전한 프라이버시</p>
        </div>
        <div className="feature-card">
          <span className="feature-icon">🔄</span>
          <h3>실시간 동기화</h3>
          <p>로컬에서 수정하면 즉시 반영</p>
        </div>
        <div className="feature-card">
          <span className="feature-icon">✏️</span>
          <h3>웹 에디터</h3>
          <p>브라우저에서 바로 편집</p>
        </div>
        <div className="feature-card">
          <span className="feature-icon">📱</span>
          <h3>모바일 지원</h3>
          <p>스마트폰에서도 편집 가능</p>
        </div>
        <div className="feature-card">
          <span className="feature-icon">🆓</span>
          <h3>오픈소스</h3>
          <p>AGPL-3.0 라이선스</p>
        </div>
      </div>

      <footer className="landing-footer">
        <p>© 2026 MDFlare · Built with Cloudflare</p>
        <p style={{ marginTop: 4, fontSize: 11, color: '#30363d' }}>v{__BUILD_VERSION__} · {__BUILD_TIME__}</p>
      </footer>
    </div>
  );
}
